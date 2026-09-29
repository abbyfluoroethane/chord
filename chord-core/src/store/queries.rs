//! Queries. All of them run on the actor's single connection.

use rusqlite::{Connection, OptionalExtension, params};

/// Which XEP-0359 id is the message key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// A chat message, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredMessage {
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
    let timestamp: Option<i64> = conn
        .query_row(
            &format!(
                "INSERT INTO messages
                    (account_id, key_kind, key, direction, peer, sender, body, timestamp, kind)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, {NOW_MS}), ?9)
                 ON CONFLICT (account_id, key_kind, key) DO NOTHING
                 RETURNING timestamp"
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
            ],
            |row| row.get(0),
        )
        .optional()?;
    Ok(timestamp.map(|timestamp| StoredMessage {
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
        "UPDATE OR IGNORE messages SET key_kind = 'stanza-id', key = ?1
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

/// All messages with `peer`, oldest first.
pub fn messages_with(
    conn: &Connection,
    account_id: i64,
    peer: &str,
) -> rusqlite::Result<Vec<StoredMessage>> {
    let mut stmt = conn.prepare(
        "SELECT key_kind, key, direction, peer, sender, body, timestamp, kind FROM messages
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
        let stored = messages_with(conn, account, peer).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            (stored[0].key_kind, stored[0].key.as_str()),
            (KeyKind::StanzaId, "s-1")
        );
    }
}
