//! Commands that change something: rooms, uploads, spaces, and contacts.

use std::path::Path;

use chord_core::features::muc::{RoomAffiliation, RoomSettings};
use chord_core::features::roster::Subscription;
use chord_core::features::spaces::{JoinOutcome, SpaceAccess};
use chord_core::jid::{BareJid, Jid};

use crate::json::{Obj, array};
use crate::{CliError, Client, Opts};

fn bare(s: &str) -> Result<BareJid, CliError> {
    BareJid::new(s).map_err(|e| CliError::from(format!("bad JID {s}: {e}")))
}

fn err(e: impl std::fmt::Display) -> CliError {
    CliError::from(e.to_string())
}

/// `join <room> [--nick N]`: join the room, and bookmark it with autojoin, so that it
/// stays in Home and the next session joins it again. A room password comes from
/// CHORD_ROOM_PASSWORD, never from an argument.
pub async fn join(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: join <room> [--nick N]".to_owned());
    let (room, nick) = match args {
        [room] => (*room, None),
        [room, "--nick", nick] => (*room, Some((*nick).to_owned())),
        _ => return Err(usage()),
    };
    let room = bare(room)?;
    let nick = nick.unwrap_or_else(|| {
        client
            .account
            .node()
            .map_or_else(|| "chord".to_owned(), |n| n.to_string())
    });
    let password = std::env::var("CHORD_ROOM_PASSWORD").ok();
    client
        .handle
        .join_room(room.clone(), nick.clone(), password)
        .await
        .map_err(err)?;
    client
        .handle
        .add_bookmark(room.clone(), None, true, Some(nick.clone()))
        .await
        .map_err(err)?;
    println!("joined {room} as {nick}");
    Ok(())
}

/// `nick <room> <nick>`: change our nick in a room.
pub async fn nick(client: &Client, room: &str, nick: &str) -> Result<(), CliError> {
    let room = bare(room)?;
    client
        .handle
        .change_nick(room.clone(), nick.to_owned())
        .await
        .map_err(err)?;
    println!("now {nick} in {room}");
    Ok(())
}

/// `leave <room>`: leave the room and remove its bookmark.
pub async fn leave(client: &Client, room: &str) -> Result<(), CliError> {
    let room = bare(room)?;
    client.handle.leave_room(room.clone()).await.map_err(err)?;
    client
        .handle
        .remove_bookmark(room.clone())
        .await
        .map_err(err)?;
    println!("left {room}");
    Ok(())
}

/// `upload <jid> <file>`: upload the file (XEP-0363) and send its URL to the JID.
pub async fn upload(client: &Client, to: &str, file: &str) -> Result<(), CliError> {
    let to = Jid::new(to).map_err(|e| format!("bad JID {to}: {e}"))?;
    let path = Path::new(file);
    let data = std::fs::read(path).map_err(|e| format!("cannot read {file}: {e}"))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("bad file name: {file}"))?
        .to_owned();
    let content_type = content_type(&filename).to_owned();
    let url = client
        .handle
        .upload(to.clone(), filename, content_type, data)
        .await
        .map_err(err)?;
    println!("sent {url} to {to}");
    Ok(())
}

/// A content type from the file extension. The upload service needs one.
fn content_type(filename: &str) -> &'static str {
    let ext = filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("txt" | "log" | "md") => "text/plain",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("ogg" | "opus") => "audio/ogg",
        _ => "application/octet-stream",
    }
}

/// `space-browse`: the public spaces on the pubsub service.
pub async fn space_browse(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let spaces = client.handle.browse_spaces().await.map_err(err)?;
    if opts.json {
        let items = spaces.iter().map(|s| {
            Obj::new()
                .str("service", &s.service)
                .str("node", &s.node)
                .str("name", &s.name)
                .opt_str("description", s.description.as_deref())
                .opt_str("access_model", s.access_model.as_deref())
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("public spaces ({})", spaces.len());
        for s in &spaces {
            println!("  {}  ({} {})", s.name, s.service, s.node);
        }
    }
    Ok(())
}

/// `space-info <service> <node>`: read a space without joining it.
pub async fn space_info(
    opts: &Opts,
    client: &Client,
    service: &str,
    node: &str,
) -> Result<(), CliError> {
    let c = client.handle.space_info(service, node).await.map_err(err)?;
    if opts.json {
        println!(
            "{}",
            Obj::new()
                .str("service", &c.service)
                .str("node", &c.node)
                .str("name", &c.name)
                .opt_str("description", c.description.as_deref())
                .opt_str("access_model", c.access_model.as_deref())
                .opt_num("channels", c.channels.map(|n| n as i64))
                .finish()
        );
    } else {
        println!("{} ({} {})", c.name, c.service, c.node);
        if let Some(d) = &c.description {
            println!("  {d}");
        }
        if let Some(n) = c.channels {
            println!("  {n} channels");
        }
    }
    Ok(())
}

/// `room-info <room>`: read a room with a disco#info query, without joining it.
pub async fn room_info(opts: &Opts, client: &Client, room: &str) -> Result<(), CliError> {
    let c = client.handle.room_info(bare(room)?).await.map_err(err)?;
    if opts.json {
        println!(
            "{}",
            Obj::new()
                .str("jid", &c.jid)
                .opt_str("name", c.name.as_deref())
                .opt_str("description", c.description.as_deref())
                .opt_str("subject", c.subject.as_deref())
                .opt_num("occupants", c.occupants.map(i64::from))
                .bool("password_protected", c.password_protected)
                .bool("members_only", c.members_only)
                .finish()
        );
    } else {
        println!("{} ({})", c.name.as_deref().unwrap_or("no name"), c.jid);
        if let Some(s) = &c.subject {
            println!("  subject: {s}");
        }
        if let Some(n) = c.occupants {
            println!("  {n} people");
        }
    }
    Ok(())
}

pub async fn space_join(client: &Client, service: &str, node: &str) -> Result<(), CliError> {
    match client.handle.join_space(service, node).await.map_err(err)? {
        JoinOutcome::Joined => println!("joined space {service} {node}"),
        JoinOutcome::Pending => println!("the owner of {service} {node} must approve the join"),
    }
    Ok(())
}

/// `space-create <name> [--private | --authorize]`.
pub async fn space_create(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (name, access) = match args {
        [name] => (*name, SpaceAccess::Open),
        [name, "--private"] => (*name, SpaceAccess::Whitelist),
        [name, "--authorize"] => (*name, SpaceAccess::Authorize),
        _ => {
            return Err("usage: space-create <name> [--private | --authorize]"
                .to_owned()
                .into());
        }
    };
    let (service, node) = client
        .handle
        .create_space_with(name, access)
        .await
        .map_err(err)?;
    if opts.json {
        println!(
            "{}",
            Obj::new()
                .str("service", &service)
                .str("node", &node)
                .finish()
        );
    } else {
        println!("created space {name}: {service} {node}");
    }
    Ok(())
}

/// `space-add-room <service> <node> <room> [name]`.
pub async fn space_add_room(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (service, node, room, name) = match args {
        [s, n, r] => (*s, *n, *r, None),
        [s, n, r, name] => (*s, *n, *r, Some(*name)),
        _ => {
            return Err("usage: space-add-room <service> <node> <room> [name]"
                .to_owned()
                .into());
        }
    };
    let room = bare(room)?;
    let name = name.map_or_else(
        || room.node().map_or_else(String::new, |n| n.to_string()),
        str::to_owned,
    );
    client
        .handle
        .add_room_to_space(service, node, room.clone(), &name)
        .await
        .map_err(err)?;
    println!("added {room} to {service} {node}");
    Ok(())
}

pub async fn space_add_member(
    client: &Client,
    service: &str,
    node: &str,
    member: &str,
) -> Result<(), CliError> {
    let member = bare(member)?;
    client
        .handle
        .add_space_member(service, node, member.clone())
        .await
        .map_err(err)?;
    println!("{member} is a member of {service} {node}");
    Ok(())
}

/// `space-leave <service> <node>`: leave a space, or cancel a join that waits for approval.
pub async fn space_leave(client: &Client, service: &str, node: &str) -> Result<(), CliError> {
    client
        .handle
        .leave_space(service, node)
        .await
        .map_err(err)?;
    println!("left space {service} {node}");
    Ok(())
}

/// `space-delete <service> <node>`: delete a space that we own.
pub async fn space_delete(client: &Client, service: &str, node: &str) -> Result<(), CliError> {
    client
        .handle
        .delete_space(service, node)
        .await
        .map_err(err)?;
    println!("deleted space {service} {node}");
    Ok(())
}

/// `contacts`: the roster.
pub async fn contacts(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let contacts = client.handle.contacts().await.map_err(err)?;
    let sub = |s: Subscription| match s {
        Subscription::None => "none",
        Subscription::To => "to",
        Subscription::From => "from",
        Subscription::Both => "both",
    };
    if opts.json {
        let items = contacts.iter().map(|c| {
            Obj::new()
                .str("jid", c.jid.as_str())
                .opt_str("name", c.name.as_deref())
                .str("subscription", sub(c.subscription))
                .bool("ask", c.ask)
                .bool("blocked", c.blocked)
                .raw(
                    "groups",
                    &array(c.groups.iter().map(|g| Obj::new().str("name", g).finish())),
                )
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("contacts ({})", contacts.len());
        for c in &contacts {
            let name = c.name.as_deref().unwrap_or("");
            let ask = if c.ask { ", asked" } else { "" };
            let blocked = if c.blocked { ", blocked" } else { "" };
            println!("  {} {name} ({}{ask}{blocked})", c.jid, sub(c.subscription));
        }
    }
    Ok(())
}

/// `contact-add <jid> [name]`: add the contact and ask to see its presence.
pub async fn contact_add(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (jid, name) = match args {
        [jid] => (*jid, None),
        [jid, name] => (*jid, Some((*name).to_owned())),
        _ => return Err("usage: contact-add <jid> [name]".to_owned().into()),
    };
    let jid = bare(jid)?;
    client
        .handle
        .add_contact(jid.clone(), name)
        .await
        .map_err(err)?;
    println!("added {jid} and asked to see their presence");
    Ok(())
}

/// `block <jid>`: block an address (XEP-0191).
pub async fn block(client: &Client, jid: &str) -> Result<(), CliError> {
    let jid = bare(jid)?;
    client
        .handle
        .block_contact(jid.clone())
        .await
        .map_err(err)?;
    println!("blocked {jid}");
    Ok(())
}

/// `unblock <jid>` or `unblock --all`.
pub async fn unblock(client: &Client, args: &[&str]) -> Result<(), CliError> {
    match args {
        ["--all"] => {
            client.handle.unblock_all().await.map_err(err)?;
            println!("unblocked everyone");
        }
        [jid] => {
            let jid = bare(jid)?;
            client
                .handle
                .unblock_contact(jid.clone())
                .await
                .map_err(err)?;
            println!("unblocked {jid}");
        }
        _ => return Err("usage: unblock <jid|--all>".to_owned().into()),
    }
    Ok(())
}

/// `blocked`: the blocked addresses in the store. Works offline.
pub async fn blocked(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let list = client.handle.blocked_contacts().await.map_err(err)?;
    if opts.json {
        let items = list
            .iter()
            .map(|j| Obj::new().str("jid", j.as_str()).finish());
        println!("{}", array(items));
    } else {
        println!("blocked ({})", list.len());
        for jid in &list {
            println!("  {jid}");
        }
    }
    Ok(())
}

/// `edit <item-id> <text>`: correct an own message (XEP-0308).
pub async fn edit(client: &Client, item: &str, text: &str) -> Result<(), CliError> {
    client
        .handle
        .edit_message(item.to_owned(), text.to_owned())
        .await
        .map_err(err)?;
    println!("edited {item}");
    Ok(())
}

/// `retract <item-id>`: retract an own message (XEP-0424).
pub async fn retract(client: &Client, item: &str) -> Result<(), CliError> {
    client
        .handle
        .retract_message(item.to_owned())
        .await
        .map_err(err)?;
    println!("retracted {item}");
    Ok(())
}

/// `react <item-id> [emoji...]`: set the own reactions to a message (XEP-0444). With no
/// emoji, remove them.
pub async fn react(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (item, emojis) = args
        .split_first()
        .ok_or_else(|| CliError::from("usage: react <item-id> [emoji...]".to_owned()))?;
    let emojis: Vec<String> = emojis.iter().map(|e| (*e).to_owned()).collect();
    client
        .handle
        .react((*item).to_owned(), emojis.clone())
        .await
        .map_err(err)?;
    if emojis.is_empty() {
        println!("removed the reactions to {item}");
    } else {
        println!("reacted to {item} with {}", emojis.join(" "));
    }
    Ok(())
}

/// `reply <item-id> <text>`: reply to a message (XEP-0461).
pub async fn reply(client: &Client, item: &str, text: &str) -> Result<(), CliError> {
    client
        .handle
        .reply(item.to_owned(), text.to_owned())
        .await
        .map_err(err)?;
    println!("replied to {item}");
    Ok(())
}

/// `read <jid>`: mark the chat or room as read. Online, it also sends a XEP-0333
/// displayed marker.
pub async fn read(client: &Client, peer: &str) -> Result<(), CliError> {
    let peer = bare(peer)?;
    client.handle.mark_read(peer.clone()).await.map_err(err)?;
    println!("marked {peer} as read");
    Ok(())
}

/// `typing <jid> on|off`: send a XEP-0085 typing state.
pub async fn typing(client: &Client, peer: &str, state: &str) -> Result<(), CliError> {
    let on = match state {
        "on" => true,
        "off" => false,
        other => return Err(format!("typing: expected on or off, got {other}").into()),
    };
    client.handle.set_typing(peer.to_owned(), on).map_err(err)?;
    println!("typing {state} for {peer}");
    Ok(())
}

/// `read-private <room> <nick>`: mark a private chat with a room occupant as read.
pub async fn read_private(client: &Client, room: &str, nick: &str) -> Result<(), CliError> {
    let room = bare(room)?;
    client
        .handle
        .mark_read_private(room.clone(), nick.to_owned())
        .await
        .map_err(err)?;
    println!("marked {room}/{nick} as read");
    Ok(())
}

/// `moderate <item-id> [reason]`: ask the room to retract a message (XEP-0425). We must
/// be a moderator of the room.
pub async fn moderate(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (item, reason) = match args {
        [item] => (*item, None),
        [item, reason] => (*item, Some((*reason).to_owned())),
        _ => return Err("usage: moderate <item-id> [reason]".to_owned().into()),
    };
    client
        .handle
        .moderate_message(item.to_owned(), reason)
        .await
        .map_err(err)?;
    println!("asked the room to retract {item}");
    Ok(())
}

fn affiliation(word: &str) -> Result<RoomAffiliation, CliError> {
    Ok(match word {
        "owner" => RoomAffiliation::Owner,
        "admin" => RoomAffiliation::Admin,
        "member" => RoomAffiliation::Member,
        "none" => RoomAffiliation::None,
        "outcast" => RoomAffiliation::Outcast,
        _ => return Err(format!("unknown affiliation {word}").into()),
    })
}

/// `room-member <room> <jid> [member|admin|owner|none|outcast]`: set an affiliation.
/// The default is member.
pub async fn room_member(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (room, jid, word) = match args {
        [room, jid] => (*room, *jid, "member"),
        [room, jid, word] => (*room, *jid, *word),
        _ => {
            return Err(
                "usage: room-member <room> <jid> [member|admin|owner|none|outcast]"
                    .to_owned()
                    .into(),
            );
        }
    };
    client
        .handle
        .set_room_affiliation(bare(room)?, bare(jid)?, affiliation(word)?, None)
        .await
        .map_err(err)?;
    println!("{jid} is now {word} of {room}");
    Ok(())
}

/// `room-members <room> [affiliation]`: list the JIDs with one affiliation (member).
pub async fn room_members(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (room, word) = match args {
        [room] => (*room, "member"),
        [room, word] => (*room, *word),
        _ => return Err("usage: room-members <room> [affiliation]".to_owned().into()),
    };
    let list = client
        .handle
        .room_affiliations(bare(room)?, affiliation(word)?)
        .await
        .map_err(err)?;
    for (jid, nick) in list {
        match nick {
            Some(nick) => println!("{jid} ({nick})"),
            None => println!("{jid}"),
        }
    }
    Ok(())
}

/// `invite <room> <jid> [reason]`: invite a JID. An owner or admin adds it as a member.
pub async fn invite(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (room, jid, reason) = match args {
        [room, jid] => (*room, *jid, None),
        [room, jid, reason] => (*room, *jid, Some((*reason).to_owned())),
        _ => return Err("usage: invite <room> <jid> [reason]".to_owned().into()),
    };
    client
        .handle
        .invite_to_room(bare(room)?, bare(jid)?, reason)
        .await
        .map_err(err)?;
    println!("invited {jid} to {room}");
    Ok(())
}

/// `room-config <room> [--name N] [--public|--private] [--members-only|--open]`.
pub async fn room_config(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::from(
            "usage: room-config <room> [--name N] [--public|--private] [--members-only|--open]"
                .to_owned(),
        )
    };
    let Some((room, mut rest)) = args.split_first() else {
        return Err(usage());
    };
    let mut settings = RoomSettings::default();
    while let Some((flag, tail)) = rest.split_first() {
        rest = tail;
        match *flag {
            "--name" => {
                let Some((name, tail)) = rest.split_first() else {
                    return Err(usage());
                };
                settings.name = Some((*name).to_owned());
                rest = tail;
            }
            "--public" => settings.public = Some(true),
            "--private" => settings.public = Some(false),
            "--members-only" => settings.members_only = Some(true),
            "--open" => settings.members_only = Some(false),
            _ => return Err(usage()),
        }
    }
    client
        .handle
        .configure_room(bare(room)?, settings)
        .await
        .map_err(err)?;
    println!("configured {room}");
    Ok(())
}

/// `pm <room> <nick> <text>`: send a private message to a room occupant.
pub async fn pm(client: &Client, room: &str, nick: &str, text: &str) -> Result<(), CliError> {
    let room = bare(room)?;
    client
        .handle
        .send_private(room.clone(), nick.to_owned(), text.to_owned())
        .await
        .map_err(err)?;
    println!("sent to {room}/{nick}: {text}");
    Ok(())
}

/// `push-enable <service> <node>`: enable push notifications (XEP-0357). The app server
/// secret comes from CHORD_PUSH_SECRET, never from an argument.
pub async fn push_enable(client: &Client, service: &str, node: &str) -> Result<(), CliError> {
    let service = Jid::new(service).map_err(|e| format!("bad JID {service}: {e}"))?;
    let form = std::env::var("CHORD_PUSH_SECRET")
        .ok()
        .map(|secret| vec![("secret".to_owned(), secret)]);
    client
        .handle
        .enable_push(service.clone(), node.to_owned(), form)
        .await
        .map_err(err)?;
    println!("enabled push to {service} {node}");
    Ok(())
}

/// `push-disable <service> [node]`.
pub async fn push_disable(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (service, node) = match args {
        [service] => (*service, None),
        [service, node] => (*service, Some((*node).to_owned())),
        _ => return Err("usage: push-disable <service> [node]".to_owned().into()),
    };
    let service = Jid::new(service).map_err(|e| format!("bad JID {service}: {e}"))?;
    client
        .handle
        .disable_push(service.clone(), node)
        .await
        .map_err(err)?;
    println!("disabled push to {service}");
    Ok(())
}

/// `push-list`: the push registrations in the store.
pub async fn push_list(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let registrations = client.handle.push_registrations().await.map_err(err)?;
    if opts.json {
        let items = registrations.iter().map(|r| {
            Obj::new()
                .str("service", &r.service)
                .str("node", &r.node)
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("push registrations ({})", registrations.len());
        for r in &registrations {
            println!("  {} {}", r.service, r.node);
        }
    }
    Ok(())
}

/// `notify <jid> [all|mentions|none [--until <unix-ms>]]`: set the notification level of a
/// chat, room, or occupant (room@service/nick). Without a level, show it. Works offline.
pub async fn notify(client: &Client, args: &[&str]) -> Result<(), CliError> {
    use chord_core::features::notify::NotificationLevel;
    let usage =
        || CliError::from("usage: notify <jid> [all|mentions|none [--until <unix-ms>]]".to_owned());
    let (peer, rest) = args.split_first().ok_or_else(usage)?;
    let (level, until) = match rest {
        [] => {
            let now = client
                .handle
                .notification_level((*peer).to_owned())
                .await
                .map_err(err)?;
            println!(
                "{peer}: {} (muted until {:?})",
                now.level.as_str(),
                now.mute_until
            );
            return Ok(());
        }
        [level] => (level, None),
        [level, "--until", ms] => (level, Some(ms.parse::<i64>().map_err(|_| usage())?)),
        _ => return Err(usage()),
    };
    let level = NotificationLevel::parse(level).ok_or_else(usage)?;
    client
        .handle
        .set_notification_level((*peer).to_owned(), level, until)
        .await
        .map_err(err)?;
    println!("{peer}: {}", level.as_str());
    Ok(())
}

/// `space-pending`: the spaces that wait for the owner to approve our join.
pub async fn space_pending(opts: &Opts, client: &Client) -> Result<(), CliError> {
    let list = client.handle.pending_space_joins().await.map_err(err)?;
    if opts.json {
        let items = list.iter().map(|(service, node, name)| {
            Obj::new()
                .str("service", service)
                .str("node", node)
                .str("name", name)
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("pending joins ({})", list.len());
        for (service, node, name) in &list {
            println!("  {name}  ({service} {node})");
        }
    }
    Ok(())
}

/// `space-requests <service> <node>`: the join requests for a space that we own.
pub async fn space_requests(
    opts: &Opts,
    client: &Client,
    service: &str,
    node: &str,
) -> Result<(), CliError> {
    let list = client
        .handle
        .space_join_requests(service, node)
        .await
        .map_err(err)?;
    if opts.json {
        let items = list.iter().map(|r| {
            Obj::new()
                .str("jid", &r.jid)
                .opt_str("subid", r.subid.as_deref())
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("join requests ({})", list.len());
        for r in &list {
            println!("  {}", r.jid);
        }
    }
    Ok(())
}

/// `space-approve` and `space-deny <service> <node> <jid>`.
pub async fn space_answer(
    client: &Client,
    service: &str,
    node: &str,
    jid: &str,
    approve: bool,
) -> Result<(), CliError> {
    if approve {
        client.handle.approve_space_join(service, node, jid).await
    } else {
        client.handle.deny_space_join(service, node, jid).await
    }
    .map_err(err)?;
    println!(
        "{} {jid} for {service} {node}",
        if approve { "approved" } else { "denied" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::content_type;

    #[test]
    fn content_type_from_extension() {
        assert_eq!(content_type("cat.PNG"), "image/png");
        assert_eq!(content_type("notes.txt"), "text/plain");
        assert_eq!(content_type("blob"), "application/octet-stream");
    }
}
