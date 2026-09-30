//! Command-line client on `chord-core`.
//!
//! Usage: chord-cli [--json] [--offline] <command>
//!   login
//!   send <jid> <text>
//!   listen [--once]
//!   spaces                          the space rail
//!   channels [home | <service> <node>]
//!   members <room>
//!   timeline <jid> [--limit N] [--follow]
//!   state                           spaces, Home channels, and the channels of each space
//!   join <room> [--nick N]          join and bookmark a room (password: CHORD_ROOM_PASSWORD)
//!   leave <room>
//!   upload <jid> <file>             XEP-0363 upload, then send the URL
//!   space-info <service> <node> | room-info <room>   read a space or a room, no join
//!   space-browse | space-join <service> <node> | space-create <name> [--private | --authorize]
//!   space-add-room <service> <node> <room> [name] | space-add-member <service> <node> <jid>
//!   space-delete <service> <node> | space-leave <service> <node> | space-pending
//!   space-requests <service> <node> | space-approve <service> <node> <jid> | space-deny ...
//!   contacts | contact-add <jid> [name]
//!   block <jid> | unblock <jid|--all> | blocked (blocked works --offline)
//!   edit <item-id> <text> | retract <item-id> | react <item-id> [emoji...]
//!   reply <item-id> <text>          <item-id> is the id in `timeline --json`
//!   read <jid>                      mark as read (also with --offline)
//!   pm <room> <nick> <text>         private message to a room occupant
//!   read-private <room> <nick>      mark a private chat as read (also with --offline)
//!   typing <jid> on|off             send a typing state (XEP-0085)
//!   moderate <item-id> [reason]     retract a message of another occupant (XEP-0425)
//!   nick <room> <nick>              change our nick in a room
//!   room-member <room> <jid> [member|admin|owner|none|outcast]   set an affiliation
//!   room-members <room> [affiliation] | invite <room> <jid> [reason]
//!   room-config <room> [--name N] [--public|--private] [--members-only|--open]
//!   notify <jid> [all|mentions|none [--until <unix-ms>]]   also with --offline
//!   presence [available|away|dnd|xa|invisible [status]]     show or set our presence (also with --offline)
//!   search <text> [--in <jid>]      search the stored messages (also with --offline)
//!   push-enable <service> <node>    secret: CHORD_PUSH_SECRET
//!   push-disable <service> [node] | push-list
//!
//! --json prints JSON. --offline reads the local database and does not log in.
//! `timeline --follow` prints each diff as it arrives, as a UI gets it.
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

mod actions;
mod json;
mod show;
mod views;

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
const USAGE: &str = "usage: chord-cli [--json] [--offline] login | send <jid> <text> | \
listen [--once] | spaces | channels [home | <service> <node>] | members <room> | \
timeline <jid> [--limit N] [--follow] | state | join <room> [--nick N] | leave <room> | \
upload <jid> <file> | space-info <service> <node> | room-info <room> | space-browse | space-join <service> <node> | \
space-create <name> [--private | --authorize] | space-add-room <service> <node> <room> [name] | \
space-add-member <service> <node> <jid> | space-delete <service> <node> | space-leave <service> <node> | space-pending | space-requests <service> <node> | \
space-approve <service> <node> <jid> | space-deny <service> <node> <jid> | contacts | \
block <jid> | unblock <jid|--all> | blocked | \
contact-add <jid> [name] | edit <item-id> <text> | retract <item-id> | \
react <item-id> [emoji...] | reply <item-id> <text> | read <jid> | pm <room> <nick> <text> | \
read-private <room> <nick> | typing <jid> on|off | moderate <item-id> [reason] | \
room-member <room> <jid> [member|admin|owner|none|outcast] | room-members <room> [affiliation] | \
invite <room> <jid> [reason] | room-config <room> [--name N] [--public|--private] [--members-only|--open] | \
push-enable <service> <node> | push-disable <service> [node] | push-list | \
notify <jid> [all|mentions|none [--until <unix-ms>]]";

/// Global options.
pub struct Opts {
    pub json: bool,
    pub offline: bool,
}

/// A running actor, logged in or not.
pub struct Client {
    pub handle: ClientHandle,
    pub events: ClientEvents,
    task: JoinHandle<()>,
    pub account: BareJid,
    pub online: bool,
    pub bound_jid: Option<Jid>,
}

/// An error, and the exit code that goes with it.
pub enum CliError {
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
    let mut opts = Opts {
        json: false,
        offline: false,
    };
    let args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| match *a {
            "--json" => {
                opts.json = true;
                false
            }
            "--offline" => {
                opts.offline = true;
                false
            }
            _ => true,
        })
        .collect();
    let result = run(&opts, &args).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

async fn run(opts: &Opts, args: &[&str]) -> Result<(), CliError> {
    let (command, rest) = args
        .split_first()
        .ok_or_else(|| CliError::from(USAGE.to_owned()))?;
    let known = [
        "login",
        "send",
        "listen",
        "spaces",
        "channels",
        "members",
        "timeline",
        "state",
        "join",
        "leave",
        "upload",
        "space-info",
        "room-info",
        "space-browse",
        "space-join",
        "space-create",
        "space-add-room",
        "space-add-member",
        "space-delete",
        "space-leave",
        "space-pending",
        "space-requests",
        "space-approve",
        "space-deny",
        "contacts",
        "block",
        "unblock",
        "blocked",
        "contact-add",
        "edit",
        "retract",
        "react",
        "reply",
        "read",
        "pm",
        "read-private",
        "typing",
        "moderate",
        "nick",
        "room-member",
        "room-members",
        "invite",
        "room-config",
        "push-enable",
        "push-disable",
        "push-list",
        "notify",
        "presence",
        "search",
    ];
    if !known.contains(command) {
        return Err(USAGE.to_owned().into());
    }
    let needs_session = !matches!(
        *command,
        "spaces"
            | "channels"
            | "members"
            | "timeline"
            | "state"
            | "read"
            | "read-private"
            | "typing"
            | "push-list"
            | "blocked"
            | "notify"
            | "presence"
            | "search"
            | "space-pending"
    );
    if needs_session && opts.offline {
        return Err(format!("{command} needs a session, not --offline").into());
    }
    let mut client = start_client(!opts.offline).await?;
    let result = match (*command, rest) {
        ("login", []) => {
            println!(
                "logged in as {}",
                client.bound_jid.as_ref().map_or("?".into(), Jid::to_string)
            );
            Ok(())
        }
        ("send", [to, text]) => send(&client, to, text).await,
        ("listen", []) => listen(&mut client, false).await,
        ("listen", ["--once"]) => listen(&mut client, true).await,
        ("spaces", []) => views::spaces(opts, &client).await,
        ("channels", args) => views::channels(opts, &client, args).await,
        ("members", [room]) => views::members(opts, &client, room).await,
        ("timeline", args) => views::timeline(opts, &client, args).await,
        ("state", []) => views::state(opts, &client).await,
        ("join", args) => actions::join(&client, args).await,
        ("leave", [room]) => actions::leave(&client, room).await,
        ("upload", [to, file]) => actions::upload(&client, to, file).await,
        ("space-info", [service, node]) => actions::space_info(opts, &client, service, node).await,
        ("room-info", [room]) => actions::room_info(opts, &client, room).await,
        ("space-browse", []) => actions::space_browse(opts, &client).await,
        ("space-join", [service, node]) => actions::space_join(&client, service, node).await,
        ("space-create", args) => actions::space_create(opts, &client, args).await,
        ("space-add-room", args) => actions::space_add_room(&client, args).await,
        ("space-add-member", [service, node, jid]) => {
            actions::space_add_member(&client, service, node, jid).await
        }
        ("space-delete", [service, node]) => actions::space_delete(&client, service, node).await,
        ("space-leave", [service, node]) => actions::space_leave(&client, service, node).await,
        ("space-pending", []) => actions::space_pending(opts, &client).await,
        ("space-requests", [service, node]) => {
            actions::space_requests(opts, &client, service, node).await
        }
        ("space-approve", [service, node, jid]) => {
            actions::space_answer(&client, service, node, jid, true).await
        }
        ("space-deny", [service, node, jid]) => {
            actions::space_answer(&client, service, node, jid, false).await
        }
        ("contacts", []) => actions::contacts(opts, &client).await,
        ("block", [jid]) => actions::block(&client, jid).await,
        ("unblock", args) => actions::unblock(&client, args).await,
        ("blocked", []) => actions::blocked(opts, &client).await,
        ("contact-add", args) => actions::contact_add(&client, args).await,
        ("edit", [item, text]) => actions::edit(&client, item, text).await,
        ("retract", [item]) => actions::retract(&client, item).await,
        ("react", args) => actions::react(&client, args).await,
        ("reply", [item, text]) => actions::reply(&client, item, text).await,
        ("read", [peer]) => actions::read(&client, peer).await,
        ("pm", [room, nick, text]) => actions::pm(&client, room, nick, text).await,
        ("typing", [peer, state]) => actions::typing(&client, peer, state).await,
        ("read-private", [room, nick]) => actions::read_private(&client, room, nick).await,
        ("moderate", args) => actions::moderate(&client, args).await,
        ("nick", [room, nick]) => actions::nick(&client, room, nick).await,
        ("room-member", args) => actions::room_member(&client, args).await,
        ("room-members", args) => actions::room_members(&client, args).await,
        ("invite", args) => actions::invite(&client, args).await,
        ("room-config", args) => actions::room_config(&client, args).await,
        ("push-enable", [service, node]) => actions::push_enable(&client, service, node).await,
        ("push-disable", args) => actions::push_disable(&client, args).await,
        ("notify", args) => actions::notify(&client, args).await,
        ("presence", args) => actions::presence(opts, &client, args).await,
        ("search", args) => actions::search(opts, &client, args).await,
        ("push-list", []) => actions::push_list(opts, &client).await,
        _ => Err(USAGE.to_owned().into()),
    };
    let stopped = stop_client(client).await;
    result.and(stopped)
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

/// Start the actor. With `login`, log in and wait for `Connected`.
async fn start_client(login: bool) -> Result<Client, CliError> {
    let jid = std::env::var("CHORD_JID").map_err(|_| "set CHORD_JID".to_owned())?;
    let account = BareJid::new(&jid).map_err(|e| format!("CHORD_JID is not a bare JID: {e}"))?;
    let path = db_path(&account)?;
    let store = Store::open(&path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let (handle, mut events, actor) = actor::new::<NativeSession>(store, account.clone())
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let task = tokio::spawn(actor.run());
    if !login {
        return Ok(Client {
            handle,
            events,
            task,
            account,
            online: false,
            bound_jid: None,
        });
    }

    handle.login(config()?).await.map_err(|e| match e {
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
        Ok(Some(bound_jid)) => Ok(Client {
            handle,
            events,
            task,
            account,
            online: true,
            bound_jid: Some(bound_jid),
        }),
        Ok(None) => Err("session closed before login".to_owned().into()),
        Err(_) => Err(ConnectError::Timeout.into()),
    }
}

/// Log out, stop the actor, and wait for it.
async fn stop_client(client: Client) -> Result<(), CliError> {
    let Client {
        handle,
        task,
        online,
        ..
    } = client;
    // Logout waits until the server answers a ping, so the server has every stanza.
    let logged_out = if online {
        tokio::time::timeout(CONNECT_TIMEOUT, handle.logout())
            .await
            .is_ok()
    } else {
        true
    };
    drop(handle);
    let _ = tokio::time::timeout(CONNECT_TIMEOUT, task).await;
    if logged_out {
        Ok(())
    } else {
        Err("no answer from the server at logout".to_owned().into())
    }
}

async fn send(client: &Client, to: &str, text: &str) -> Result<(), CliError> {
    let to = Jid::new(to).map_err(|e| format!("bad JID {to}: {e}"))?;
    let id = client
        .handle
        .send_chat(to.clone(), text.to_owned())
        .await
        .map_err(|e| e.to_string())?;
    println!("sent to {to}: {text} (origin-id {id})");
    Ok(())
}

async fn listen(client: &mut Client, once: bool) -> Result<(), CliError> {
    println!(
        "listening as {}",
        client.bound_jid.as_ref().map_or("?".into(), Jid::to_string)
    );
    while let Some(event) = next(&mut client.events).await {
        match event {
            ClientEvent::MessageReceived(message) => {
                println!("{}: {}", message.sender, message.body);
                if once {
                    break;
                }
            }
            ClientEvent::Typing { peer, typers } => {
                if typers.is_empty() {
                    println!("{peer}: nobody types");
                } else {
                    println!("{peer}: {} typing", typers.join(", "));
                }
            }
            ClientEvent::ConnectionState(ConnectionState::AuthFailed(failure)) => {
                return Err(ConnectError::AuthFailed(failure).into());
            }
            ClientEvent::ConnectionState(ConnectionState::Connected { resumed, .. }) => {
                println!("reconnected (resumed: {resumed})");
            }
            ClientEvent::ConnectionState(ConnectionState::Disconnected) => break,
            ClientEvent::ConnectionState(state) => println!("connection: {state:?}"),
            ClientEvent::Notification(n) => println!(
                "notify: {} in {}: {}{}",
                n.sender_name,
                n.peer,
                n.body_preview,
                if n.mention { " (mention)" } else { "" }
            ),
            ClientEvent::BlockListChanged => println!("blocklist changed"),
            ClientEvent::Notice(notice) => println!("notice: {notice}"),
            ClientEvent::SubscriptionRequest(jid) => println!("{jid} asks to see your presence"),
            ClientEvent::RoomInvite {
                room,
                from,
                reason,
                password,
            } => println!(
                "invite: {from} invites you to {room}{}{}",
                reason.map_or(String::new(), |r| format!(": {r}")),
                if password.is_some() {
                    " (password)"
                } else {
                    ""
                }
            ),
            other => log::debug!("event: {other:?}"),
        }
    }
    Ok(())
}

pub async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| std::pin::Pin::new(&mut *stream).poll_next(cx)).await
}
