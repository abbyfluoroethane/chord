//! Commands that change something: rooms, uploads, spaces, and contacts.

use std::path::Path;

use chord_core::features::roster::Subscription;
use chord_core::features::spaces::JoinOutcome;
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

pub async fn space_join(client: &Client, service: &str, node: &str) -> Result<(), CliError> {
    match client.handle.join_space(service, node).await.map_err(err)? {
        JoinOutcome::Joined => println!("joined space {service} {node}"),
        JoinOutcome::Pending => println!("the owner of {service} {node} must approve the join"),
    }
    Ok(())
}

/// `space-create <name> [--private]`.
pub async fn space_create(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let (name, private) = match args {
        [name] => (*name, false),
        [name, "--private"] => (*name, true),
        _ => return Err("usage: space-create <name> [--private]".to_owned().into()),
    };
    let (service, node) = client
        .handle
        .create_space(name, private)
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
            println!("  {} {name} ({}{ask})", c.jid, sub(c.subscription));
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
