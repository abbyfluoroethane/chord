//! `Timeline`: the messages in a room or a 1:1 chat, pre-shaped for display.

use jid::BareJid;
use rusqlite::{OptionalExtension, params};

use super::{QueryCtx, ViewItem};
use crate::store::queries::FileMeta;

/// The id of a timeline item: stable for the life of the stored row, also when the key of
/// a sent message changes to its stanza-id. `find_by_timeline_id` reads it.
pub fn item_id(rowid: i64) -> String {
    format!("m:{rowid}")
}

/// Two messages from the same sender within this time form one group.
pub const GROUP_GAP_MS: i64 = 5 * 60 * 1000;

/// The address without its resource.
fn bare_address(jid: &str) -> &str {
    jid.split_once('/').map_or(jid, |(bare, _)| bare)
}

/// One message, ready to show.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
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
    /// XEP-0446: what the sender said about the attached file.
    pub attachment_info: Option<FileMeta>,
    /// XEP-0333: for an outgoing message, how far it got.
    pub status: DeliveryStatus,
}

/// The reactions with one emoji.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    /// True if our account is one of the senders.
    pub mine: bool,
}

/// A short view of the message that a reply quotes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct ReplyPreview {
    /// Timeline id of the quoted message, if it is in the store.
    pub id: Option<String>,
    pub sender_name: String,
    /// The start of the quoted text. Empty if the message is not in the store.
    pub body: String,
}

/// How far an outgoing message got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum DeliveryStatus {
    Sent,
    Received,
    Displayed,
    /// The server or the peer answered with an error message (RFC 6121, 8.5).
    Failed,
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
    file: FileMeta,
    status: String,
    /// XEP-0421: the occupant that wrote a room message.
    occupant_id: Option<String>,
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
                reply_to_sender, oob_url,
                file_name, file_size, file_type, file_hash,
                CASE WHEN failed_at IS NOT NULL THEN 'failed' ELSE status END,
                occupant_id
         FROM messages
         WHERE account_id = ?1 AND peer = ?2
           AND NOT (failed_at IS NOT NULL AND retracted_at IS NOT NULL)
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
            file: FileMeta {
                name: row.get(13)?,
                size: row
                    .get::<_, Option<i64>>(14)?
                    .and_then(|s| u64::try_from(s).ok()),
                media_type: row.get(15)?,
                sha256: row.get(16)?,
            },
            status: row.get(17)?,
            occupant_id: row.get(18)?,
        })
    })?;
    let mut rows: Vec<Row> = rows.collect::<rusqlite::Result<_>>()?;
    rows.reverse();

    let mut items: Vec<TimelineItem> = Vec::with_capacity(rows.len());
    let mut previous_occupant: Option<String> = None;
    let mut senders: std::collections::HashMap<String, (bool, String, Option<String>)> =
        std::collections::HashMap::new();
    for row in rows {
        // One sender often writes many messages of the window: look it up once.
        let (sender_name, avatar) = match senders.get(&row.sender) {
            Some((groupchat, name, avatar)) if *groupchat == row.groupchat => {
                (name.clone(), avatar.clone())
            }
            _ => {
                let (name, avatar_owner) = display_name(q, &row.sender, row.groupchat)?;
                let avatar = match row.sender.split_once('/') {
                    Some((room, nick)) if row.groupchat => occupant_avatar_hash(q, room, nick)?,
                    _ => avatar_hash(q, &avatar_owner)?,
                };
                senders.insert(
                    row.sender.clone(),
                    (row.groupchat, name.clone(), avatar.clone()),
                );
                (name, avatar)
            }
        };
        // The same nick is the same person only if the occupant-id agrees (XEP-0421): a nick
        // that another person took later is not the same sender.
        let same_occupant = match (&previous_occupant, &row.occupant_id) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        // In a room the resource is the nick, so the full address names the person. In a 1:1
        // chat the resource names a device or a session, and it changes on each reconnect:
        // compare the bare address.
        let same_sender_as_previous = same_occupant
            && items.last().is_some_and(|p| {
                let same_person = if row.groupchat {
                    p.sender == row.sender
                } else {
                    bare_address(&p.sender) == bare_address(&row.sender)
                };
                same_person && row.timestamp - p.timestamp < GROUP_GAP_MS
            });
        previous_occupant = row.occupant_id.clone();
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
            id: item_id(row.rowid),
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
            attachment_info: if row.retracted || row.attachment.is_none() {
                None
            } else {
                Some(row.file).filter(|f| *f != FileMeta::default())
            },
            attachment: if row.retracted { None } else { row.attachment },
            status: match row.status.as_str() {
                "failed" => DeliveryStatus::Failed,
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
                id: Some(item_id(m.rowid)),
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

/// The avatar hash of `owner`. An avatar with no image yet has no hash here. The image
/// request would fail, and the view changes again when the image arrives.
pub(crate) fn avatar_hash(q: &QueryCtx<'_>, owner: &str) -> rusqlite::Result<Option<String>> {
    q.store
        .conn()
        .prepare_cached(
            "SELECT hash FROM avatars
             WHERE account_id = ?1 AND owner = ?2 AND data IS NOT NULL",
        )?
        .query_row(params![q.account_id, owner], |row| row.get(0))
        .optional()
}

/// The avatar hash of an occupant of a room. The avatar of the real JID comes first, if
/// the room gave it. Then comes the avatar that we took from the occupant itself.
pub(crate) fn occupant_avatar_hash(
    q: &QueryCtx<'_>,
    room: &str,
    nick: &str,
) -> rusqlite::Result<Option<String>> {
    let real: Option<String> = q
        .store
        .conn()
        .prepare_cached(
            "SELECT real_jid FROM occupants WHERE account_id = ?1 AND room = ?2 AND nick = ?3",
        )?
        .query_row(params![q.account_id, room, nick], |row| row.get(0))
        .optional()?
        .flatten();
    if let Some(real) = real
        && let Some(hash) = avatar_hash(q, &real)?
    {
        return Ok(Some(hash));
    }
    avatar_hash(q, &format!("{room}/{nick}"))
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
    fn an_avatar_has_a_hash_in_the_view_only_when_its_image_is_stored() {
        let store = Store::open_in_memory().unwrap();
        let account = BareJid::new("alice@chord.localhost").unwrap();
        let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
        let q = QueryCtx {
            store: &store,
            account_id,
            account: &account,
        };
        let owner = "bob@chord.localhost";
        store
            .conn()
            .execute(
                "INSERT INTO avatars (account_id, owner, hash, mime, data)
                 VALUES (?1, ?2, 'abc', 'image/png', NULL)",
                params![account_id, owner],
            )
            .unwrap();
        assert_eq!(avatar_hash(&q, owner).unwrap(), None);
        store
            .conn()
            .execute("UPDATE avatars SET data = x'01' WHERE owner = ?1", [owner])
            .unwrap();
        assert_eq!(avatar_hash(&q, owner).unwrap().as_deref(), Some("abc"));
    }

    #[test]
    fn an_occupant_avatar_comes_from_the_real_jid_first_and_then_the_full_jid() {
        let store = Store::open_in_memory().unwrap();
        let account = BareJid::new("alice@chord.localhost").unwrap();
        let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
        let q = QueryCtx {
            store: &store,
            account_id,
            account: &account,
        };
        let room = "dev@rooms.chord.localhost";
        let insert = |owner: &str, hash: &str| {
            store
                .conn()
                .execute(
                    "INSERT INTO avatars (account_id, owner, hash, mime, data)
                     VALUES (?1, ?2, ?3, 'image/png', x'01')",
                    params![account_id, owner, hash],
                )
                .unwrap();
        };
        store
            .conn()
            .execute(
                "INSERT INTO occupants (account_id, room, nick, real_jid, affiliation, role)
                 VALUES (?1, ?2, 'bobby', 'bob@chord.localhost', 'none', 'participant'),
                        (?1, ?2, 'anon', NULL, 'none', 'participant')",
                params![account_id, room],
            )
            .unwrap();
        assert_eq!(occupant_avatar_hash(&q, room, "bobby").unwrap(), None);
        insert(&format!("{room}/bobby"), "full");
        assert_eq!(
            occupant_avatar_hash(&q, room, "bobby").unwrap().as_deref(),
            Some("full")
        );
        insert("bob@chord.localhost", "real");
        assert_eq!(
            occupant_avatar_hash(&q, room, "bobby").unwrap().as_deref(),
            Some("real")
        );
        insert(&format!("{room}/anon"), "anon-hash");
        assert_eq!(
            occupant_avatar_hash(&q, room, "anon").unwrap().as_deref(),
            Some("anon-hash")
        );
        assert_eq!(occupant_avatar_hash(&q, room, "nobody").unwrap(), None);
    }

    #[test]
    fn a_new_resource_of_the_same_peer_stays_in_the_group() {
        let store = Store::open_in_memory().unwrap();
        let account = BareJid::new("alice@chord.localhost").unwrap();
        let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
        let q = QueryCtx {
            store: &store,
            account_id,
            account: &account,
        };
        let peer = "bob@chord.localhost";
        // Each reconnect gives bob a new resource.
        for (key, sender, at) in [
            ("a", "bob@chord.localhost/phone-1", 1_000),
            ("b", "bob@chord.localhost/phone-2", 2_000),
            ("c", "carol@chord.localhost/x", 3_000),
        ] {
            insert_message(
                store.conn(),
                account_id,
                &NewMessage {
                    kind: MessageKind::Chat,
                    key_kind: KeyKind::OriginId,
                    key,
                    direction: Direction::In,
                    peer,
                    sender,
                    body: "hi",
                    timestamp: Some(at),
                    extras: MessageExtras::default(),
                },
            )
            .unwrap();
        }
        let items = query(&q, peer, 50).unwrap();
        let grouped: Vec<bool> = items.iter().map(|i| i.same_sender_as_previous).collect();
        assert_eq!(grouped, [false, true, false]);
    }

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
