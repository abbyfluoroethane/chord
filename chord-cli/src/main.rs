//! Command-line client on `chord-core`.
//!
//! Usage:
//!   chord-cli login
//!   chord-cli send <jid> <text>
//!   chord-cli listen [--once]
//!
//! Environment:
//!   CHORD_JID        account, for example alice@chord.localhost
//!   CHORD_PASSWORD   password (never an argument, so it stays out of the shell history)
//!   CHORD_SERVER     "srv" (default) or "starttls://host:port". With the dev-insecure
//!                    feature also "tcp://host:port" (no TLS).
//!   SSL_CERT_FILE    optional PEM file of trusted CAs. It replaces the system trust store.
//!   CHORD_DB         account database (default: ~/.local/share/chord/<jid>.sqlite3)
//!   CHORD_LOG        log level on stderr: error, warn, info, debug, or trace (default: no log)
//!
//! Exit codes:
//!   0  success
//!   1  other error (bad usage, bad environment variable, send failure)
//!   2  login rejected (wrong username or password, account disabled, ...)
//!   3  server unreachable, or its TLS certificate is invalid
//!   4  login timed out

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use chord_core::actor::{
    self, ClientEvent, ClientEvents, ClientHandle, ConnectionState, LoginError,
};
use chord_core::jid::{BareJid, Jid};
use chord_core::session::native::NativeSession;
use chord_core::session::{ConnectError, ServerAddr, SessionConfig, Stream};
use chord_core::store::Store;
use tokio::task::JoinHandle;

/// `connect` returns after the login, so `Connected` must arrive at once. This is a safety limit.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const USAGE: &str = "usage: chord-cli login | send <jid> <text> | listen [--once]";

/// An error, and the exit code that goes with it.
enum CliError {
    Connect(ConnectError),
    Other(String),
}

impl CliError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Connect(ConnectError::AuthFailed(_)) => 2,
            Self::Connect(ConnectError::Unreachable(_) | ConnectError::TlsInvalid(_)) => 3,
            Self::Connect(ConnectError::Timeout) => 4,
            Self::Other(_) => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(e) => e.fmt(f),
            Self::Other(msg) => f.write_str(msg),
        }
    }
}

impl From<ConnectError> for CliError {
    fn from(e: ConnectError) -> Self {
        Self::Connect(e)
    }
}

impl From<String> for CliError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

/// A minimal logger on stderr. `CHORD_LOG` sets the level.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

fn init_logger() {
    let Ok(value) = std::env::var("CHORD_LOG") else {
        return;
    };
    match value.parse::<log::LevelFilter>() {
        Ok(level) if level != log::LevelFilter::Off => {
            if log::set_logger(&StderrLogger).is_ok() {
                log::set_max_level(level);
            }
        }
        _ => eprintln!("warning: CHORD_LOG must be error, warn, info, debug, or trace"),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    init_logger();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["login"] => login().await,
        ["send", to, text] => send(to, text).await,
        ["listen"] => listen(false).await,
        ["listen", "--once"] => listen(true).await,
        _ => Err(USAGE.to_owned().into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

fn config() -> Result<SessionConfig, String> {
    let jid = std::env::var("CHORD_JID").map_err(|_| "set CHORD_JID".to_owned())?;
    let jid = BareJid::new(&jid).map_err(|e| format!("CHORD_JID is not a bare JID: {e}"))?;
    let password = std::env::var("CHORD_PASSWORD").map_err(|_| "set CHORD_PASSWORD".to_owned())?;
    let server = match std::env::var("CHORD_SERVER").ok().as_deref() {
        None | Some("srv") => ServerAddr::Srv,
        Some(s) => parse_server(s)?,
    };
    Ok(SessionConfig::new(jid, password, server))
}

fn parse_server(s: &str) -> Result<ServerAddr, String> {
    let (scheme, rest) = s
        .split_once("://")
        .ok_or(format!("bad CHORD_SERVER: {s}"))?;
    let (host, port) = rest
        .rsplit_once(':')
        .ok_or(format!("CHORD_SERVER needs a port: {s}"))?;
    let host = host.to_owned();
    let port = port
        .parse()
        .map_err(|_| format!("bad port in CHORD_SERVER: {s}"))?;
    match scheme {
        #[cfg(feature = "dev-insecure")]
        "tcp" => Ok(ServerAddr::InsecureTcp { host, port }),
        #[cfg(not(feature = "dev-insecure"))]
        "tcp" => Err(format!(
            "plain TCP needs a build with --features dev-insecure: {s}"
        )),
        "starttls" => Ok(ServerAddr::StartTls { host, port }),
        _ => Err(format!("CHORD_SERVER scheme must be tcp or starttls: {s}")),
    }
}

/// The path of the account database: `CHORD_DB`, or `~/.local/share/chord/<jid>.sqlite3`.
fn db_path(jid: &BareJid) -> Result<PathBuf, CliError> {
    if let Some(path) = std::env::var_os("CHORD_DB") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").ok_or("set HOME or CHORD_DB".to_owned())?;
    let dir = PathBuf::from(home).join(".local/share/chord");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir.join(format!("{jid}.sqlite3")))
}

/// Start the actor, log in, and wait for `Connected`.
async fn start_client() -> Result<(ClientHandle, ClientEvents, JoinHandle<()>, Jid), CliError> {
    let config = config()?;
    let path = db_path(&config.jid)?;
    let store = Store::open(&path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let (handle, mut events, actor) = actor::new::<NativeSession>(store, config.jid.clone())
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let task = tokio::spawn(actor.run());

    handle.login(config).await.map_err(|e| match e {
        LoginError::Connect(e) => CliError::Connect(e),
        LoginError::ActorGone => CliError::Other(e.to_string()),
    })?;
    let wait = async {
        while let Some(event) = next(&mut events).await {
            if let ClientEvent::ConnectionState(ConnectionState::Connected { bound_jid, .. }) =
                event
            {
                return Some(bound_jid);
            }
        }
        None
    };
    match tokio::time::timeout(CONNECT_TIMEOUT, wait).await {
        Ok(Some(bound_jid)) => Ok((handle, events, task, bound_jid)),
        Ok(None) => Err("session closed before login".to_owned().into()),
        Err(_) => Err(ConnectError::Timeout.into()),
    }
}

/// Log out, stop the actor, and wait for it.
async fn stop_client(handle: ClientHandle, task: JoinHandle<()>) -> Result<(), CliError> {
    // Logout waits until the server answers a ping, so the server has every stanza.
    let logged_out = tokio::time::timeout(CONNECT_TIMEOUT, handle.logout()).await;
    drop(handle);
    let _ = tokio::time::timeout(CONNECT_TIMEOUT, task).await;
    logged_out.map_err(|_| "no answer from the server at logout".to_owned().into())
}

async fn login() -> Result<(), CliError> {
    let (handle, _events, task, bound_jid) = start_client().await?;
    println!("logged in as {bound_jid}");
    stop_client(handle, task).await
}

async fn send(to: &str, text: &str) -> Result<(), CliError> {
    let to = Jid::new(to).map_err(|e| format!("bad JID {to}: {e}"))?;
    let (handle, _events, task, _) = start_client().await?;
    let id = handle
        .send_chat(to.clone(), text.to_owned())
        .await
        .map_err(|e| e.to_string())?;
    stop_client(handle, task)
        .await
        .map_err(|_| format!("no confirmation from the server for the message to {to}"))?;
    println!("sent to {to}: {text} (origin-id {id})");
    Ok(())
}

async fn listen(once: bool) -> Result<(), CliError> {
    let (handle, mut events, task, bound_jid) = start_client().await?;
    println!("listening as {bound_jid}");
    let mut result = Ok(());
    while let Some(event) = next(&mut events).await {
        match event {
            ClientEvent::MessageReceived(message) => {
                println!("{}: {}", message.sender, message.body);
                if once {
                    break;
                }
            }
            ClientEvent::ConnectionState(ConnectionState::AuthFailed(failure)) => {
                result = Err(ConnectError::AuthFailed(failure).into());
                break;
            }
            ClientEvent::ConnectionState(ConnectionState::Connected { resumed, .. }) => {
                println!("reconnected (resumed: {resumed})");
            }
            ClientEvent::ConnectionState(ConnectionState::Disconnected) => break,
            ClientEvent::ConnectionState(state) => println!("connection: {state:?}"),
            ClientEvent::Notice(notice) => println!("notice: {notice}"),
        }
    }
    let stopped = stop_client(handle, task).await;
    result.and(stopped)
}

async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| std::pin::Pin::new(&mut *stream).poll_next(cx)).await
}
