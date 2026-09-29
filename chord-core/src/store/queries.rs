//! Queries. All of them run on the actor's single connection.

use rusqlite::{Connection, OptionalExtension, params};

/// Which XEP-0359 id is the message key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum KeyKind {
    /// The stanza-id that our own server added.
    StanzaId,
    /// The origin-id that the sender added.
    OriginId,
}

impl KeyKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::StanzaId => "stanza-id",
            Self::OriginId => "origin-id",
        }
    }

    fn parse(s: &str) -> rusqlite::Result<Self> {
        match s {
            "stanza-id" => Ok(Self::StanzaId),
            "origin-id" => Ok(Self::OriginId),
            other => Err(rusqlite::Error::InvalidColumnType(
                0,
                format!("key_kind {other}"),
                rusqlite::types::Type::Text,
            )),
        }
    }
}

/// The direction of a message, seen from the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum Direction {
    In,
    Out,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
        }
    }

    fn parse(s: &str) -> rusqlite::Result<Self> {
        match s {
            "in" => Ok(Self::In),
            "out" => Ok(Self::Out),
            other => Err(rusqlite::Error::InvalidColumnType(
                0,
                format!("direction {other}"),
                rusqlite::types::Type::Text,
            )),
        }
    }
}

/// A 1:1 chat message, or a MUC message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum MessageKind {
    Chat,
    Groupchat,
}

impl MessageKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Groupchat => "groupchat",
        }
    }

    fn parse(s: &str) -> rusqlite::Result<Self> {
        match s {
            "chat" => Ok(Self::Chat),
            "groupchat" => Ok(Self::Groupchat),
            other => Err(rusqlite::Error::InvalidColumnType(
                0,
                format!("kind {other}"),
                rusqlite::types::Type::Text,
            )),
        }
    }
}

/// The ids and references of a message, besides its key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageExtras {
    /// The `id` attribute of the stanza.
    pub message_id: Option<String>,
    /// XEP-0359 origin-id.
    pub origin_id: Option<String>,
    /// XEP-0359 stanza-id from our server (1:1) or from the room.
    pub stanza_id: Option<String>,
    /// XEP-0461: the id that this message replies to.
    pub reply_to: Option<String>,
    /// XEP-0461: the JID of the sender of the message that this one replies to.
    pub reply_to_sender: Option<String>,
    /// XEP-0066: an attachment URL.
    pub oob_url: Option<String>,
}

/// A chat message, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct StoredMessage {
    /// Row id in `messages`.
    pub rowid: i64,
    pub kind: MessageKind,
    pub key_kind: KeyKind,
    pub key: String,
    pub direction: Direction,
    /// Bare JID of the other side.
    pub peer: String,
    /// JID of the sender, as in the stanza.
    pub sender: String,
    pub body: String,
    /// Unix time in ms.
    pub timestamp: i64,
}

/// A message to store. `timestamp` is `None` for "now".
pub struct NewMessage<'a> {
    pub kind: MessageKind,
    pub key_kind: KeyKind,
    pub key: &'a str,
    pub direction: Direction,
    pub peer: &'a str,
    pub sender: &'a str,
    pub body: &'a str,
    pub timestamp: Option<i64>,
    pub extras: MessageExtras,
}

/// Current Unix time in ms, from SQLite's clock. This also works in the phase 2
/// WASM build, where `std::time::SystemTime::now` panics.
const NOW_MS: &str = "CAST(unixepoch('subsec') * 1000 AS INTEGER)";

/// Return the id of the account row for `jid`. Create the row if necessary.
pub fn ensure_account(conn: &Connection, jid: &str) -> rusqlite::Result<i64> {
    conn.execute(
        &format!("INSERT INTO accounts (jid, created_at) VALUES (?1, {NOW_MS}) ON CONFLICT (jid) DO NOTHING"),
        params![jid],
    )?;
    conn.query_row(
        "SELECT id FROM accounts WHERE jid = ?1",
        params![jid],
        |row| row.get(0),
    )
}

/// Store a message. Returns `None` if a message with the same key exists already.
pub fn insert_message(
    conn: &Connection,
    account_id: i64,
    message: &NewMessage<'_>,
) -> rusqlite::Result<Option<StoredMessage>> {
    let x = &message.extras;
    // The key is one of the ids too.
    let origin_id = x
        .origin_id
        .as_deref()
        .or((message.key_kind == KeyKind::OriginId).then_some(message.key));
    let stanza_id = x
        .stanza_id
        .as_deref()
        .or((message.key_kind == KeyKind::StanzaId).then_some(message.key));
    let inserted: Option<(i64, i64)> = conn
        .query_row(
            &format!(
                "INSERT INTO messages
                    (account_id, key_kind, key, direction, peer, sender, body, timestamp, kind,
                     message_id, origin_id, stanza_id, reply_to, reply_to_sender, oob_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, {NOW_MS}), ?9,
                         ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT (account_id, key_kind, key) DO NOTHING
                 RETURNING id, timestamp"
            ),
            params![
                account_id,
                message.key_kind.as_str(),
                message.key,
                message.direction.as_str(),
                message.peer,
                message.sender,
                message.body,
                message.timestamp,
                message.kind.as_str(),
                x.message_id,
                origin_id,
                stanza_id,
                x.reply_to,
                x.reply_to_sender,
                x.oob_url,
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(inserted.map(|(rowid, timestamp)| StoredMessage {
        rowid,
        kind: message.kind,
        key_kind: message.key_kind,
        key: message.key.to_owned(),
        direction: message.direction,
        peer: message.peer.to_owned(),
        sender: message.sender.to_owned(),
        body: message.body.to_owned(),
        timestamp,
    }))
}

/// Change the key of a stored message from its origin-id to the stanza-id that our
/// server gave it later (for example through carbons or MAM). Returns false if no
/// message matches, or if a message with that stanza-id exists already.
///
/// The match includes the direction and the peer, so a forged origin-id from another
/// JID cannot change a message of a different conversation.
pub fn upgrade_to_stanza_id(
    conn: &Connection,
    account_id: i64,
    origin_id: &str,
    direction: Direction,
    peer: &str,
    stanza_id: &str,
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE OR IGNORE messages SET key_kind = 'stanza-id', key = ?1, stanza_id = ?1
         WHERE account_id = ?2 AND key_kind = 'origin-id' AND key = ?3
           AND direction = ?4 AND peer = ?5",
        params![stanza_id, account_id, origin_id, direction.as_str(), peer],
    )?;
    Ok(changed == 1)
}

/// Clear the tables that describe the live session: presence, occupants, and joined
/// rooms. The actor calls it at each new session (not after a resumption).
pub fn clear_volatile(store: &crate::store::Store, account_id: i64) -> rusqlite::Result<()> {
    store.conn().execute_batch(&format!(
        "DELETE FROM presences WHERE account_id = {account_id};
         DELETE FROM occupants WHERE account_id = {account_id};
         UPDATE rooms SET joined = 0 WHERE account_id = {account_id};"
    ))
}

/// A stored message, with the fields that extensions (corrections, reactions, ...) need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRow {
    pub rowid: i64,
    pub kind: MessageKind,
    pub direction: Direction,
    pub peer: String,
    pub sender: String,
    pub message_id: Option<String>,
    pub origin_id: Option<String>,
    pub stanza_id: Option<String>,
    pub body: String,
    pub timestamp: i64,
    pub retracted: bool,
}

const ROW_COLUMNS: &str = "id, kind, direction, peer, sender, message_id, origin_id, stanza_id,
     COALESCE(edited_body, body), timestamp, retracted_at IS NOT NULL";

fn message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        rowid: row.get(0)?,
        kind: MessageKind::parse(&row.get::<_, String>(1)?)?,
        direction: Direction::parse(&row.get::<_, String>(2)?)?,
        peer: row.get(3)?,
        sender: row.get(4)?,
        message_id: row.get(5)?,
        origin_id: row.get(6)?,
        stanza_id: row.get(7)?,
        body: row.get(8)?,
        timestamp: row.get(9)?,
        retracted: row.get(10)?,
    })
}

/// The newest message in the chat or room `peer` that has `id` as its `id` attribute,
/// origin-id, or stanza-id. A reference from another XEP uses one of them.
pub fn find_message(
    conn: &Connection,
    account_id: i64,
    peer: &str,
    id: &str,
) -> rusqlite::Result<Option<MessageRow>> {
    conn.prepare_cached(&format!(
        "SELECT {ROW_COLUMNS} FROM messages
         WHERE account_id = ?1 AND peer = ?2
           AND (message_id = ?3 OR origin_id = ?3 OR stanza_id = ?3)
         ORDER BY id DESC LIMIT 1"
    ))?
    .query_row(params![account_id, peer, id], message_row)
    .optional()
}

/// The message with this timeline id. `TimelineItem::id` is `m:<row id>` and stays the
/// same for the life of the row. The old forms `stanza-id:<id>` and `origin-id:<id>` still
/// work.
pub fn find_by_timeline_id(
    conn: &Connection,
    account_id: i64,
    timeline_id: &str,
) -> rusqlite::Result<Option<MessageRow>> {
    let Some((kind, key)) = timeline_id.split_once(':') else {
        return Ok(None);
    };
    if kind == "m" {
        let Ok(rowid) = key.parse::<i64>() else {
            return Ok(None);
        };
        return conn
            .prepare_cached(&format!(
                "SELECT {ROW_COLUMNS} FROM messages WHERE account_id = ?1 AND id = ?2"
            ))?
            .query_row(params![account_id, rowid], message_row)
            .optional();
    }
    let found = conn
        .prepare_cached(&format!(
            "SELECT {ROW_COLUMNS} FROM messages
             WHERE account_id = ?1 AND key_kind = ?2 AND key = ?3"
        ))?
        .query_row(params![account_id, kind, key], message_row)
        .optional()?;
    if found.is_some() || kind != "origin-id" {
        return Ok(found);
    }
    // The key of a sent message changes to its stanza-id when the server echo or the
    // archive gives one. A UI can still hold the old id. The row keeps its origin-id.
    conn.prepare_cached(&format!(
        "SELECT {ROW_COLUMNS} FROM messages
         WHERE account_id = ?1 AND origin_id = ?2 ORDER BY id DESC LIMIT 1"
    ))?
    .query_row(params![account_id, key], message_row)
    .optional()
}

/// The id that other XEPs use to reference this message: the room stanza-id in a room,
/// the `id` attribute (or the origin-id) in a 1:1 chat.
pub fn reference_id(row: &MessageRow) -> Option<&str> {
    match row.kind {
        MessageKind::Groupchat => row.stanza_id.as_deref(),
        MessageKind::Chat => row.message_id.as_deref().or(row.origin_id.as_deref()),
    }
}

/// All messages with `peer`, oldest first.
pub fn messages_with(
    conn: &Connection,
    account_id: i64,
    peer: &str,
) -> rusqlite::Result<Vec<StoredMessage>> {
    let mut stmt = conn.prepare(
        "SELECT key_kind, key, direction, peer, sender, body, timestamp, kind, id FROM messages
         WHERE account_id = ?1 AND peer = ?2 ORDER BY timestamp, id",
    )?;
    let rows = stmt.query_map(params![account_id, peer], |row| {
        Ok(StoredMessage {
            key_kind: KeyKind::parse(&row.get::<_, String>(0)?)?,
            key: row.get(1)?,
            direction: Direction::parse(&row.get::<_, String>(2)?)?,
            peer: row.get(3)?,
            sender: row.get(4)?,
            body: row.get(5)?,
            timestamp: row.get(6)?,
            kind: MessageKind::parse(&row.get::<_, String>(7)?)?,
            rowid: row.get(8)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn message(key: &str) -> NewMessage<'_> {
        NewMessage {
            kind: MessageKind::Chat,
            key_kind: KeyKind::StanzaId,
            key,
            direction: Direction::In,
            peer: "bob@chord.localhost",
            sender: "bob@chord.localhost/phone",
            body: "hi",
            timestamp: None,
            extras: MessageExtras::default(),
        }
    }

    #[test]
    fn insert_is_idempotent_per_key() {
        let store = Store::open_in_memory().unwrap();
        let account = ensure_account(store.conn(), "alice@chord.localhost").unwrap();
        assert_eq!(
            ensure_account(store.conn(), "alice@chord.localhost").unwrap(),
            account
        );

        let first = insert_message(store.conn(), account, &message("s1")).unwrap();
        assert!(
            first.unwrap().timestamp > 1_700_000_000_000,
            "SQLite clock in ms"
        );
        assert!(
            insert_message(store.conn(), account, &message("s1"))
                .unwrap()
                .is_none()
        );
        assert!(
            insert_message(store.conn(), account, &message("s2"))
                .unwrap()
                .is_some()
        );
        let stored = messages_with(store.conn(), account, "bob@chord.localhost").unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[test]
    fn find_message_by_each_id_and_reference_id() {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        let account = ensure_account(conn, "alice@chord.localhost").unwrap();
        let mut chat = message("s-1");
        chat.extras = MessageExtras {
            message_id: Some("m-1".into()),
            origin_id: Some("o-1".into()),
            ..MessageExtras::default()
        };
        let stored = insert_message(conn, account, &chat).unwrap().unwrap();
        let peer = "bob@chord.localhost";
        for id in ["m-1", "o-1", "s-1"] {
            let row = find_message(conn, account, peer, id).unwrap().unwrap();
            assert_eq!(row.rowid, stored.rowid, "{id}");
        }
        assert!(
            find_message(conn, account, "eve@chord.localhost", "m-1")
                .unwrap()
                .is_none()
        );
        let row = find_by_timeline_id(conn, account, "stanza-id:s-1")
            .unwrap()
            .unwrap();
        // In a 1:1 chat the reference is the `id` attribute.
        assert_eq!(reference_id(&row), Some("m-1"));
        // In a room the reference is the room stanza-id.
        let room = MessageRow {
            kind: MessageKind::Groupchat,
            ..row
        };
        assert_eq!(reference_id(&room), Some("s-1"));
    }

    #[test]
    fn origin_id_key_upgrades_to_stanza_id_once() {
        let store = Store::open_in_memory().unwrap();
        let account = ensure_account(store.conn(), "alice@chord.localhost").unwrap();
        let sent = NewMessage {
            kind: MessageKind::Chat,
            key_kind: KeyKind::OriginId,
            key: "o-1",
            direction: Direction::Out,
            peer: "bob@chord.localhost",
            sender: "alice@chord.localhost",
            body: "hello",
            timestamp: None,
            extras: MessageExtras::default(),
        };
        insert_message(store.conn(), account, &sent)
            .unwrap()
            .unwrap();
        let conn = store.conn();
        let peer = "bob@chord.localhost";
        // Another direction or peer does not match.
        assert!(!upgrade_to_stanza_id(conn, account, "o-1", Direction::In, peer, "s-1").unwrap());
        assert!(
            !upgrade_to_stanza_id(conn, account, "o-1", Direction::Out, "eve@x", "s-1").unwrap()
        );
        assert!(upgrade_to_stanza_id(conn, account, "o-1", Direction::Out, peer, "s-1").unwrap());
        assert!(!upgrade_to_stanza_id(conn, account, "o-1", Direction::Out, peer, "s-1").unwrap());
        // The old timeline id still finds the row.
        let row = find_by_timeline_id(conn, account, "origin-id:o-1")
            .unwrap()
            .unwrap();
        assert_eq!(row.stanza_id.as_deref(), Some("s-1"));
        assert!(
            find_by_timeline_id(conn, account, "origin-id:o-2")
                .unwrap()
                .is_none()
        );
        let stored = messages_with(conn, account, peer).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            (stored[0].key_kind, stored[0].key.as_str()),
            (KeyKind::StanzaId, "s-1")
        );
    }
}
