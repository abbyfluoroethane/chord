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
//!   CHORD_SERVER     "srv" (default), "starttls://host:port", or "tcp://host:port" (no TLS)

use std::process::ExitCode;
use std::time::Duration;

use chord_core::jid::{BareJid, Jid};
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, Session, SessionConfig, SessionEvent, Stream};
use chord_core::xmpp_parsers::iq::Iq;
use chord_core::xmpp_parsers::message::Message;
use chord_core::xmpp_parsers::ping::Ping;
use chord_core::xmpp_parsers::presence::Presence;
use chord_core::xmpp_parsers::stanza::Stanza;

/// `tokio-xmpp` retries a failed login forever, so the CLI stops waiting after this time.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const USAGE: &str = "usage: chord-cli login | send <jid> <text> | listen [--once]";

type Events = <NativeSession as Session>::Events;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["login"] => login().await,
        ["send", to, text] => send(to, text).await,
        ["listen"] => listen(false).await,
        ["listen", "--once"] => listen(true).await,
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
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
    Ok(SessionConfig {
        jid,
        password,
        server,
    })
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
        "tcp" => Ok(ServerAddr::InsecureTcp { host, port }),
        "starttls" => Ok(ServerAddr::StartTls { host, port }),
        _ => Err(format!("CHORD_SERVER scheme must be tcp or starttls: {s}")),
    }
}

/// Start a session and wait for `Connected`.
async fn connect() -> Result<(NativeSession, Events, Jid), String> {
    let config = config()?;
    let account = config.jid.clone();
    let mut session = NativeSession::connect(config)
        .await
        .map_err(|e| e.to_string())?;
    let mut events = session.events().ok_or("no event stream")?;
    let wait = async {
        while let Some(event) = next(&mut events).await {
            if let SessionEvent::Connected { bound_jid, .. } = event {
                return Some(bound_jid);
            }
        }
        None
    };
    match tokio::time::timeout(CONNECT_TIMEOUT, wait).await {
        Ok(Some(bound_jid)) => Ok((session, events, bound_jid)),
        Ok(None) => Err("session closed before login".to_owned()),
        Err(_) => {
            session.disconnect().await;
            Err(format!(
                "no login as {account} after {}s. Examine the password and CHORD_SERVER.",
                CONNECT_TIMEOUT.as_secs()
            ))
        }
    }
}

async fn login() -> Result<(), String> {
    let (session, _events, bound_jid) = connect().await?;
    println!("logged in as {bound_jid}");
    session.disconnect().await;
    Ok(())
}

async fn send(to: &str, text: &str) -> Result<(), String> {
    let to = Jid::new(to).map_err(|e| format!("bad JID {to}: {e}"))?;
    let (session, mut events, bound_jid) = connect().await?;
    let message = Message::chat(to.clone()).with_body("".into(), text.to_owned());
    session
        .send(message.into())
        .await
        .map_err(|e| e.to_string())?;

    // The server handles stanzas in order. When the ping reply arrives, the server has the message.
    let ping_id = "chord-cli-ping";
    let server = Jid::new(bound_jid.domain().as_str()).map_err(|e| e.to_string())?;
    let ping = Iq::from_get(ping_id, Ping).with_to(server);
    session.send(ping.into()).await.map_err(|e| e.to_string())?;
    let acked = tokio::time::timeout(CONNECT_TIMEOUT, async {
        while let Some(event) = next(&mut events).await {
            if let SessionEvent::Stanza(stanza) = event
                && let Stanza::Iq(iq) = *stanza
                && iq.id() == ping_id
            {
                return true;
            }
        }
        false
    })
    .await;
    session.disconnect().await;
    match acked {
        Ok(true) => {
            println!("sent to {to}: {text}");
            Ok(())
        }
        _ => Err(format!(
            "no confirmation from the server for the message to {to}"
        )),
    }
}

async fn listen(once: bool) -> Result<(), String> {
    let (session, mut events, bound_jid) = connect().await?;
    // Initial presence, so that the server routes chat messages to this resource.
    session
        .send(Presence::available().into())
        .await
        .map_err(|e| e.to_string())?;
    println!("listening as {bound_jid}");
    while let Some(event) = next(&mut events).await {
        match event {
            SessionEvent::Stanza(stanza) => {
                if let Stanza::Message(m) = *stanza
                    && let Some((_, body)) = m.get_best_body(vec![])
                {
                    let from = m.from.as_ref().map(|j| j.to_string()).unwrap_or_default();
                    println!("{from}: {body}");
                    if once {
                        break;
                    }
                }
            }
            SessionEvent::Connected { resumed, .. } => println!("reconnected (resumed: {resumed})"),
            SessionEvent::Disconnected(reason) => println!("disconnected: {reason:?}"),
        }
    }
    session.disconnect().await;
    Ok(())
}

async fn next(events: &mut Events) -> Option<SessionEvent> {
    std::future::poll_fn(|cx| std::pin::Pin::new(&mut *events).poll_next(cx)).await
}
