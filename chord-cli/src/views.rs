//! The view commands: spaces, channels, members, timeline, and state.
//!
//! A command subscribes to a view. For a snapshot it applies the diffs until the list
//! is quiet, then prints the list. `timeline --follow` prints each diff as it arrives,
//! so the output is what a UI gets.

use std::time::{Duration, Instant};

use chord_core::jid::BareJid;
use chord_core::views::{ChannelScope, ListDiff, ViewItem, ViewStream, diff};

use crate::json::{Obj, array};
use crate::show::{Show, diff_human, diff_json};
use crate::{CliError, Client, Opts, next};

/// A list is complete when no diff arrives for this time.
const QUIET: Duration = Duration::from_millis(1500);
/// A snapshot waits at most this long.
const MAX_WAIT: Duration = Duration::from_secs(15);

/// Apply diffs until the list is quiet. Offline, the store gives the list at once.
async fn snapshot<T: ViewItem>(stream: &mut ViewStream<T>, online: bool) -> Vec<T> {
    let mut list = Vec::new();
    let quiet = if online {
        QUIET
    } else {
        Duration::from_millis(50)
    };
    let start = Instant::now();
    while start.elapsed() < MAX_WAIT {
        match tokio::time::timeout(quiet, next(stream)).await {
            Ok(Some(d)) => diff::apply(&mut list, &[d]),
            Ok(None) | Err(_) => break,
        }
    }
    list
}

fn print_list<T: Show>(opts: &Opts, title: &str, items: &[T]) {
    if opts.json {
        println!("{}", array(items.iter().map(Show::json)));
    } else {
        println!("{title} ({})", items.len());
        for item in items {
            println!("  {}", item.human());
        }
    }
}

pub async fn spaces(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let mut stream = client
        .handle
        .space_list()
        .await
        .map_err(|e| e.to_string())?;
    let items = snapshot(&mut stream, client.online).await;
    print_list(opts, "spaces", &items);
    Ok(())
}

/// `channels` for Home, or `channels <service> <node>` for a space.
pub async fn channels(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let scope = match args {
        [] | ["home"] => ChannelScope::Home,
        [service, node] => ChannelScope::Space {
            service: (*service).to_owned(),
            node: (*node).to_owned(),
        },
        _ => {
            return Err("usage: channels [home | <service> <node>]"
                .to_owned()
                .into());
        }
    };
    let mut stream = client
        .handle
        .channel_list(scope)
        .await
        .map_err(|e| e.to_string())?;
    let items = snapshot(&mut stream, client.online).await;
    print_list(opts, "channels", &items);
    Ok(())
}

pub async fn members(opts: &Opts, client: &Client, room: &str) -> Result<(), CliError> {
    let room = BareJid::new(room).map_err(|e| format!("bad JID {room}: {e}"))?;
    let mut stream = client
        .handle
        .member_list(room)
        .await
        .map_err(|e| e.to_string())?;
    let items = snapshot(&mut stream, client.online).await;
    print_list(opts, "members", &items);
    Ok(())
}

/// `timeline <jid> [--limit N] [--follow]`.
pub async fn timeline(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: timeline <jid> [--limit N] [--follow]".to_owned());
    let (room, mut rest) = args.split_first().ok_or_else(usage)?;
    let room = BareJid::new(room).map_err(|e| format!("bad JID {room}: {e}"))?;
    let (mut limit, mut follow) = (None, false);
    while let Some((arg, tail)) = rest.split_first() {
        match *arg {
            "--follow" => follow = true,
            "--limit" => {
                let (n, t) = tail.split_first().ok_or_else(usage)?;
                limit = Some(n.parse::<usize>().map_err(|_| usage())?);
                rest = t;
                continue;
            }
            _ => return Err(usage()),
        }
        rest = tail;
    }

    let mut timeline = client
        .handle
        .timeline(room)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(limit) = limit
        && limit > chord_core::views::DEFAULT_TIMELINE_WINDOW
    {
        timeline
            .paginate_back(limit - chord_core::views::DEFAULT_TIMELINE_WINDOW)
            .map_err(|e| e.to_string())?;
    }
    if !follow {
        let mut items = snapshot(&mut timeline.stream, client.online).await;
        if let Some(limit) = limit {
            let skip = items.len().saturating_sub(limit);
            items.drain(..skip);
        }
        print_list(opts, "messages", &items);
        return Ok(());
    }
    // Print each diff until the stream ends (logout, or Ctrl-C).
    while let Some(d) = next(&mut timeline.stream).await {
        print_diff(opts, &d);
    }
    Ok(())
}

fn print_diff<T: Show>(opts: &Opts, d: &ListDiff<T>) {
    if opts.json {
        println!("{}", diff_json(d));
    } else {
        println!("{}", diff_human(d));
    }
}

/// The full view state: the spaces, the Home channels, and the channels of each space.
pub async fn state(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let err = |e: chord_core::actor::ClientError| CliError::from(e.to_string());
    let mut spaces_stream = client.handle.space_list().await.map_err(err)?;
    let mut home_stream = client
        .handle
        .channel_list(ChannelScope::Home)
        .await
        .map_err(err)?;
    let spaces = snapshot(&mut spaces_stream, client.online).await;
    let home = snapshot(&mut home_stream, false).await;
    let mut per_space = Vec::new();
    for space in &spaces {
        let scope = ChannelScope::Space {
            service: space.service.clone(),
            node: space.node.clone(),
        };
        let mut stream = client.handle.channel_list(scope).await.map_err(err)?;
        per_space.push((space.clone(), snapshot(&mut stream, false).await));
    }
    if opts.json {
        let spaces_json = per_space.iter().map(|(space, channels)| {
            Obj::new()
                .raw("space", &space.json())
                .raw("channels", &array(channels.iter().map(Show::json)))
                .finish()
        });
        let json = Obj::new()
            .str("account", client.account.as_str())
            .bool("online", client.online)
            .raw("home", &array(home.iter().map(Show::json)))
            .raw("spaces", &array(spaces_json))
            .finish();
        println!("{json}");
    } else {
        println!(
            "account {} ({})",
            client.account,
            if client.online { "online" } else { "offline" }
        );
        print_list(opts, "home", &home);
        for (space, channels) in &per_space {
            print_list(opts, &format!("space {}", space.human()), channels);
        }
    }
    Ok(())
}
