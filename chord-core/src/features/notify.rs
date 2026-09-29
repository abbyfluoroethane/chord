//! Notification policy: which live messages should notify the user.
//!
//! Each chat, room, or private chat with a room occupant has a level: `All`, `Mentions`,
//! or `None` (muted), and an optional mute-until time. A peer without a stored level
//! has the default: `Mentions` for a room, `All` for everything else.
//!
//! `after_store` emits `ClientEvent::Notification` for a live incoming message that the
//! policy lets through. Archive messages, our own messages, and messages with a delay
//! stamp (room history) never notify.
//!
//! A message is a mention in a room if its body has our nick as a word, or if it is a
//! XEP-0461 reply to one of our messages. XEP-0372 references are not used.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};

use super::message_ext::{self, Incoming};
use super::{Ctx, FeatureCommand, replies};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::store::Store;
use crate::store::queries::{self, Direction, MessageKind, StoredMessage};

/// The longest body preview, in characters.
const PREVIEW_CHARS: usize = 200;

/// How much a peer may notify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Every incoming message notifies.
    All,
    /// Only a mention in a room notifies.
    Mentions,
    /// Nothing notifies.
    None,
}

impl NotificationLevel {
    /// The name that the store and the CLI use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mentions => "mentions",
            Self::None => "none",
        }
    }

    /// The level with this name, if any.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "all" => Some(Self::All),
            "mentions" => Some(Self::Mentions),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// The setting of one peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotificationSetting {
    pub level: NotificationLevel,
    /// Unix time in ms. Until then, the peer does not notify at all.
    pub mute_until: Option<i64>,
}

/// A message that should notify the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// The peer as `messages.peer` stores it. Use it with `notification_level`.
    pub peer: String,
    /// The room, for a message in a room. `None` for a chat and for a private message.
    pub room: Option<BareJid>,
    /// JID of the sender: room@service/nick in a room.
    pub sender: String,
    /// The nick in a room, else the contact name.
    pub sender_name: String,
    /// The start of the body, at most 200 characters.
    pub body_preview: String,
    /// True if the message mentions us or replies to one of our messages.
    pub mention: bool,
    /// The id of the message in the timeline view.
    pub item_id: String,
}

/// A command from the public API.
pub(crate) enum Command {
    Set {
        peer: String,
        level: NotificationLevel,
        mute_until: Option<i64>,
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
    Get {
        peer: String,
        reply: oneshot::Sender<Result<NotificationSetting, ClientError>>,
    },
}

impl ClientHandle {
    /// Set the notification level of `peer`: a chat or room bare JID, or room@service/nick
    /// for a private chat. `mute_until` is a Unix time in ms. Works offline.
    pub async fn set_notification_level(
        &self,
        peer: String,
        level: NotificationLevel,
        mute_until: Option<i64>,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Notify(Command::Set {
            peer,
            level,
            mute_until,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The notification level and mute-until time of `peer`, with the default level if
    /// none is set. Works offline.
    pub async fn notification_level(
        &self,
        peer: String,
    ) -> Result<NotificationSetting, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Notify(Command::Get { peer, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    run(ctx.store, ctx.account_id, command);
}

/// Run a command against the store. It needs no session.
pub(crate) fn run(store: &Store, account_id: i64, command: Command) {
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    match command {
        Command::Set {
            peer,
            level,
            mute_until,
            reply,
        } => {
            let result = set(store, account_id, &peer, level, mute_until);
            let _ = reply.send(result.map_err(store_error));
        }
        Command::Get { peer, reply } => {
            let _ = reply.send(get(store, account_id, &peer).map_err(store_error));
        }
    }
}

/// The level that a peer has without a stored setting.
pub(crate) fn default_level(kind: MessageKind) -> NotificationLevel {
    match kind {
        MessageKind::Groupchat => NotificationLevel::Mentions,
        MessageKind::Chat => NotificationLevel::All,
    }
}

fn set(
    store: &Store,
    account_id: i64,
    peer: &str,
    level: NotificationLevel,
    mute_until: Option<i64>,
) -> rusqlite::Result<()> {
    store.conn().execute(
        "INSERT INTO notification_levels (account_id, peer, level, mute_until)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (account_id, peer)
         DO UPDATE SET level = excluded.level, mute_until = excluded.mute_until",
        params![account_id, peer, level.as_str(), mute_until],
    )?;
    Ok(())
}

/// The setting of a peer for the API. A bare JID cannot tell a room from a chat, so
/// this uses the `rooms` table to pick the default.
fn get(store: &Store, account_id: i64, peer: &str) -> rusqlite::Result<NotificationSetting> {
    let is_room = store
        .conn()
        .query_row(
            "SELECT 1 FROM rooms WHERE account_id = ?1 AND jid = ?2",
            params![account_id, peer],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let kind = if is_room {
        MessageKind::Groupchat
    } else {
        MessageKind::Chat
    };
    setting(store, account_id, peer, kind)
}

fn setting(
    store: &Store,
    account_id: i64,
    peer: &str,
    kind: MessageKind,
) -> rusqlite::Result<NotificationSetting> {
    let row: Option<(String, Option<i64>)> = store
        .conn()
        .query_row(
            "SELECT level, mute_until FROM notification_levels
             WHERE account_id = ?1 AND peer = ?2",
            params![account_id, peer],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let stored = row.and_then(|(level, mute_until)| {
        Some(NotificationSetting {
            level: NotificationLevel::parse(&level)?,
            mute_until,
        })
    });
    Ok(stored.unwrap_or(NotificationSetting {
        level: default_level(kind),
        mute_until: None,
    }))
}

/// Should a message notify? `mention` only matters for the level `Mentions`.
pub(crate) fn allows(setting: NotificationSetting, mention: bool, now: i64) -> bool {
    if setting.mute_until.is_some_and(|until| until > now) {
        return false;
    }
    match setting.level {
        NotificationLevel::All => true,
        NotificationLevel::Mentions => mention,
        NotificationLevel::None => false,
    }
}

/// True if `body` has `nick` as a whole word, ignoring case. A word boundary is a
/// character that is no letter, digit, or underscore.
pub(crate) fn mentions_nick(body: &str, nick: &str) -> bool {
    if nick.is_empty() {
        return false;
    }
    let body: Vec<char> = body.to_lowercase().chars().collect();
    let nick: Vec<char> = nick.to_lowercase().chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    if body.len() < nick.len() {
        return false;
    }
    (0..=body.len() - nick.len()).any(|start| {
        let end = start + nick.len();
        body[start..end] == nick[..]
            && (start == 0 || !is_word(body[start - 1]))
            && (end == body.len() || !is_word(body[end]))
    })
}

/// The id of the message in the timeline view.
fn item_id(rowid: i64) -> String {
    format!("m:{rowid}")
}

fn now_ms(store: &Store) -> rusqlite::Result<i64> {
    store.conn().query_row(
        "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
        [],
        |row| row.get(0),
    )
}

/// True if the message is a reply to one of our messages.
fn replies_to_us(ctx: &Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some((id, _)) = replies::reference(incoming.message) else {
        return false;
    };
    matches!(
        queries::find_message(ctx.store.conn(), ctx.account_id, incoming.peer, &id),
        Ok(Some(row)) if row.direction == Direction::Out
    )
}

fn sender_name(ctx: &Ctx<'_>, incoming: &Incoming<'_>) -> String {
    if message_ext::is_private(incoming.peer) || incoming.kind == MessageKind::Groupchat {
        return incoming
            .sender
            .rsplit_once('/')
            .map_or(incoming.sender, |(_, nick)| nick)
            .to_owned();
    }
    let bare = incoming.sender.split('/').next().unwrap_or(incoming.sender);
    let name: Option<String> = ctx
        .store
        .conn()
        .query_row(
            "SELECT name FROM contacts WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, bare],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten();
    name.unwrap_or_else(|| bare.split('@').next().unwrap_or(bare).to_owned())
}

/// A new message is in the store. Emit a notification if the policy allows it.
pub(crate) fn after_store(
    ctx: &mut Ctx<'_>,
    incoming: &Incoming<'_>,
    stored: &StoredMessage,
    live: bool,
) {
    // A delay stamp means room history or an offline message, not a new one.
    if !live || incoming.direction != Direction::In || incoming.timestamp.is_some() {
        return;
    }
    let room = if incoming.kind == MessageKind::Groupchat {
        Jid::new(incoming.peer).ok().map(|jid| jid.to_bare())
    } else {
        None
    };
    let mention = match &room {
        Some(room) => {
            let by_nick = ctx
                .state
                .muc
                .nicks
                .get(room)
                .is_some_and(|nick| mentions_nick(&stored.body, nick));
            by_nick || replies_to_us(ctx, incoming)
        }
        None => false,
    };
    let policy = setting(ctx.store, ctx.account_id, incoming.peer, incoming.kind)
        .and_then(|policy| now_ms(ctx.store).map(|now| (policy, now)));
    let (policy, now) = match policy {
        Ok(found) => found,
        Err(e) => {
            ctx.store_error("read a notification level", e);
            return;
        }
    };
    if !allows(policy, mention, now) {
        return;
    }
    let notification = Notification {
        peer: incoming.peer.to_owned(),
        room,
        sender: incoming.sender.to_owned(),
        sender_name: sender_name(ctx, incoming),
        body_preview: stored.body.chars().take(PREVIEW_CHARS).collect(),
        mention,
        item_id: item_id(stored.rowid),
    };
    ctx.emit(ClientEvent::Notification(notification));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Effect;
    use crate::features::testing::Harness;
    use crate::store::queries::{KeyKind, MessageExtras, NewMessage, insert_message};
    use xmpp_parsers::message::Message;
    use xmpp_parsers::minidom::Element;
    use xmpp_parsers::minidom::rxml::NcName;

    const BOB: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn setting_of(level: NotificationLevel, mute_until: Option<i64>) -> NotificationSetting {
        NotificationSetting { level, mute_until }
    }

    fn message_with(body: &str, reply_to: Option<&str>) -> Message {
        let mut m = Message::new(None);
        if let Some(id) = reply_to {
            m.payloads.push(
                Element::builder("reply", replies::NS_REPLY)
                    .attr(NcName::try_from("id".to_owned()).unwrap(), id)
                    .build(),
            );
        }
        m.bodies
            .insert(xmpp_parsers::message::Lang(String::new()), body.to_owned());
        m
    }

    /// Store `message` and run the hook. Returns the notifications that came out.
    fn deliver(
        h: &mut Harness,
        kind: MessageKind,
        peer: &str,
        sender: &str,
        key: &str,
        message: &Message,
        live: bool,
    ) -> Vec<Notification> {
        let body = message.bodies.values().next().unwrap().clone();
        let extras = MessageExtras {
            reply_to: replies::reference(message).map(|(id, _)| id),
            ..MessageExtras::default()
        };
        let stored = insert_message(
            h.store.conn(),
            h.account_id,
            &NewMessage {
                kind,
                key_kind: KeyKind::StanzaId,
                key,
                direction: Direction::In,
                peer,
                sender,
                body: &body,
                timestamp: None,
                extras,
            },
        )
        .unwrap()
        .unwrap();
        let incoming = Incoming {
            message,
            kind,
            direction: Direction::In,
            peer,
            sender,
            timestamp: None,
        };
        h.with_ctx(|ctx| after_store(ctx, &incoming, &stored, live));
        std::mem::take(&mut h.effects)
            .into_iter()
            .filter_map(|e| match e {
                Effect::Emit(ClientEvent::Notification(n)) => Some(n),
                _ => None,
            })
            .collect()
    }

    fn room_harness(nick: &str) -> Harness {
        let mut h = Harness::new();
        h.state
            .muc
            .nicks
            .insert(BareJid::new(ROOM).unwrap(), nick.to_owned());
        h
    }

    fn chat(h: &mut Harness, key: &str, live: bool) -> Vec<Notification> {
        let m = message_with("hello", None);
        let sender = format!("{BOB}/phone");
        deliver(h, MessageKind::Chat, BOB, &sender, key, &m, live)
    }

    fn room_msg(h: &mut Harness, key: &str, body: &str, reply: Option<&str>) -> Vec<Notification> {
        let m = message_with(body, reply);
        let sender = format!("{ROOM}/carol");
        deliver(h, MessageKind::Groupchat, ROOM, &sender, key, &m, true)
    }

    #[test]
    fn defaults() {
        let h = Harness::new();
        let level = |peer: &str| get(&h.store, h.account_id, peer).unwrap().level;
        assert_eq!(level(BOB), NotificationLevel::All);
        assert_eq!(level("dev@rooms/carol"), NotificationLevel::All);
        assert_eq!(
            default_level(MessageKind::Groupchat),
            NotificationLevel::Mentions
        );
        h.store
            .conn()
            .execute(
                "INSERT INTO rooms (account_id, jid) VALUES (?1, ?2)",
                params![h.account_id, ROOM],
            )
            .unwrap();
        assert_eq!(level(ROOM), NotificationLevel::Mentions);
    }

    #[test]
    fn chat_notifies_by_default_and_setting_is_stored() {
        let mut h = Harness::new();
        let out = chat(&mut h, "s-1", true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].sender_name, "bob");
        assert_eq!(out[0].body_preview, "hello");
        assert!(!out[0].mention);
        assert!(out[0].item_id.starts_with("m:"));
        let level = NotificationLevel::Mentions;
        set(&h.store, h.account_id, BOB, level, Some(5)).unwrap();
        assert_eq!(
            get(&h.store, h.account_id, BOB).unwrap(),
            setting_of(level, Some(5))
        );
    }

    #[test]
    fn muted_peer_does_not_notify() {
        let mut h = Harness::new();
        set(&h.store, h.account_id, BOB, NotificationLevel::None, None).unwrap();
        assert!(chat(&mut h, "s-1", true).is_empty());
    }

    #[test]
    fn mute_until_in_the_future_and_past() {
        let all = NotificationLevel::All;
        assert!(!allows(setting_of(all, Some(2000)), true, 1000));
        assert!(allows(setting_of(all, Some(500)), false, 1000));
        let mut h = Harness::new();
        set(&h.store, h.account_id, BOB, all, Some(1)).unwrap();
        assert_eq!(chat(&mut h, "s-1", true).len(), 1);
        set(&h.store, h.account_id, BOB, all, Some(i64::MAX)).unwrap();
        assert!(chat(&mut h, "s-2", true).is_empty());
    }

    #[test]
    fn archived_message_does_not_notify() {
        let mut h = Harness::new();
        assert!(chat(&mut h, "s-1", false).is_empty());
    }

    #[test]
    fn nick_mention_uses_word_boundaries() {
        assert!(mentions_nick("hi Al, look", "al"));
        assert!(mentions_nick("al", "AL"));
        assert!(mentions_nick("(al)", "al"));
        assert!(!mentions_nick("also here", "al"));
        assert!(!mentions_nick("pal", "al"));
        assert!(!mentions_nick("al_ex", "al"));
        assert!(!mentions_nick("anything", ""));
    }

    #[test]
    fn room_notifies_on_mention_only_by_default() {
        let mut h = room_harness("al");
        assert!(room_msg(&mut h, "r-1", "also here", None).is_empty());
        let out = room_msg(&mut h, "r-2", "hey AL!", None);
        assert_eq!(out.len(), 1);
        assert!(out[0].mention);
        assert_eq!(out[0].room, Some(BareJid::new(ROOM).unwrap()));
        assert_eq!(out[0].sender_name, "carol");
    }

    #[test]
    fn reply_to_own_message_is_a_mention() {
        let mut h = room_harness("al");
        insert_message(
            h.store.conn(),
            h.account_id,
            &NewMessage {
                kind: MessageKind::Groupchat,
                key_kind: KeyKind::StanzaId,
                key: "own-1",
                direction: Direction::Out,
                peer: ROOM,
                sender: "dev@rooms.chord.localhost/al",
                body: "mine",
                timestamp: None,
                extras: MessageExtras {
                    message_id: Some("own-1".into()),
                    ..MessageExtras::default()
                },
            },
        )
        .unwrap();
        assert!(room_msg(&mut h, "r-1", "no", Some("other")).is_empty());
        let out = room_msg(&mut h, "r-2", "yes", Some("own-1"));
        assert_eq!(out.len(), 1);
        assert!(out[0].mention);
    }

    #[test]
    fn preview_is_cut_to_200_characters() {
        let mut h = Harness::new();
        let long = "é".repeat(300);
        let m = message_with(&long, None);
        let out = deliver(&mut h, MessageKind::Chat, BOB, BOB, "s-1", &m, true);
        assert_eq!(out[0].body_preview.chars().count(), 200);
    }
}
