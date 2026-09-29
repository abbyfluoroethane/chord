//! `ChannelList`: the rooms and direct chats of a space, or of Home.

use rusqlite::params;

use super::timeline::{contact_name, local_part};
use super::{QueryCtx, ViewItem};

/// Which channels a list shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChannelScope {
    /// Direct chats, and the rooms that are in no space.
    Home,
    /// The rooms of one XEP-0503 space.
    Space { service: String, node: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    /// A 1:1 chat.
    Direct,
    /// A MUC room.
    Room,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelItem {
    /// Bare JID of the room or the peer.
    pub jid: String,
    pub name: String,
    pub kind: ChannelKind,
    /// Category inside a space, if the space gives one.
    pub category: Option<String>,
    /// True if the account is in the room now. Always true for a direct chat.
    pub joined: bool,
    /// Time of the newest message, Unix ms.
    pub last_activity: Option<i64>,
    /// Incoming messages after our read position (XEP-0333). With no read position,
    /// every incoming message counts.
    pub unread: u32,
}

impl ViewItem for ChannelItem {
    type Key = String;
    fn key(&self) -> String {
        self.jid.clone()
    }
}

pub(crate) fn query(q: &QueryCtx<'_>, scope: &ChannelScope) -> rusqlite::Result<Vec<ChannelItem>> {
    let mut items = match scope {
        ChannelScope::Home => home(q)?,
        ChannelScope::Space { service, node } => space(q, service, node)?,
    };
    for item in &mut items {
        item.unread = unread(q, &item.jid)?;
    }
    Ok(items)
}

/// Incoming messages in `peer` after our read position.
pub(crate) fn unread(q: &QueryCtx<'_>, peer: &str) -> rusqlite::Result<u32> {
    q.store
        .conn()
        .prepare_cached(
            "SELECT COUNT(*) FROM messages
             WHERE account_id = ?1 AND peer = ?2 AND direction = 'in' AND retracted_at IS NULL
               AND id > COALESCE((SELECT last_read FROM read_state
                                  WHERE account_id = ?1 AND peer = ?2), 0)",
        )?
        .query_row(params![q.account_id, peer], |row| row.get(0))
}

/// Direct chats by newest activity, then the rooms that are in no space, by name.
fn home(q: &QueryCtx<'_>) -> rusqlite::Result<Vec<ChannelItem>> {
    let conn = q.store.conn();
    let mut out = Vec::new();
    let mut stmt = conn.prepare_cached(
        "SELECT peer, MAX(timestamp) FROM messages WHERE account_id = ?1 AND kind = 'chat'
         GROUP BY peer ORDER BY MAX(timestamp) DESC",
    )?;
    let dms: Vec<(String, i64)> = stmt
        .query_map(params![q.account_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (jid, last) in dms {
        out.push(ChannelItem {
            name: contact_name(q, &jid)?,
            jid,
            kind: ChannelKind::Direct,
            category: None,
            joined: true,
            last_activity: Some(last),
            unread: 0,
        });
    }
    let mut stmt = conn.prepare_cached(
        "SELECT r.jid, r.name, r.joined,
                (SELECT MAX(timestamp) FROM messages m
                 WHERE m.account_id = r.account_id AND m.peer = r.jid)
         FROM rooms r
         WHERE r.account_id = ?1 AND (r.bookmarked = 1 OR r.joined = 1)
           AND NOT EXISTS (SELECT 1 FROM space_items s
                           WHERE s.account_id = r.account_id AND s.room_jid = r.jid)
         ORDER BY COALESCE(r.name, r.jid)",
    )?;
    let rooms = stmt.query_map(params![q.account_id], |row| {
        let jid: String = row.get(0)?;
        let name: Option<String> = row.get(1)?;
        Ok(ChannelItem {
            name: name.unwrap_or_else(|| local_part(&jid)),
            jid,
            kind: ChannelKind::Room,
            category: None,
            joined: row.get::<_, i64>(2)? != 0,
            last_activity: row.get(3)?,
            unread: 0,
        })
    })?;
    for room in rooms {
        out.push(room?);
    }
    Ok(out)
}

/// The room items of a space, by category and position.
fn space(q: &QueryCtx<'_>, service: &str, node: &str) -> rusqlite::Result<Vec<ChannelItem>> {
    let mut stmt = q.store.conn().prepare_cached(
        "SELECT s.room_jid, COALESCE(s.name, r.name), s.category, COALESCE(r.joined, 0),
                (SELECT MAX(timestamp) FROM messages m
                 WHERE m.account_id = s.account_id AND m.peer = s.room_jid)
         FROM space_items s
         LEFT JOIN rooms r ON r.account_id = s.account_id AND r.jid = s.room_jid
         WHERE s.account_id = ?1 AND s.service = ?2 AND s.node = ?3 AND s.room_jid IS NOT NULL
         ORDER BY s.category IS NOT NULL, s.category, s.position, s.item_id",
    )?;
    let rows = stmt.query_map(params![q.account_id, service, node], |row| {
        let jid: String = row.get(0)?;
        let name: Option<String> = row.get(1)?;
        Ok(ChannelItem {
            name: name.unwrap_or_else(|| local_part(&jid)),
            jid,
            kind: ChannelKind::Room,
            category: row.get(2)?,
            joined: row.get::<_, i64>(3)? != 0,
            last_activity: row.get(4)?,
            unread: 0,
        })
    })?;
    rows.collect()
}
