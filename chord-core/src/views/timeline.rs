//! `Timeline`: the messages in a room or a 1:1 chat, pre-shaped for display.

use jid::BareJid;
use rusqlite::{OptionalExtension, params};

use super::{QueryCtx, ViewItem};

/// Two messages from the same sender within this time form one group.
pub const GROUP_GAP_MS: i64 = 5 * 60 * 1000;

/// One message, ready to show.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineItem {
    /// Stable id: `m:<row id>`. It does not change when the server echo or the archive
    /// changes the key of the message. Commands that refer to a message (reply, edit,
    /// retract, react) take this id.
    pub id: String,
    /// The stanza-id from the server or the room, if known. It can arrive after the message.
    pub stanza_id: Option<String>,
    /// The XEP-0359 origin-id, if the message has one.
    pub origin_id: Option<String>,
    /// JID of the sender. For a room message: room@service/nick.
    pub sender: String,
    /// Display name: roster name, room nick, or the local part of the JID.
    pub sender_name: String,
    /// Avatar hash (XEP-0084 id) of the sender, if known.
    pub avatar: Option<String>,
    /// The newest text: the correction if the message was edited. Empty if retracted.
    pub body: String,
    /// Unix time in ms.
    pub timestamp: i64,
    pub outgoing: bool,
    /// True if the previous item has the same sender and is less than 5 min older.
    pub same_sender_as_previous: bool,
    /// XEP-0308: the sender corrected the message.
    pub edited: bool,
    /// XEP-0424: the sender retracted the message.
    pub retracted: bool,
    /// XEP-0444: the reactions, one entry per emoji, most used first.
    pub reactions: Vec<ReactionSummary>,
    /// XEP-0461: the message that this one replies to.
    pub reply_to: Option<ReplyPreview>,
    /// XEP-0066: an attachment URL (for example from an upload).
    pub attachment: Option<String>,
    /// XEP-0333: for an outgoing message, how far it got.
    pub status: DeliveryStatus,
}

/// The reactions with one emoji.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    /// True if our account is one of the senders.
    pub mine: bool,
}

/// A short view of the message that a reply quotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyPreview {
    /// Timeline id of the quoted message, if it is in the store.
    pub id: Option<String>,
    pub sender_name: String,
    /// The start of the quoted text. Empty if the message is not in the store.
    pub body: String,
}

/// How far an outgoing message got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryStatus {
    Sent,
    Received,
    Displayed,
}

/// The length of the quoted text in a `ReplyPreview`.
const PREVIEW_CHARS: usize = 120;

impl ViewItem for TimelineItem {
    type Key = String;
    fn key(&self) -> String {
        self.id.clone()
    }
}

struct Row {
    rowid: i64,
    stanza_id: Option<String>,
    origin_id: Option<String>,
    outgoing: bool,
    sender: String,
    body: String,
    timestamp: i64,
    groupchat: bool,
    edited: bool,
    retracted: bool,
    reply_to: Option<String>,
    reply_to_sender: Option<String>,
    attachment: Option<String>,
    status: String,
}

/// The `peer` value of a timeline: the room or the chat peer, or `room/nick` for the
/// private messages with one occupant.
pub(crate) fn peer_of(room: &BareJid, nick: Option<&str>) -> String {
    match nick {
        Some(nick) => format!("{room}/{nick}"),
        None => room.to_string(),
    }
}

/// The newest `window` messages of `peer`, oldest first. A peer with a `/` is a room
/// occupant, and its messages show the nick as the sender name.
pub(crate) fn query(
    q: &QueryCtx<'_>,
    room: &str,
    window: usize,
) -> rusqlite::Result<Vec<TimelineItem>> {
    let conn = q.store.conn();
    let mut stmt = conn.prepare_cached(
        "SELECT id, stanza_id, origin_id, direction, sender, COALESCE(edited_body, body), timestamp,
                kind, edited_body IS NOT NULL, retracted_at IS NOT NULL, reply_to,
                reply_to_sender, oob_url, status
         FROM messages
         WHERE account_id = ?1 AND peer = ?2
         ORDER BY timestamp DESC, id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![q.account_id, room, window as i64], |row| {
        Ok(Row {
            rowid: row.get(0)?,
            stanza_id: row.get(1)?,
            origin_id: row.get(2)?,
            outgoing: row.get::<_, String>(3)? == "out",
            sender: row.get(4)?,
            body: row.get(5)?,
            timestamp: row.get(6)?,
            groupchat: row.get::<_, String>(7)? == "groupchat" || room.contains('/'),
            edited: row.get(8)?,
            retracted: row.get(9)?,
            reply_to: row.get(10)?,
            reply_to_sender: row.get(11)?,
            attachment: row.get(12)?,
            status: row.get(13)?,
        })
    })?;
    let mut rows: Vec<Row> = rows.collect::<rusqlite::Result<_>>()?;
    rows.reverse();

    let mut items: Vec<TimelineItem> = Vec::with_capacity(rows.len());
    for row in rows {
        let (sender_name, avatar_owner) = display_name(q, &row.sender, row.groupchat)?;
        let avatar = avatar_hash(q, &avatar_owner)?;
        let same_sender_as_previous = items
            .last()
            .is_some_and(|p| p.sender == row.sender && row.timestamp - p.timestamp < GROUP_GAP_MS);
        let reply_to = match &row.reply_to {
            Some(id) => Some(reply_preview(
                q,
                room,
                id,
                row.reply_to_sender.as_deref(),
                row.groupchat,
            )?),
            None => None,
        };
        items.push(TimelineItem {
            id: format!("m:{}", row.rowid),
            stanza_id: row.stanza_id,
            origin_id: row.origin_id,
            sender: row.sender,
            sender_name,
            avatar,
            body: if row.retracted {
                String::new()
            } else {
                row.body
            },
            timestamp: row.timestamp,
            outgoing: row.outgoing,
            same_sender_as_previous,
            edited: row.edited,
            retracted: row.retracted,
            reactions: if row.retracted {
                Vec::new()
            } else {
                reactions(q, row.rowid)?
            },
            reply_to,
            attachment: if row.retracted { None } else { row.attachment },
            status: match row.status.as_str() {
                "displayed" => DeliveryStatus::Displayed,
                "received" => DeliveryStatus::Received,
                _ => DeliveryStatus::Sent,
            },
        });
    }
    Ok(items)
}

/// The display name of a sender, and the owner of its avatar.
fn display_name(
    q: &QueryCtx<'_>,
    sender: &str,
    groupchat: bool,
) -> rusqlite::Result<(String, String)> {
    if groupchat {
        let nick = sender
            .rsplit_once('/')
            .map_or(sender, |(_, n)| n)
            .to_owned();
        Ok((nick, sender.to_owned()))
    } else {
        let bare = sender.split('/').next().unwrap_or(sender).to_owned();
        Ok((contact_name(q, &bare)?, bare))
    }
}

/// The reactions to a message, one entry per emoji, most used first. Our own reactions
/// have our bare JID as the sender.
fn reactions(q: &QueryCtx<'_>, rowid: i64) -> rusqlite::Result<Vec<ReactionSummary>> {
    let mut stmt = q.store.conn().prepare_cached(
        "SELECT sender, emojis FROM reactions WHERE account_id = ?1 AND message = ?2",
    )?;
    let rows = stmt.query_map(params![q.account_id, rowid], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out: Vec<ReactionSummary> = Vec::new();
    for row in rows {
        let (sender, emojis) = row?;
        let mine = sender == q.account.as_str();
        for emoji in crate::store::json::from_array(&emojis) {
            match out.iter_mut().find(|r| r.emoji == emoji) {
                Some(r) => {
                    r.count += 1;
                    r.mine |= mine;
                }
                None => out.push(ReactionSummary {
                    emoji,
                    count: 1,
                    mine,
                }),
            }
        }
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.emoji.cmp(&b.emoji)));
    Ok(out)
}

fn reply_preview(
    q: &QueryCtx<'_>,
    room: &str,
    id: &str,
    sender: Option<&str>,
    groupchat: bool,
) -> rusqlite::Result<ReplyPreview> {
    let found = crate::store::queries::find_message(q.store.conn(), q.account_id, room, id)?;
    match found {
        Some(m) => {
            let (sender_name, _) = display_name(q, &m.sender, groupchat)?;
            let body = if m.retracted {
                String::new()
            } else {
                m.body.chars().take(PREVIEW_CHARS).collect()
            };
            Ok(ReplyPreview {
                id: Some(format!("m:{}", m.rowid)),
                sender_name,
                body,
            })
        }
        None => {
            let sender_name = match sender {
                Some(s) => display_name(q, s, groupchat)?.0,
                None => String::new(),
            };
            Ok(ReplyPreview {
                id: None,
                sender_name,
                body: String::new(),
            })
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::store::queries::{
        Direction, KeyKind, MessageExtras, MessageKind, NewMessage, ensure_account,
        find_by_timeline_id, insert_message, upgrade_to_stanza_id,
    };
    use crate::views::diff::{ListDiff, diff};

    #[test]
    fn a_key_upgrade_gives_one_update() {
        let store = Store::open_in_memory().unwrap();
        let account = BareJid::new("alice@chord.localhost").unwrap();
        let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
        let q = QueryCtx {
            store: &store,
            account_id,
            account: &account,
        };
        let peer = "bob@chord.localhost";
        insert_message(
            store.conn(),
            account_id,
            &NewMessage {
                kind: MessageKind::Chat,
                key_kind: KeyKind::OriginId,
                key: "o-1",
                direction: Direction::Out,
                peer,
                sender: "alice@chord.localhost",
                body: "hi",
                timestamp: None,
                extras: MessageExtras::default(),
            },
        )
        .unwrap();
        let before = query(&q, peer, 50).unwrap();
        assert_eq!(before[0].origin_id.as_deref(), Some("o-1"));
        assert_eq!(before[0].stanza_id, None);
        assert!(
            upgrade_to_stanza_id(store.conn(), account_id, "o-1", Direction::Out, peer, "s-1")
                .unwrap()
        );
        let after = query(&q, peer, 50).unwrap();
        assert_eq!(before[0].id, after[0].id);
        assert_eq!(after[0].stanza_id.as_deref(), Some("s-1"));
        let diffs = diff(&before, &after);
        assert!(matches!(
            diffs.as_slice(),
            [ListDiff::Update { index: 0, .. }]
        ));
        // The new id and both old forms find the row.
        for id in [&after[0].id, "origin-id:o-1", "stanza-id:s-1"] {
            let row = find_by_timeline_id(store.conn(), account_id, id).unwrap();
            assert!(row.is_some(), "{id}");
        }
        assert!(
            find_by_timeline_id(store.conn(), account_id, "m:999")
                .unwrap()
                .is_none()
        );
    }
}
