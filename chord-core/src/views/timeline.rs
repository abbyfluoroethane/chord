//! `Timeline`: the messages in a room or a 1:1 chat, pre-shaped for display.

use jid::BareJid;
use rusqlite::{OptionalExtension, params};

use super::{QueryCtx, ViewItem};

/// Two messages from the same sender within this time form one group.
pub const GROUP_GAP_MS: i64 = 5 * 60 * 1000;

/// One message, ready to show.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineItem {
    /// Stable id: `stanza-id:<id>` or `origin-id:<id>`.
    pub id: String,
    /// JID of the sender. For a room message: room@service/nick.
    pub sender: String,
    /// Display name: roster name, room nick, or the local part of the JID.
    pub sender_name: String,
    /// Avatar hash (XEP-0084 id) of the sender, if known.
    pub avatar: Option<String>,
    pub body: String,
    /// Unix time in ms.
    pub timestamp: i64,
    pub outgoing: bool,
    /// True if the previous item has the same sender and is less than 5 min older.
    pub same_sender_as_previous: bool,
}

impl ViewItem for TimelineItem {
    type Key = String;
    fn key(&self) -> String {
        self.id.clone()
    }
}

/// The newest `window` messages of `room`, oldest first.
pub(crate) fn query(
    q: &QueryCtx<'_>,
    room: &BareJid,
    window: usize,
) -> rusqlite::Result<Vec<TimelineItem>> {
    let conn = q.store.conn();
    let mut stmt = conn.prepare_cached(
        "SELECT key_kind, key, direction, sender, body, timestamp, kind FROM messages
         WHERE account_id = ?1 AND peer = ?2
         ORDER BY timestamp DESC, id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![q.account_id, room.as_str(), window as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)? == "out",
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)? == "groupchat",
        ))
    })?;
    let mut rows: Vec<_> = rows.collect::<rusqlite::Result<_>>()?;
    rows.reverse();

    let mut items: Vec<TimelineItem> = Vec::with_capacity(rows.len());
    for (key_kind, key, outgoing, sender, body, timestamp, groupchat) in rows {
        let (sender_name, avatar_owner) = if groupchat {
            let nick = sender
                .rsplit_once('/')
                .map_or(sender.as_str(), |(_, n)| n)
                .to_owned();
            (nick, sender.clone())
        } else {
            let bare = sender.split('/').next().unwrap_or(&sender).to_owned();
            (contact_name(q, &bare)?, bare)
        };
        let avatar = avatar_hash(q, &avatar_owner)?;
        let same_sender_as_previous = items
            .last()
            .is_some_and(|p| p.sender == sender && timestamp - p.timestamp < GROUP_GAP_MS);
        items.push(TimelineItem {
            id: format!("{key_kind}:{key}"),
            sender,
            sender_name,
            avatar,
            body,
            timestamp,
            outgoing,
            same_sender_as_previous,
        });
    }
    Ok(items)
}

/// Roster name of a bare JID, or its local part.
pub(crate) fn contact_name(q: &QueryCtx<'_>, bare: &str) -> rusqlite::Result<String> {
    let name: Option<String> = q
        .store
        .conn()
        .prepare_cached("SELECT name FROM contacts WHERE account_id = ?1 AND jid = ?2")?
        .query_row(params![q.account_id, bare], |row| row.get(0))
        .optional()?
        .flatten();
    Ok(name.unwrap_or_else(|| local_part(bare)))
}

pub(crate) fn avatar_hash(q: &QueryCtx<'_>, owner: &str) -> rusqlite::Result<Option<String>> {
    q.store
        .conn()
        .prepare_cached("SELECT hash FROM avatars WHERE account_id = ?1 AND owner = ?2")?
        .query_row(params![q.account_id, owner], |row| row.get(0))
        .optional()
}

/// The local part of a JID, or the whole JID if it has none.
pub(crate) fn local_part(jid: &str) -> String {
    jid.split_once('@')
        .map_or(jid, |(local, _)| local)
        .to_owned()
}
