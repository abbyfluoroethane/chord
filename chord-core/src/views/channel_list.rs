//! `ChannelList`: the rooms and direct chats of a space, or of Home.

use rusqlite::params;

use super::timeline::{contact_name, local_part};
use super::{QueryCtx, ViewItem};

/// Which channels a list shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(
        tag = "type",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum ChannelScope {
    /// Direct chats, and the rooms that are in no space.
    Home,
    /// The rooms of one XEP-0503 space.
    Space { service: String, node: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(
        tag = "type",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum ChannelKind {
    /// A 1:1 chat.
    Direct,
    /// A MUC room.
    Room,
    /// The private messages with one room occupant. The `jid` of the item is
    /// `room/nick`, and its name is the nick. A UI can show "nick (in room)".
    PrivateMessage {
        /// Bare JID of the room.
        room: String,
        nick: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct ChannelItem {
    /// Bare JID of the room or the peer. `room/nick` for a private message channel.
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
    /// For a direct chat: the blocklist (XEP-0191) holds the peer. Always false otherwise.
    pub blocked: bool,
    /// For a joined room outside a space: the people in it now, us included. A UI shows it
    /// as the size of a group chat. `None` for other items.
    pub members: Option<u32>,
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
        if item.kind == ChannelKind::Direct {
            item.blocked = crate::features::blocking::is_blocked(q.store, q.account_id, &item.jid);
        }
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

/// Direct chats and the rooms that are in no space (group chats), by newest activity. The
/// rooms with no message yet come last, by name.
fn home(q: &QueryCtx<'_>) -> rusqlite::Result<Vec<ChannelItem>> {
    let conn = q.store.conn();
    let mut out = Vec::new();
    let mut stmt = conn.prepare_cached(
        // A loose index scan: each step seeks the next peer in `messages_by_kind_peer`,
        // so the cost does not grow with the number of messages.
        "WITH RECURSIVE p(peer) AS (
             SELECT MIN(peer) FROM messages WHERE account_id = ?1 AND kind = 'chat'
             UNION ALL
             SELECT (SELECT MIN(peer) FROM messages
                     WHERE account_id = ?1 AND kind = 'chat' AND peer > p.peer)
             FROM p WHERE p.peer IS NOT NULL
         )
         SELECT peer, (SELECT MAX(timestamp) FROM messages
                       WHERE account_id = ?1 AND kind = 'chat' AND peer = p.peer) AS last
         FROM p WHERE peer IS NOT NULL ORDER BY last DESC",
    )?;
    let dms: Vec<(String, i64)> = stmt
        .query_map(params![q.account_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (jid, last) in dms {
        let (name, kind) = match jid.split_once('/') {
            // A room occupant. A user JID with a resource never is a peer here.
            Some((room, nick)) => (
                nick.to_owned(),
                ChannelKind::PrivateMessage {
                    room: room.to_owned(),
                    nick: nick.to_owned(),
                },
            ),
            None => (contact_name(q, &jid)?, ChannelKind::Direct),
        };
        out.push(ChannelItem {
            name,
            jid,
            kind,
            category: None,
            joined: true,
            last_activity: Some(last),
            unread: 0,
            blocked: false,
            members: None,
        });
    }
    let mut stmt = conn.prepare_cached(
        "SELECT r.jid, r.name, r.joined,
                (SELECT MAX(timestamp) FROM messages m
                 WHERE m.account_id = r.account_id AND m.peer = r.jid),
                (SELECT COUNT(*) FROM occupants o
                 WHERE o.account_id = r.account_id AND o.room = r.jid)
         FROM rooms r
         WHERE r.account_id = ?1 AND (r.bookmarked = 1 OR r.joined = 1)
           AND NOT EXISTS (SELECT 1 FROM space_items s
                           WHERE s.account_id = r.account_id AND s.room_jid = r.jid)
         ORDER BY COALESCE(r.name, r.jid)",
    )?;
    let rooms = stmt.query_map(params![q.account_id], |row| {
        let jid: String = row.get(0)?;
        let name: Option<String> = row.get(1)?;
        let joined = row.get::<_, i64>(2)? != 0;
        let members: u32 = row.get(4)?;
        Ok(ChannelItem {
            name: name.unwrap_or_else(|| local_part(&jid)),
            jid,
            kind: ChannelKind::Room,
            category: None,
            joined,
            last_activity: row.get(3)?,
            unread: 0,
            blocked: false,
            members: (joined && members > 0).then_some(members),
        })
    })?;
    for room in rooms {
        out.push(room?);
    }
    // As in a DM list: the newest first. `None` sorts last, and the sort keeps the name
    // order of the rooms without a message.
    out.sort_by_key(|item| std::cmp::Reverse(item.last_activity));
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
            blocked: false,
            members: None,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::store::queries::{
        Direction, KeyKind, MessageExtras, MessageKind, NewMessage, ensure_account, insert_message,
    };
    use jid::BareJid;

    fn put(store: &Store, account_id: i64, key: &str, dir: Direction, peer: &str) -> i64 {
        insert_message(
            store.conn(),
            account_id,
            &NewMessage {
                kind: MessageKind::Chat,
                key_kind: KeyKind::StanzaId,
                key,
                direction: dir,
                peer,
                sender: peer,
                body: "hi",
                timestamp: None,
                extras: MessageExtras::default(),
            },
        )
        .unwrap()
        .unwrap()
        .rowid
    }

    #[test]
    fn group_chats_sit_among_the_dms_by_activity_with_a_member_count() {
        let store = Store::open_in_memory().unwrap();
        let account = BareJid::new("alice@chord.localhost").unwrap();
        let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
        let q = QueryCtx {
            store: &store,
            account_id,
            account: &account,
        };
        let conn = store.conn();
        let at = |peer: &str, ms: i64| {
            put(
                &store,
                account_id,
                &format!("{peer}-{ms}"),
                Direction::In,
                peer,
            );
            if peer.contains("@muc.") {
                conn.execute(
                    "UPDATE messages SET kind = 'groupchat' WHERE stanza_id = ?1",
                    params![format!("{peer}-{ms}")],
                )
                .unwrap();
            }
            conn.execute(
                "UPDATE messages SET timestamp = ?1 WHERE stanza_id = ?2",
                params![ms, format!("{peer}-{ms}")],
            )
            .unwrap();
        };
        let room = |jid: &str, joined: bool, occupants: &[&str]| {
            conn.execute(
                "INSERT INTO rooms (account_id, jid, name, joined, bookmarked) VALUES (?1, ?2, ?3, ?4, 1)",
                params![account_id, jid, jid.split('@').next().unwrap(), joined],
            )
            .unwrap();
            for nick in occupants {
                conn.execute(
                    "INSERT INTO occupants (account_id, room, nick) VALUES (?1, ?2, ?3)",
                    params![account_id, jid, nick],
                )
                .unwrap();
            }
        };
        room("hikers@muc.chord.localhost", true, &["al", "bo", "cy"]);
        room("quiet@muc.chord.localhost", true, &[]);
        room("zebra@muc.chord.localhost", false, &[]);
        room("apes@muc.chord.localhost", false, &[]);
        at("bob@chord.localhost", 100);
        at("hikers@muc.chord.localhost", 200);
        at("carol@chord.localhost", 300);
        let items = query(&q, &ChannelScope::Home).unwrap();
        let order: Vec<&str> = items.iter().map(|i| i.jid.as_str()).collect();
        assert_eq!(
            order,
            [
                "carol@chord.localhost",
                "hikers@muc.chord.localhost",
                "bob@chord.localhost",
                // No message yet: by name.
                "apes@muc.chord.localhost",
                "quiet@muc.chord.localhost",
                "zebra@muc.chord.localhost",
            ]
        );
        let members = |jid: &str| items.iter().find(|i| i.jid == jid).unwrap().members;
        assert_eq!(members("hikers@muc.chord.localhost"), Some(3));
        // A joined room with no occupant yet, a room we left, and a DM: no count.
        assert_eq!(members("quiet@muc.chord.localhost"), None);
        assert_eq!(members("zebra@muc.chord.localhost"), None);
        assert_eq!(members("bob@chord.localhost"), None);
    }

    #[test]
    fn private_peer_and_chat_use_the_same_read_position() {
        let store = Store::open_in_memory().unwrap();
        let account = BareJid::new("alice@chord.localhost").unwrap();
        let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
        let q = QueryCtx {
            store: &store,
            account_id,
            account: &account,
        };
        let pm = "room@muc.chord.localhost/bob";
        let chat = "carol@chord.localhost";
        for peer in [pm, chat] {
            put(
                &store,
                account_id,
                &format!("{peer}-1"),
                Direction::In,
                peer,
            );
            put(
                &store,
                account_id,
                &format!("{peer}-2"),
                Direction::In,
                peer,
            );
            put(
                &store,
                account_id,
                &format!("{peer}-3"),
                Direction::Out,
                peer,
            );
        }
        let items = query(&q, &ChannelScope::Home).unwrap();
        let pm_item = items.iter().find(|i| i.jid == pm).unwrap();
        assert_eq!(pm_item.name, "bob");
        assert_eq!(
            pm_item.kind,
            ChannelKind::PrivateMessage {
                room: "room@muc.chord.localhost".into(),
                nick: "bob".into()
            }
        );
        assert_eq!(pm_item.unread, 2);
        assert_eq!(items.iter().find(|i| i.jid == chat).unwrap().unread, 2);

        // Read the first incoming message of each peer, as mark_read does.
        for peer in [pm, chat] {
            let first: i64 = store
                .conn()
                .query_row(
                    "SELECT MIN(id) FROM messages WHERE peer = ?1",
                    params![peer],
                    |r| r.get(0),
                )
                .unwrap();
            store
                .conn()
                .execute(
                    "INSERT INTO read_state (account_id, peer, last_read) VALUES (?1, ?2, ?3)",
                    params![account_id, peer, first],
                )
                .unwrap();
            assert_eq!(unread(&q, peer).unwrap(), 1, "{peer}");
        }
    }
}
