//! Calls: the STUN and TURN servers (XEP-0215) and the call messages (XEP-0353). There is
//! no media, so a call ends at `proceed` and `finish`. The file also holds `adhoc`, which
//! a push app server needs for registration.

use std::time::Duration;

use chord_core::actor::ClientEvent;
use chord_core::features::extdisco::IceKind;
use chord_core::features::jmi::{CallEnd, CallEvent, CallReason};
use chord_core::jid::BareJid;

use crate::json::{Obj, array};
use crate::{CliError, Client, Opts, next};

/// How long `call` and `call-answer` wait for the other side.
const WAIT: Duration = Duration::from_secs(60);
/// How long the caller waits for the `finish` of the callee after `proceed`.
const FINISH_WAIT: Duration = Duration::from_secs(10);

fn err(e: impl std::fmt::Display) -> CliError {
    CliError::from(e.to_string())
}

/// `ice [--secrets]`: the STUN and TURN servers of the server (XEP-0215). The username
/// and the password stay hidden unless `--secrets` is set.
pub async fn ice(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let secrets = match args {
        [] => false,
        ["--secrets"] => true,
        _ => return Err("usage: ice [--secrets]".to_owned().into()),
    };
    let servers = client.handle.ice_servers().await.map_err(err)?;
    let shown = |v: &Option<String>| match (v, secrets) {
        (Some(v), true) => v.clone(),
        (Some(_), false) => "<hidden>".to_owned(),
        (None, _) => "-".to_owned(),
    };
    if opts.json {
        let items = servers.iter().map(|s| {
            let mut o = Obj::new()
                .str("type", &s.type_name)
                .str("host", &s.host)
                .opt_str("uri", s.uri().as_deref())
                .opt_str("transport", s.transport.as_deref())
                .bool("restricted", s.restricted)
                .opt_str(
                    "username",
                    s.username.as_ref().map(|_| shown(&s.username)).as_deref(),
                )
                .opt_str(
                    "password",
                    s.password.as_ref().map(|_| shown(&s.password)).as_deref(),
                );
            if let Some(port) = s.port {
                o = o.num("port", i64::from(port));
            }
            if let Some(expires) = s.expires_ms {
                o = o.num("expiresMs", expires);
            }
            o.finish()
        });
        println!("{}", array(items));
        return Ok(());
    }
    println!("ice servers ({})", servers.len());
    for s in &servers {
        let kind = match s.kind {
            IceKind::Stun => "stun",
            IceKind::Turn => "turn",
            IceKind::Other => s.type_name.as_str(),
        };
        println!(
            "  {kind} {} restricted={} username={} password={} expires_ms={}",
            s.uri().unwrap_or_else(|| s.host.clone()),
            s.restricted,
            shown(&s.username),
            shown(&s.password),
            s.expires_ms.map_or("-".to_owned(), |e| e.to_string()),
        );
    }
    Ok(())
}

/// `adhoc <jid> <node> [name=value ...]`: run a one-step ad-hoc command (XEP-0050) and print
/// the fields of the result. A push app server uses it for registration.
pub async fn adhoc(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: adhoc <jid> <node> [name=value ...]".to_owned());
    let [to, node, rest @ ..] = args else {
        return Err(usage());
    };
    let to =
        chord_core::jid::Jid::new(to).map_err(|e| CliError::from(format!("bad JID {to}: {e}")))?;
    let fields = rest
        .iter()
        .map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .ok_or_else(usage)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = client
        .handle
        .execute_command(to, (*node).to_owned(), fields)
        .await
        .map_err(err)?;
    if opts.json {
        let items = result
            .iter()
            .map(|(k, v)| Obj::new().str("name", k).str("value", v).finish());
        println!("{}", array(items));
    } else {
        println!("command {node} completed ({} fields)", result.len());
        for (name, value) in &result {
            println!("  {name}={value}");
        }
    }
    Ok(())
}

fn describe(event: &CallEvent) -> String {
    match event {
        CallEvent::Incoming { sid, from, media } => {
            format!("incoming {sid} from {from} media={}", media.join("+"))
        }
        CallEvent::Ringing { sid, from } => format!("ringing {sid} at {from}"),
        CallEvent::Proceeded { sid, from } => format!("proceeded {sid} by {from}"),
        CallEvent::Ended {
            sid,
            peer,
            end,
            reason,
        } => format!(
            "ended {sid} with {peer}: {end:?}{}",
            reason.as_ref().map_or(String::new(), |r| format!(" ({r})"))
        ),
    }
}

/// The next call event. Other events go by. `None` on timeout or when the stream ends.
async fn next_call(client: &mut Client, limit: Duration) -> Option<CallEvent> {
    let wait = async {
        while let Some(event) = next(&mut client.events).await {
            if let ClientEvent::Call(event) = event {
                return Some(event);
            }
        }
        None
    };
    tokio::time::timeout(limit, wait).await.ok().flatten()
}

/// `call <jid> [audio|video] [--retract-after <secs>] [--finish]`: propose a call and print
/// what happens. With `--retract-after`, retract if nobody proceeded by then. With
/// `--finish`, finish the call right after `proceed` and wait for the peer to see it.
pub async fn call(client: &mut Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::from(
            "usage: call <jid> [audio|video] [--retract-after <secs>] [--finish]".to_owned(),
        )
    };
    let (to, mut rest) = args.split_first().ok_or_else(usage)?;
    let to = BareJid::new(to).map_err(|e| CliError::from(format!("bad JID {to}: {e}")))?;
    let mut media = "audio".to_owned();
    let mut retract_after = None;
    let mut finish = false;
    while let Some((arg, tail)) = rest.split_first() {
        rest = tail;
        match *arg {
            "audio" | "video" => media = (*arg).to_owned(),
            "--finish" => finish = true,
            "--retract-after" => {
                let (secs, tail) = rest.split_first().ok_or_else(usage)?;
                rest = tail;
                retract_after = Some(Duration::from_secs(secs.parse().map_err(|_| usage())?));
            }
            _ => return Err(usage()),
        }
    }
    let sid = client
        .handle
        .propose_call(to.clone(), vec![media])
        .await
        .map_err(err)?;
    println!("proposed {sid} to {to}");
    let started = tokio::time::Instant::now();
    let mut proceeded = false;
    loop {
        let limit = match (retract_after, proceeded) {
            (Some(after), false) => after.saturating_sub(started.elapsed()),
            (_, true) => FINISH_WAIT,
            _ => WAIT,
        };
        let Some(event) = next_call(client, limit).await else {
            if !proceeded && retract_after.is_some() {
                client.handle.retract_call(sid.clone()).await.map_err(err)?;
                println!("retracted {sid}");
                return Ok(());
            }
            println!("no more events");
            return Ok(());
        };
        println!("{}", describe(&event));
        match event {
            CallEvent::Proceeded { .. } => {
                proceeded = true;
                if finish {
                    client
                        .handle
                        .finish_call(sid.clone(), Some(CallReason::Success))
                        .await
                        .map_err(err)?;
                    println!("finished {sid}");
                    return Ok(());
                }
            }
            CallEvent::Ended { .. } => return Ok(()),
            _ => {}
        }
    }
}

/// `call-answer accept|reject [reason] [--ring]`: wait for an incoming call, then answer it.
/// With `--ring`, send `ringing` first. After `accept`, wait for the `finish` of the caller.
pub async fn call_answer(client: &mut Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: call-answer accept|reject [reason] [--ring]".to_owned());
    let ring = args.contains(&"--ring");
    let args: Vec<&str> = args.iter().copied().filter(|a| *a != "--ring").collect();
    let (action, rest) = args.split_first().ok_or_else(usage)?;
    let reason = match rest {
        [] => None,
        [r] => Some(CallReason::parse(r).ok_or_else(usage)?),
        _ => return Err(usage()),
    };
    let sid = loop {
        match next_call(client, WAIT).await {
            Some(event @ CallEvent::Incoming { .. }) => {
                println!("{}", describe(&event));
                if let CallEvent::Incoming { sid, .. } = event {
                    break sid;
                }
            }
            Some(event) => println!("{}", describe(&event)),
            None => return Err("no incoming call".to_owned().into()),
        }
    };
    if ring {
        client.handle.ring_call(sid.clone()).await.map_err(err)?;
        println!("ringing {sid}");
    }
    match *action {
        "accept" => {
            client.handle.accept_call(sid.clone()).await.map_err(err)?;
            println!("accepted {sid}");
            // Wait for the caller to finish the call.
            while let Some(event) = next_call(client, FINISH_WAIT).await {
                println!("{}", describe(&event));
                if matches!(
                    event,
                    CallEvent::Ended {
                        end: CallEnd::Finished,
                        ..
                    }
                ) {
                    break;
                }
            }
        }
        "reject" => {
            client
                .handle
                .reject_call(sid.clone(), reason)
                .await
                .map_err(err)?;
            println!("rejected {sid}");
        }
        _ => return Err(usage()),
    }
    Ok(())
}

/// `call-watch [--secs N]`: print call events. For a test with a peer that retracts.
pub async fn call_watch(client: &mut Client, args: &[&str]) -> Result<(), CliError> {
    let secs = match args {
        [] => 30,
        ["--secs", n] => n
            .parse()
            .map_err(|_| CliError::from("usage: call-watch [--secs N]".to_owned()))?,
        _ => return Err("usage: call-watch [--secs N]".to_owned().into()),
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while let Some(event) = next_call(client, deadline - tokio::time::Instant::now()).await {
        println!("{}", describe(&event));
        if matches!(event, CallEvent::Ended { .. }) {
            break;
        }
    }
    Ok(())
}
