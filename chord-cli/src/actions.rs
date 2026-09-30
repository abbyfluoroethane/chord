//! Commands that change something: rooms, uploads, spaces, and contacts.

use std::path::Path;

use chord_core::features::blocking::ReportReason;
use chord_core::features::muc::{RoomAffiliation, RoomRole, RoomSettings};
use chord_core::features::presence::{Availability, InvisibleMethod};
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
pub async fn join(client: &mut Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: join <room> [--nick N] [--share-password]".to_owned());
    let Some((room, mut rest)) = args.split_first() else {
        return Err(usage());
    };
    let mut nick = None;
    let mut share_password = false;
    while let Some((flag, tail)) = rest.split_first() {
        match *flag {
            "--nick" => {
                let (value, tail) = tail.split_first().ok_or_else(usage)?;
                nick = Some((*value).to_owned());
                rest = tail;
            }
            "--share-password" => {
                share_password = true;
                rest = tail;
            }
            _ => return Err(usage()),
        }
    }
    let room = bare(room)?;
    let password = std::env::var("CHORD_ROOM_PASSWORD").ok();
    // With no nick, the room may have reserved one for us (XEP-0045, 7.12).
    let joined = {
        let join = async {
            match &nick {
                Some(nick) => {
                    client
                        .handle
                        .join_room(room.clone(), nick.clone(), password)
                        .await
                }
                None => {
                    client
                        .handle
                        .join_room_default_nick(room.clone(), password, None)
                        .await
                }
            }
        };
        tokio::pin!(join);
        // A room can hold the join until we solve a CAPTCHA (XEP-0158).
        loop {
            tokio::select! {
                result = &mut join => break result,
                Some(event) = crate::next(&mut client.events) => {
                    if let chord_core::actor::ClientEvent::RoomCaptcha { room, form } = event {
                        crate::forms::answer_captcha(&client.handle, room, form).await?;
                    }
                }
            }
        }
    };
    if let Err(e) = &joined
        && e.condition() == Some("not-authorized")
    {
        return Err(format!(
            "{room} needs a password: set CHORD_ROOM_PASSWORD and try again ({e})"
        )
        .into());
    }
    joined.map_err(err)?;
    // The password goes into the bookmark only when the user asks for it.
    if share_password {
        client
            .handle
            .add_bookmark_with_password_choice(room.clone(), None, true, nick.clone(), true)
            .await
            .map_err(err)?;
    } else {
        client
            .handle
            .add_bookmark(room.clone(), None, true, nick.clone())
            .await
            .map_err(err)?;
    }
    match nick {
        Some(nick) => println!("joined {room} as {nick}"),
        None => println!("joined {room}"),
    }
    Ok(())
}

/// Join a room that we joined before, and wait for it, so that a room command that follows
/// finds us in the room. Each CLI command is a new session.
async fn enter(client: &Client, room: &BareJid) -> Result<(), CliError> {
    client
        .handle
        .join_room_default_nick(room.clone(), None, None)
        .await
        .map_err(err)
}

/// `subject <room> <text>`: set the subject of a room (an empty text clears it).
pub async fn subject(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let [room, text] = args else {
        return Err("usage: subject <room> <text>".to_owned().into());
    };
    let room = bare(room)?;
    enter(client, &room).await?;
    client
        .handle
        .set_room_subject(room.clone(), (*text).to_owned())
        .await
        .map_err(err)?;
    println!("the subject of {room} is now: {text}");
    Ok(())
}

/// `room-role <room> <nick> <none|visitor|participant|moderator> [reason]`: set the role
/// of an occupant. `none` kicks, `visitor` mutes, `participant` gives voice back.
pub async fn room_role(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (room, nick, word, reason) = match args {
        [room, nick, word] => (*room, *nick, *word, None),
        [room, nick, word, reason] => (*room, *nick, *word, Some((*reason).to_owned())),
        _ => {
            return Err(
                "usage: room-role <room> <nick> <none|visitor|participant|moderator> [reason]"
                    .to_owned()
                    .into(),
            );
        }
    };
    let role = match word {
        "none" => RoomRole::None,
        "visitor" => RoomRole::Visitor,
        "participant" => RoomRole::Participant,
        "moderator" => RoomRole::Moderator,
        _ => return Err(format!("unknown role {word}").into()),
    };
    let room = bare(room)?;
    enter(client, &room).await?;
    client
        .handle
        .set_room_role(room.clone(), nick.to_owned(), role, reason)
        .await
        .map_err(err)?;
    println!("{nick} is now {word} in {room}");
    Ok(())
}

/// `room-destroy <room> [reason] [--alternate <room>]`: destroy a room that we own.
pub async fn room_destroy(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage =
        || CliError::from("usage: room-destroy <room> [reason] [--alternate <room>]".to_owned());
    let (room, mut rest) = args.split_first().ok_or_else(usage)?;
    let mut reason = None;
    let mut alternate = None;
    while let Some((word, tail)) = rest.split_first() {
        rest = tail;
        if *word == "--alternate" {
            let (alt, tail) = rest.split_first().ok_or_else(usage)?;
            alternate = Some(bare(alt)?);
            rest = tail;
        } else if reason.is_none() {
            reason = Some((*word).to_owned());
        } else {
            return Err(usage());
        }
    }
    client
        .handle
        .destroy_room(bare(room)?, reason, alternate)
        .await
        .map_err(err)?;
    println!("destroyed {room}");
    Ok(())
}

/// `decline <room> <from-jid> [reason]`: decline an invitation (a mediated decline).
pub async fn decline(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (room, from, reason) = match args {
        [room, from] => (*room, *from, None),
        [room, from, reason] => (*room, *from, Some((*reason).to_owned())),
        _ => {
            return Err("usage: decline <room> <from-jid> [reason]"
                .to_owned()
                .into());
        }
    };
    client
        .handle
        .decline_room_invite(bare(room)?, bare(from)?, reason)
        .await
        .map_err(err)?;
    println!("declined the invitation from {from} to {room}");
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
    // The core retracts the bookmark of the room as it leaves.
    client.handle.leave_room(room.clone()).await.map_err(err)?;
    println!("left {room}");
    Ok(())
}

/// `upload <jid> <file>`: upload the file (XEP-0363) and send its URL to the JID.
pub async fn upload(client: &Client, to: &str, file: &str) -> Result<(), CliError> {
    let to = Jid::new(to).map_err(|e| format!("bad JID {to}: {e}"))?;
    let path = Path::new(file);
    let file_handle = std::fs::File::open(path).map_err(|e| format!("cannot read {file}: {e}"))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("bad file name: {file}"))?
        .to_owned();
    let content_type = content_type(&filename).to_owned();
    let url = client
        .handle
        .upload_file(to.clone(), filename, content_type, file_handle)
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
                .bool("change_subject", c.change_subject)
                .opt_str("anonymity", c.anonymity.as_deref())
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
        if c.change_subject {
            println!("  anyone may change the subject");
        }
        if let Some(a) = &c.anonymity {
            println!("  {a}");
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

/// `space-create <name> [--private | --authorize] [--description TEXT]`.
pub async fn space_create(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::from(
            "usage: space-create <name> [--private | --authorize] [--description TEXT]".to_owned(),
        )
    };
    let Some((name, mut rest)) = args.split_first() else {
        return Err(usage());
    };
    let mut access = SpaceAccess::Open;
    let mut description = None;
    while let Some((flag, tail)) = rest.split_first() {
        rest = tail;
        match *flag {
            "--private" => access = SpaceAccess::Whitelist,
            "--authorize" => access = SpaceAccess::Authorize,
            "--description" => {
                let (text, tail) = rest.split_first().ok_or_else(usage)?;
                description = Some(*text);
                rest = tail;
            }
            _ => return Err(usage()),
        }
    }
    let name = *name;
    let (service, node) = client
        .handle
        .create_space_described(name, description, access)
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

/// `space-members <service> <node>`: the affiliations of a space that we own.
pub async fn space_members(
    opts: &Opts,
    client: &Client,
    service: &str,
    node: &str,
) -> Result<(), CliError> {
    let list = client
        .handle
        .space_members(service, node)
        .await
        .map_err(err)?;
    if opts.json {
        let items = list.iter().map(|m| {
            Obj::new()
                .str("jid", &m.jid)
                .str("affiliation", &m.affiliation)
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("members ({})", list.len());
        for m in &list {
            println!("  {}  {}", m.jid, m.affiliation);
        }
    }
    Ok(())
}

/// `space-remove <service> <node> <jid>` and `space-ban <service> <node> <jid>`.
pub async fn space_unaffiliate(
    client: &Client,
    service: &str,
    node: &str,
    jid: &str,
    ban: bool,
) -> Result<(), CliError> {
    let member = bare(jid)?;
    if ban {
        client.handle.ban_space_member(service, node, member).await
    } else {
        client
            .handle
            .remove_space_member(service, node, member)
            .await
    }
    .map_err(err)?;
    println!(
        "{} {jid} for {service} {node}",
        if ban { "banned" } else { "removed" }
    );
    Ok(())
}

/// `space-config <service> <node>`: the node configuration form of a space that we own.
pub async fn space_config(
    opts: &Opts,
    client: &Client,
    service: &str,
    node: &str,
) -> Result<(), CliError> {
    let fields = client
        .handle
        .space_config(service, node)
        .await
        .map_err(err)?;
    if opts.json {
        let items = fields.iter().map(|f| {
            Obj::new()
                .str("var", &f.var)
                .str("value", &f.value)
                .finish()
        });
        println!("{}", array(items));
    } else {
        for f in &fields {
            println!("{} = {}", f.var, f.value);
        }
    }
    Ok(())
}

/// `space-set <service> <node> [--name N] [--description D]`: change a space that we own.
pub async fn space_set(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::from("usage: space-set <service> <node> [--name N] [--description D]".to_owned())
    };
    let [service, node, rest @ ..] = args else {
        return Err(usage());
    };
    let (mut name, mut description) = (None, None);
    let mut rest = rest;
    while let Some((flag, tail)) = rest.split_first() {
        let (value, tail) = tail.split_first().ok_or_else(usage)?;
        match *flag {
            "--name" => name = Some(*value),
            "--description" => description = Some(*value),
            _ => return Err(usage()),
        }
        rest = tail;
    }
    client
        .handle
        .configure_space(service, node, name, description)
        .await
        .map_err(err)?;
    println!("changed {service} {node}");
    Ok(())
}

/// `space-avatar <service> <node> <file>` and `space-banner ...`: upload an image and set
/// it as the avatar or the banner of a space that we own.
pub async fn space_image(
    client: &Client,
    service: &str,
    node: &str,
    file: &str,
    banner: bool,
) -> Result<(), CliError> {
    let data = std::fs::read(file).map_err(|e| format!("cannot read {file}: {e}"))?;
    let filename = Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("bad file name: {file}"))?;
    let mime = content_type(filename);
    if banner {
        client
            .handle
            .set_space_banner(service, node, mime, data, 0, 0)
            .await
    } else {
        client
            .handle
            .set_space_avatar(service, node, mime, data, 0, 0)
            .await
    }
    .map_err(err)?;
    println!(
        "set the {} of {service} {node}",
        if banner { "banner" } else { "avatar" }
    );
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
    // The core goes on in the background: it reads the room config form and sets the
    // space field (XEP-0503), and it grants the members of the space. Stay online for it.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
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
                .opt_str("idleSince", c.idle_since.as_deref())
                .opt_str("activity", c.activity.as_deref())
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
            let idle = c
                .idle_since
                .as_deref()
                .map_or(String::new(), |t| format!(", idle since {t}"));
            let playing = c
                .activity
                .as_deref()
                .map_or(String::new(), |t| format!(", playing {t}"));
            let online = if c.online { ", online" } else { "" };
            let groups = if c.groups.is_empty() {
                String::new()
            } else {
                format!(", groups {}", c.groups.join("/"))
            };
            println!(
                "  {} {name} ({}{ask}{blocked}{online}{idle}{playing}{groups})",
                c.jid,
                sub(c.subscription)
            );
        }
    }
    Ok(())
}

/// `contact-add <jid> [name] [--preauth TOKEN]`: add the contact and ask to see its
/// presence. The token is the XEP-0379 token of a `?roster;preauth=` link.
pub async fn contact_add(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = "usage: contact-add <jid> [name] [--preauth TOKEN]";
    let (args, preauth) = match args {
        [rest @ .., "--preauth", token] => (rest, Some((*token).to_owned())),
        rest => (rest, None),
    };
    let (jid, name) = match args {
        [jid] => (*jid, None),
        [jid, name] => (*jid, Some((*name).to_owned())),
        _ => return Err(usage.to_owned().into()),
    };
    let jid = bare(jid)?;
    match preauth {
        Some(token) => {
            client
                .handle
                .add_contact_with_preauth(jid.clone(), name, token)
                .await
        }
        None => client.handle.add_contact(jid.clone(), name).await,
    }
    .map_err(err)?;
    println!("added {jid} and asked to see their presence");
    Ok(())
}

/// `contact-approve <jid> [--add-back]`: accept the subscription request of `jid`. The
/// contact then sees our presence. With `--add-back` we also ask to see theirs.
pub async fn contact_approve(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (jid, add_back) = match args {
        [jid] => (*jid, false),
        [jid, "--add-back"] => (*jid, true),
        _ => {
            return Err("usage: contact-approve <jid> [--add-back]"
                .to_owned()
                .into());
        }
    };
    let jid = bare(jid)?;
    client
        .handle
        .approve_subscription_with(jid.clone(), add_back)
        .await
        .map_err(err)?;
    println!(
        "approved {jid}{}",
        if add_back { " and added back" } else { "" }
    );
    Ok(())
}

/// `contact-groups <jid> [group...]`: replace the groups of a contact. No group clears them.
pub async fn contact_groups(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let Some((jid, groups)) = args.split_first() else {
        return Err("usage: contact-groups <jid> [group...]".to_owned().into());
    };
    let jid = bare(jid)?;
    let groups: Vec<String> = groups.iter().map(|g| (*g).to_owned()).collect();
    client
        .handle
        .set_contact_groups(jid.clone(), groups.clone())
        .await
        .map_err(err)?;
    println!("groups of {jid}: {}", groups.join(", "));
    Ok(())
}

/// `contact-rename <jid> [name]`: give the contact a new name. Without a name, clear it.
pub async fn contact_rename(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (jid, name) = match args {
        [jid] => (*jid, None),
        [jid, name] => (*jid, Some((*name).to_owned())),
        _ => return Err("usage: contact-rename <jid> [name]".to_owned().into()),
    };
    let jid = bare(jid)?;
    client
        .handle
        .rename_contact(jid.clone(), name.clone())
        .await
        .map_err(err)?;
    println!(
        "renamed {jid} to {}",
        name.as_deref().unwrap_or("(no name)")
    );
    Ok(())
}

/// `idle <seconds-ago>|off [hold-secs]`: tell the contacts that we are idle since that
/// many seconds (XEP-0319), or not idle. Stay online afterwards, so that they see it.
pub async fn idle(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: idle <seconds-ago>|off [hold-secs]".to_owned());
    let (what, hold) = match args {
        [what] => (*what, 5),
        [what, hold] => (*what, hold.parse::<u64>().map_err(|_| usage())?),
        _ => return Err(usage()),
    };
    let since = if what == "off" {
        None
    } else {
        let ago: i64 = what.parse().map_err(|_| usage())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Some(now - ago)
    };
    client.handle.set_idle(since).await.map_err(err)?;
    match since {
        Some(_) => println!("idle for {what} seconds"),
        None => println!("not idle"),
    }
    tokio::time::sleep(std::time::Duration::from_secs(hold)).await;
    Ok(())
}

/// `block <jid> [--report spam|abuse]`: block an address (XEP-0191). With `--report` the
/// block carries a report (XEP-0377) when the server takes reports.
pub async fn block(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: block <jid> [--report spam|abuse]".to_owned());
    let (jid, reason) = match args {
        [jid] => (*jid, None),
        [jid, "--report", "spam"] => (*jid, Some(ReportReason::Spam)),
        [jid, "--report", "abuse"] => (*jid, Some(ReportReason::Abuse)),
        _ => return Err(usage()),
    };
    let jid = bare(jid)?;
    match reason {
        Some(reason) => {
            let reported = client
                .handle
                .block_and_report(jid.clone(), reason)
                .await
                .map_err(err)?;
            let how = if reported {
                "reported"
            } else {
                "no report, the server does not take them"
            };
            println!("blocked {jid} ({how})");
        }
        None => {
            client
                .handle
                .block_contact(jid.clone())
                .await
                .map_err(err)?;
            println!("blocked {jid}");
        }
    }
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

/// `presence [available|away|dnd|xa|invisible [status]]`: set our presence, then show it.
pub async fn presence(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    if let Some((first, status)) = args.split_first() {
        let availability = match *first {
            "available" => Availability::Available,
            "away" => Availability::Away,
            "dnd" => Availability::Dnd,
            "xa" => Availability::ExtendedAway,
            "invisible" => Availability::Invisible,
            other => return Err(format!("presence: unknown availability {other}").into()),
        };
        let status = (!status.is_empty()).then(|| status.join(" "));
        let invisible = availability == Availability::Invisible;
        let set = client.handle.set_presence(availability, status).await;
        // Say which mechanism hides us, or that the server has none.
        if invisible && !opts.offline {
            let method = client.handle.invisible_method().await.map_err(err)?;
            println!(
                "invisible via {}",
                match method {
                    Some(InvisibleMethod::Command) => "xep-0186 (invisible command)",
                    Some(InvisibleMethod::PrivacyList) => "xep-0016 (privacy list)",
                    None => "nothing (the server has no invisible mode)",
                }
            );
        }
        set.map_err(err)?;
    }
    let own = client.handle.own_presence().await.map_err(err)?;
    let name = match own.availability {
        Availability::Available => "available",
        Availability::Away => "away",
        Availability::Dnd => "dnd",
        Availability::ExtendedAway => "xa",
        Availability::Invisible => "invisible",
    };
    if opts.json {
        println!(
            "{}",
            Obj::new()
                .str("availability", name)
                .opt_str("status", own.status.as_deref())
                .finish()
        );
    } else {
        println!("{name} {}", own.status.as_deref().unwrap_or(""));
    }
    Ok(())
}

/// `search <text> [--in <jid>]`: search the stored messages, newest first.
pub async fn search(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (query, peer) = match args {
        [query] => (*query, None),
        [query, "--in", peer] => (*query, Some(bare(peer)?.to_string())),
        _ => return Err("usage: search <text> [--in <jid>]".to_owned().into()),
    };
    let hits = client
        .handle
        .search_messages(peer, query.to_owned(), 50)
        .await
        .map_err(err)?;
    if opts.json {
        let items = hits.iter().map(|h| {
            Obj::new()
                .str("id", &h.id)
                .str("peer", &h.peer)
                .str("sender", &h.sender)
                .str("body", &h.body)
                .finish()
        });
        println!("{}", array(items));
    } else {
        for h in &hits {
            println!("{} {} {}: {}", h.id, h.peer, h.sender, h.body);
        }
        println!("{} found", hits.len());
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

/// `typing <jid> on|off|gone`: send a XEP-0085 state. `gone` says that we closed the chat.
pub async fn typing(client: &Client, peer: &str, state: &str) -> Result<(), CliError> {
    let on = match state {
        "on" => true,
        "off" => false,
        "gone" => {
            client.handle.close_chat(peer.to_owned()).map_err(err)?;
            println!("typing gone for {peer}");
            return Ok(());
        }
        other => return Err(format!("typing: expected on, off or gone, got {other}").into()),
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

/// `invite <room> <jid> [reason] [--direct]`: invite a JID. An owner or admin adds it as a
/// member. With `--direct`, send a direct invitation (XEP-0249) for a room that does not
/// pass on invitations.
pub async fn invite(client: &mut Client, args: &[&str]) -> Result<(), CliError> {
    let direct = args.contains(&"--direct");
    let args: Vec<&str> = args.iter().copied().filter(|a| *a != "--direct").collect();
    let (room, jid, reason) = match args.as_slice() {
        [room, jid] => (*room, *jid, None),
        [room, jid, reason] => (*room, *jid, Some((*reason).to_owned())),
        _ => {
            return Err("usage: invite <room> <jid> [reason] [--direct]"
                .to_owned()
                .into());
        }
    };
    let room = bare(room)?;
    // The room takes an invitation from an occupant only, and an owner grants membership
    // only from inside the room.
    enter(client, &room).await?;
    if direct {
        client
            .handle
            .invite_to_room_direct(room.clone(), bare(jid)?, reason)
            .await
            .map_err(err)?;
        println!("sent {jid} a direct invitation to {room}");
        return Ok(());
    }
    client
        .handle
        .invite_to_room(room.clone(), bare(jid)?, reason)
        .await
        .map_err(err)?;
    println!("invited {jid} to {room}");
    // A room can refuse the invitation a moment later, and Chord then sends a direct one.
    // Show what it says.
    let end = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while let Ok(Some(event)) = tokio::time::timeout_at(end, crate::next(&mut client.events)).await
    {
        if let chord_core::actor::ClientEvent::Notice(notice) = event {
            println!("notice: {notice}");
        }
    }
    Ok(())
}

/// `room-config <room> [--name N] [--public|--private] [--members-only|--open]`.
pub async fn room_config(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::from(
            "usage: room-config <room> [--name N] [--public|--private] [--members-only|--open] \
             [--protect|--unprotect]"
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
            // The password comes from the environment, never from an argument.
            "--protect" => match std::env::var("CHORD_ROOM_PASSWORD") {
                Ok(password) if !password.is_empty() => settings.password = Some(password),
                _ => return Err("--protect needs CHORD_ROOM_PASSWORD".to_owned().into()),
            },
            "--unprotect" => settings.password = Some(String::new()),
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

/// `pin <item-id>`: pin a message. The pin goes to a private PEP node of our account, so
/// our other devices see it (XEP-0223).
pub async fn pin(client: &Client, item: &str) -> Result<(), CliError> {
    client
        .handle
        .pin_message(item.to_owned())
        .await
        .map_err(err)?;
    println!("pinned {item}");
    Ok(())
}

/// `unpin <chat> <key>`: remove a pin. `chat` and `key` come from `pins`.
pub async fn unpin(client: &Client, chat: &str, key: &str) -> Result<(), CliError> {
    client
        .handle
        .unpin_message(chat.to_owned(), key.to_owned())
        .await
        .map_err(err)?;
    println!("unpinned {key} in {chat}");
    Ok(())
}

/// `pins [chat]`: the pins of one chat or of all chats. With a session it asks the server
/// first, so a new database shows the pins that another device made. With `--offline` it
/// lists what the store has.
pub async fn pins(opts: &Opts, client: &Client, chat: Option<&str>) -> Result<(), CliError> {
    if !opts.offline {
        client.handle.refresh_pins().await.map_err(err)?;
    }
    let list = client
        .handle
        .pins(chat.map(str::to_owned))
        .await
        .map_err(err)?;
    if opts.json {
        let items = list.iter().map(|p| {
            Obj::new()
                .str("chat", &p.chat)
                .str("key", &p.key)
                .opt_str("itemId", p.item_id.as_deref())
                .str("sender", &p.sender)
                .str("body", &p.body)
                .num("timestamp", p.timestamp)
                .num("pinnedAt", p.pinned_at)
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("pins ({})", list.len());
        for p in &list {
            println!("  {} {} {}: {}", p.chat, p.key, p.sender, p.body);
        }
    }
    Ok(())
}

/// `profile [jid]`: read the nickname (XEP-0172) and the vCard4 name (XEP-0292) of an
/// account. With no JID, our own account.
pub async fn profile(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let jid = match args {
        [] => client
            .bound_jid
            .as_ref()
            .map(Jid::to_bare)
            .ok_or_else(|| CliError::from("no session".to_owned()))?,
        [jid] => bare(jid)?,
        _ => return Err("usage: profile [jid]".to_owned().into()),
    };
    let profile = client.handle.profile(jid.clone()).await.map_err(err)?;
    if opts.json {
        println!(
            "{}",
            Obj::new()
                .str("jid", jid.as_str())
                .opt_str("nickname", profile.nickname.as_deref())
                .opt_str("fullName", profile.full_name.as_deref())
                .finish()
        );
    } else {
        println!(
            "{jid} nickname={} full-name={}",
            profile.nickname.as_deref().unwrap_or("-"),
            profile.full_name.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

/// `set-nickname <text>` or `set-nickname --remove`: publish our nickname (XEP-0172).
pub async fn set_nickname(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let nickname = match args {
        ["--remove"] => None,
        [] => return Err("usage: set-nickname <text> | --remove".to_owned().into()),
        words => Some(words.join(" ")),
    };
    client
        .handle
        .set_nickname(nickname.clone())
        .await
        .map_err(err)?;
    match nickname {
        Some(n) => println!("nickname set to {n}"),
        None => println!("nickname removed"),
    }
    Ok(())
}

/// `avatar-set <file>`: publish the image as our avatar (XEP-0084 and the vCard photo).
/// The type comes from the bytes of the file. The core accepts up to 64 KiB.
pub async fn avatar_set(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let [path] = args else {
        return Err("usage: avatar-set <file>".to_owned().into());
    };
    let data = std::fs::read(path).map_err(|e| err(format!("cannot read {path}: {e}")))?;
    let mime = chord_core::features::avatars::sniff_mime(&data).ok_or_else(|| {
        CliError::from("the file is not a png, jpeg, gif or webp image".to_owned())
    })?;
    // The size is in the IHDR of a PNG. Other types send 0 by 0.
    let size = |at: usize| {
        data.get(at..at + 4).map_or(0, |b| {
            u16::try_from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
        })
    };
    let (width, height) = if mime == "image/png" {
        (size(16), size(20))
    } else {
        (0, 0)
    };
    let hash = chord_core::features::avatars::sha1_hex(&data);
    client
        .handle
        .set_avatar(mime.to_owned(), data, width, height)
        .await
        .map_err(err)?;
    println!("avatar set {hash} {mime}");
    Ok(())
}

/// `avatar-get <jid>`: fetch the avatar of a JID from the server, and print what we stored.
pub async fn avatar_get(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let [jid] = args else {
        return Err("usage: avatar-get <jid>".to_owned().into());
    };
    let jid = bare(jid)?;
    client
        .handle
        .refresh_avatar(jid.clone())
        .await
        .map_err(err)?;
    match client.handle.avatar(jid.clone()).await.map_err(err)? {
        Some(a) => println!(
            "{jid} avatar {} {} {}",
            a.hash,
            a.mime.as_deref().unwrap_or("-"),
            a.data.as_ref().map_or(0, Vec::len)
        ),
        None => println!("{jid} has no avatar"),
    }
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
