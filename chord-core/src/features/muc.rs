//! Multi-user chat (XEP-0045): join, leave, occupants, and groupchat messages.
//!
//! Private messages between occupants (section 7.5) are chat rows with the peer
//! `room@service/nick`. Their timeline is `ViewKey::PrivateTimeline`.

use std::collections::HashMap;

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::muc::Muc;
use xmpp_parsers::muc::muc::History;
use xmpp_parsers::muc::user::{Affiliation, MucUser, Role, Status};
use xmpp_parsers::presence::{Presence, Show, Type as PresenceType};
use xmpp_parsers::stanza_error::{DefinedCondition, StanzaError};
use xmpp_parsers::stanza_id::OriginId;

use super::chat::{MessageIds, delay_ms};
use super::message_ext::{self, Incoming, Outgoing};
use super::{Ctx, IqResponse, bookmarks, mam, new_id};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::store::queries::{self, Direction, KeyKind, MessageExtras, MessageKind, NewMessage};
use crate::views::{ChannelScope, ViewKey};

const NS_MUC_OWNER: &str = "http://jabber.org/protocol/muc#owner";
const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";

type Reply = oneshot::Sender<Result<(), ClientError>>;
type PrivateReply = oneshot::Sender<Result<String, ClientError>>;

/// A join that waits for the self-presence or an error.
#[derive(Debug)]
pub(super) struct Join {
    nick: String,
    /// True for a nick change in a room that we are in already.
    changing_nick: bool,
    replies: Vec<Reply>,
}

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    pub(super) joins: HashMap<BareJid, Join>,
    /// The rooms that we are in, with our nick.
    pub(super) nicks: HashMap<BareJid, String>,
    /// The rooms that were joined before this session. `on_connected` joins them again.
    previous: Vec<BareJid>,
    /// Messages for rooms that we are joining. They go out when the join completes.
    outbox: HashMap<BareJid, Vec<Message>>,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The answer to the configuration of a new room, with the join replies that wait
    /// for it: others cannot enter the room until it is unlocked.
    InstantRoom(BareJid, Vec<Reply>),
}

/// A command from the public API.
pub(crate) enum Command {
    Join {
        room: BareJid,
        nick: String,
        password: Option<String>,
        reply: Reply,
    },
    Leave {
        room: BareJid,
        reply: Reply,
    },
    AddBookmark {
        room: BareJid,
        name: Option<String>,
        autojoin: bool,
        nick: Option<String>,
        reply: Reply,
    },
    RemoveBookmark {
        room: BareJid,
        reply: Reply,
    },
    SendPrivate {
        room: BareJid,
        nick: String,
        body: String,
        reply: PrivateReply,
    },
}

impl ClientHandle {
    /// Join a room and wait for the answer of the room. Fails with `ClientError::Server`
    /// when the room refuses: nick in use (`conflict`), wrong password
    /// (`not-authorized`), members only (`registration-required`), or banned
    /// (`forbidden`). The join does not fetch history: MAM does.
    pub async fn join_room(
        &self,
        room: BareJid,
        nick: String,
        password: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::Join {
            room,
            nick,
            password,
            reply,
        })
        .await
    }

    /// Leave a room. A bookmark with autojoin stays: call `remove_bookmark` to remove it.
    pub async fn leave_room(&self, room: BareJid) -> Result<(), ClientError> {
        self.room_command(|reply| Command::Leave { room, reply })
            .await
    }

    /// Save a room in the bookmarks of the account (XEP-0402). This changes the bookmark
    /// only: it does not join or leave the room.
    pub async fn add_bookmark(
        &self,
        room: BareJid,
        name: Option<String>,
        autojoin: bool,
        nick: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::AddBookmark {
            room,
            name,
            autojoin,
            nick,
            reply,
        })
        .await
    }

    /// Remove a room from the bookmarks. This does not leave the room.
    pub async fn remove_bookmark(&self, room: BareJid) -> Result<(), ClientError> {
        self.room_command(|reply| Command::RemoveBookmark { room, reply })
            .await
    }

    /// Send a private message to one occupant of a room (XEP-0045, section 7.5) and store
    /// it. Returns its origin-id. Fails with `ClientError::Invalid` when we are not in the
    /// room or the nick is not an occupant. Read the answers with `private_timeline`.
    pub async fn send_private(
        &self,
        room: BareJid,
        nick: String,
        body: String,
    ) -> Result<String, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Muc(Command::SendPrivate {
            room,
            nick,
            body,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    async fn room_command(
        &self,
        command: impl FnOnce(Reply) -> Command,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Muc(command(reply)))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// The state for a new session (not a resumption). Fails the joins that wait, and keeps
/// the list of joined rooms, which the actor clears next. Call it before that.
pub(crate) fn next_session(ctx: &mut Ctx<'_>) -> State {
    let previous = db(
        ctx,
        "read the joined rooms",
        ctx.store
            .conn()
            .prepare_cached("SELECT jid FROM rooms WHERE account_id = ?1 AND joined = 1")
            .and_then(|mut stmt| {
                stmt.query_map(params![ctx.account_id], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            }),
    )
    .unwrap_or_default()
    .iter()
    .filter_map(|jid| BareJid::new(jid).ok())
    .collect();
    for (_, join) in ctx.state.muc.joins.drain() {
        for reply in join.replies {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
    State {
        previous,
        ..State::default()
    }
}

/// Join the rooms of the last session. Bookmarks with autojoin join after the bookmarks
/// arrive.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    for room in std::mem::take(&mut ctx.state.muc.previous) {
        join_room(ctx, &room, None, None, None);
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::InstantRoom(room, replies) => {
            let result = match response {
                IqResponse::Result(_) => Ok(()),
                IqResponse::Error(e) => {
                    let text = error_text(&e);
                    ctx.emit(ClientEvent::Notice(format!(
                        "Cannot unlock the new room {room}: {text}"
                    )));
                    Err(ClientError::Server(text))
                }
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            for reply in replies {
                let _ = reply.send(result.clone());
            }
        }
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Join {
            room,
            nick,
            password,
            reply,
        } => join_room(ctx, &room, Some(nick), password, Some(reply)),
        Command::Leave { room, reply } => {
            let _ = reply.send(leave(ctx, &room));
        }
        Command::AddBookmark {
            room,
            name,
            autojoin,
            nick,
            reply,
        } => bookmarks::add(ctx, room, name, autojoin, nick, reply),
        Command::RemoveBookmark { room, reply } => bookmarks::remove(ctx, room, reply),
        Command::SendPrivate {
            room,
            nick,
            body,
            reply,
        } => {
            let _ = reply.send(send_private(ctx, &room, &nick, body));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    if let Command::SendPrivate { reply, .. } = command {
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
    let (Command::Join { reply, .. }
    | Command::Leave { reply, .. }
    | Command::AddBookmark { reply, .. }
    | Command::RemoveBookmark { reply, .. }) = command
    else {
        return;
    };
    let _ = reply.send(Err(ClientError::NotConnected));
}

/// The row of a room.
struct RoomRow {
    nick: Option<String>,
    password: Option<String>,
    joined: bool,
}

fn room_row(ctx: &Ctx<'_>, room: &BareJid) -> Option<RoomRow> {
    db(
        ctx,
        "read a room",
        ctx.store
            .conn()
            .prepare_cached(
                "SELECT nick, password, joined FROM rooms
                 WHERE account_id = ?1 AND jid = ?2",
            )
            .and_then(|mut stmt| {
                stmt.query_row(params![ctx.account_id, room.as_str()], |row| {
                    Ok(RoomRow {
                        nick: row.get(0)?,
                        password: row.get(1)?,
                        joined: row.get::<_, i64>(2)? != 0,
                    })
                })
                .optional()
            }),
    )
    .flatten()
}

/// Whether the account knows `room`: from a bookmark or from a join.
pub(crate) fn is_room(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    room_row(ctx, room).is_some()
}

/// The stored password of a room.
pub(crate) fn stored_password(ctx: &Ctx<'_>, room: &BareJid) -> Option<String> {
    room_row(ctx, room).and_then(|row| row.password)
}

/// Leave a room because the bookmarks say so. A store error or an unknown room is not
/// an error here.
pub(crate) fn leave_room_quietly(ctx: &mut Ctx<'_>, room: &BareJid) {
    if let Err(e) = leave(ctx, room) {
        log::debug!("cannot leave {room}: {e}");
    }
}

/// Whether we are in the room now, or a join is on its way.
pub(crate) fn is_joined_or_joining(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    ctx.state.muc.nicks.contains_key(room) || ctx.state.muc.joins.contains_key(room)
}

/// Add a room row, or set its nick and password. `None` keeps the stored value.
fn ensure_room(ctx: &Ctx<'_>, room: &BareJid, nick: Option<&str>, password: Option<&str>) {
    db(
        ctx,
        "store a room",
        ctx.store.conn().execute(
            "INSERT INTO rooms (account_id, jid, nick, password) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account_id, jid) DO UPDATE SET
                nick = COALESCE(excluded.nick, nick),
                password = COALESCE(excluded.password, password)",
            params![ctx.account_id, room.as_str(), nick, password],
        ),
    );
}

fn set_joined(ctx: &Ctx<'_>, room: &BareJid, joined: bool) {
    db(
        ctx,
        "set the joined flag",
        ctx.store.conn().execute(
            "UPDATE rooms SET joined = ?3 WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, room.as_str(), joined],
        ),
    );
}

fn clear_occupants(ctx: &Ctx<'_>, room: &BareJid) {
    db(
        ctx,
        "clear occupants",
        ctx.store.conn().execute(
            "DELETE FROM occupants WHERE account_id = ?1 AND room = ?2",
            params![ctx.account_id, room.as_str()],
        ),
    );
}

/// Log a store error and return the value of a good result.
fn db<T>(ctx: &Ctx<'_>, what: &str, result: rusqlite::Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            ctx.store_error(what, e);
            None
        }
    }
}

/// Mark the views that show a room.
pub(crate) fn mark_room(ctx: &mut Ctx<'_>, room: &BareJid) {
    ctx.changed(ViewKey::Timeline(room.clone()));
    ctx.changed(ViewKey::MemberList(room.clone()));
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
    let spaces: Vec<(String, String)> = db(
        ctx,
        "read the spaces of a room",
        ctx.store
            .conn()
            .prepare_cached(
                "SELECT DISTINCT service, node FROM space_items
                 WHERE account_id = ?1 AND room_jid = ?2",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![ctx.account_id, room.as_str()], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect::<rusqlite::Result<_>>()
            }),
    )
    .unwrap_or_default();
    for (service, node) in spaces {
        ctx.changed(ViewKey::ChannelList(ChannelScope::Space { service, node }));
    }
}

/// Our nick in a room: the nick of the joined room, else the nick that we asked for.
fn our_nick(ctx: &Ctx<'_>, room: &BareJid) -> Option<String> {
    ctx.state
        .muc
        .nicks
        .get(room)
        .cloned()
        .or_else(|| room_row(ctx, room).and_then(|row| row.nick))
}

/// Join a room. The nick and the password default to the stored ones, and the nick to the
/// local part of our JID. `reply` gets the answer of the room.
pub(crate) fn join_room(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    nick: Option<String>,
    password: Option<String>,
    reply: Option<Reply>,
) {
    let row = room_row(ctx, room);
    let nick = nick
        .or_else(|| row.as_ref().and_then(|r| r.nick.clone()))
        .or_else(|| ctx.account.node().map(|n| n.as_str().to_owned()))
        .unwrap_or_else(|| "chord".to_owned());
    let password = password.or_else(|| row.and_then(|r| r.password));

    let current = ctx.state.muc.nicks.get(room).cloned();
    if current.as_deref() == Some(nick.as_str()) {
        if let Some(reply) = reply {
            let _ = reply.send(Ok(()));
        }
        return;
    }
    if let Some(join) = ctx.state.muc.joins.get_mut(room) {
        if join.nick == nick {
            join.replies.extend(reply);
        } else if let Some(reply) = reply {
            let _ = reply.send(Err(ClientError::Invalid(format!(
                "a join of {room} with the nick {} is in progress",
                join.nick
            ))));
        }
        return;
    }
    let target = match room.with_resource_str(&nick) {
        Ok(full) => Jid::from(full),
        Err(e) => {
            if let Some(reply) = reply {
                let _ = reply.send(Err(ClientError::Invalid(format!("bad nick {nick}: {e}"))));
            }
            return;
        }
    };
    ensure_room(ctx, room, Some(&nick), password.as_deref());
    let mut muc = Muc::new().with_history(History::new().with_maxstanzas(0));
    muc.password = password;
    ctx.send(Presence::available().with_to(target).with_payload(muc));
    ctx.state.muc.joins.insert(
        room.clone(),
        Join {
            nick,
            changing_nick: current.is_some(),
            replies: reply.into_iter().collect(),
        },
    );
    mark_room(ctx, room);
}

/// Leave a room.
fn leave(ctx: &mut Ctx<'_>, room: &BareJid) -> Result<(), ClientError> {
    let Some(row) = room_row(ctx, room) else {
        return Err(ClientError::Invalid(format!("unknown room {room}")));
    };
    let pending = ctx.state.muc.joins.remove(room);
    let nick = ctx
        .state
        .muc
        .nicks
        .remove(room)
        .or_else(|| pending.as_ref().map(|j| j.nick.clone()));
    if let Some(join) = pending {
        for reply in join.replies {
            let _ = reply.send(Err(ClientError::Invalid("left the room".into())));
        }
    }
    if let Some(nick) = nick
        && let Ok(full) = room.with_resource_str(&nick)
    {
        ctx.send(Presence::unavailable().with_to(Jid::from(full)));
    } else if !row.joined {
        return Ok(());
    }
    set_joined(ctx, room, false);
    clear_occupants(ctx, room);
    mark_room(ctx, room);
    Ok(())
}

/// Send a message to a room or a contact. A room is a room that the account knows.
pub(crate) fn send_chat(ctx: &mut Ctx<'_>, to: Jid, body: String) -> Result<String, ClientError> {
    let room = to.to_bare();
    if !is_room(ctx, &room) {
        return Ok(super::chat::send(ctx, to, body));
    }
    if let Some(nick) = to.resource() {
        return send_private(ctx, &room, nick.as_str(), body);
    }
    send(ctx, &room, body)
}

/// Whether `nick` is an occupant of `room` now.
fn is_occupant(ctx: &Ctx<'_>, room: &BareJid, nick: &str) -> bool {
    db(
        ctx,
        "read an occupant",
        ctx.store
            .conn()
            .prepare_cached(
                "SELECT 1 FROM occupants WHERE account_id = ?1 AND room = ?2 AND nick = ?3",
            )
            .and_then(|mut stmt| {
                stmt.query_row(params![ctx.account_id, room.as_str(), nick], |_| Ok(()))
                    .optional()
            }),
    )
    .flatten()
    .is_some()
}

/// Send a private message to an occupant and store it. Returns its origin-id. The
/// message is a `chat` with an empty muc#user element, so that carbons skip it.
fn send_private(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    nick: &str,
    body: String,
) -> Result<String, ClientError> {
    // While the join runs, the occupant list is not complete. Then the message waits
    // in the outbox, and the room answers with an error if the nick is not there.
    let joined = ctx.state.muc.nicks.contains_key(room);
    if joined && !is_occupant(ctx, room, nick) {
        return Err(ClientError::Invalid(format!("{nick} is not in {room}")));
    }
    let to = Jid::new(&format!("{room}/{nick}"))
        .map_err(|e| ClientError::Invalid(format!("bad nick {nick}: {e}")))?;
    let origin_id = new_id();
    let mut message = Message::chat(to.clone())
        .with_body("".into(), body.clone())
        .with_payload(OriginId {
            id: origin_id.clone(),
        });
    message
        .payloads
        .push(Element::builder("x", NS_MUC_USER).build());
    message.id = Some(Id(origin_id.clone()));
    send_to_room(ctx, room, message)?;
    let our_nick = ctx
        .state
        .muc
        .nicks
        .get(room)
        .cloned()
        .or_else(|| ctx.state.muc.joins.get(room).map(|j| j.nick.clone()))
        .unwrap_or_default();

    let peer = to.to_string();
    let sender = format!("{room}/{our_nick}");
    let new = NewMessage {
        kind: MessageKind::Chat,
        key_kind: KeyKind::OriginId,
        key: &origin_id,
        direction: Direction::Out,
        peer: &peer,
        sender: &sender,
        body: &body,
        timestamp: None,
        extras: MessageExtras {
            message_id: Some(origin_id.clone()),
            origin_id: Some(origin_id.clone()),
            ..MessageExtras::default()
        },
    };
    if let Err(e) = queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        ctx.store_error("store a sent message", e);
    }
    private_changed(ctx, room, nick);
    Ok(origin_id)
}

fn private_changed(ctx: &mut Ctx<'_>, room: &BareJid, nick: &str) {
    ctx.changed(ViewKey::PrivateTimeline(room.clone(), nick.to_owned()));
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
}

/// Store a private message: a `chat` or `normal` message between our account and a full
/// JID of a room that we know (XEP-0045, section 7.5). It comes live, as a carbon of a
/// message that another client sent, or from the archive. Returns false if `message` is
/// no private message. With `live`, also report it as `MessageReceived`. Corrections,
/// reactions, and markers are not supported in private messages.
pub(crate) fn store_private(
    ctx: &mut Ctx<'_>,
    message: &Message,
    ids: &MessageIds,
    timestamp: Option<i64>,
    live: bool,
) -> bool {
    if !matches!(message.type_, MessageType::Chat | MessageType::Normal) {
        return false;
    }
    let Some(from) = &message.from else {
        return false;
    };
    let (direction, occupant) = if from.to_bare() == *ctx.account {
        match &message.to {
            Some(to) => (Direction::Out, to),
            None => return false,
        }
    } else {
        (Direction::In, from)
    };
    let Some(nick) = occupant.resource().map(|n| n.as_str().to_owned()) else {
        return false;
    };
    let room = occupant.to_bare();
    if !is_room(ctx, &room) {
        return false;
    }
    let Some(body) = message_ext::body(message) else {
        return true;
    };
    let peer = format!("{room}/{nick}");
    let sender = match direction {
        Direction::In => from.to_string(),
        Direction::Out => format!(
            "{room}/{}",
            our_nick(ctx, &room).unwrap_or_else(|| ctx.account.to_string())
        ),
    };

    if let (Some(stanza_id), Some(origin_id)) = (&ids.stanza_id, &ids.origin_id) {
        match queries::upgrade_to_stanza_id(
            ctx.store.conn(),
            ctx.account_id,
            origin_id,
            direction,
            &peer,
            stanza_id,
        ) {
            Ok(true) => {
                private_changed(ctx, &room, &nick);
                return true;
            }
            Ok(false) => {}
            Err(e) => ctx.store_error("update a message key", e),
        }
    }
    let ids = MessageIds {
        stanza_id: ids.stanza_id.clone(),
        origin_id: ids.origin_id.clone(),
    };
    let extras = message_ext::extras(message, &ids);
    let Some((key_kind, key)) = ids.key() else {
        log::debug!("private message from {from} has no stanza-id or origin-id. Not stored.");
        return true;
    };
    let new = NewMessage {
        kind: MessageKind::Chat,
        key_kind,
        key: &key,
        direction,
        peer: &peer,
        sender: &sender,
        body: &body,
        timestamp,
        extras,
    };
    match queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        Ok(Some(stored)) => {
            if live {
                ctx.emit(ClientEvent::MessageReceived(stored));
            }
            private_changed(ctx, &room, &nick);
        }
        Ok(None) => log::debug!("message {key} is stored already"),
        Err(e) => ctx.store_error("store a message", e),
    }
    true
}

/// Send a message to a room, and store nothing. If we are not in the room yet (for
/// example right after login, while the autojoin runs), the message waits in the outbox
/// and goes out when the join completes. The send starts the join if none runs. Fails if
/// the room is unknown.
pub(crate) fn send_to_room(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    message: Message,
) -> Result<(), ClientError> {
    if ctx.state.muc.nicks.contains_key(room) {
        ctx.send(message);
        return Ok(());
    }
    if room_row(ctx, room).is_none() {
        return Err(ClientError::Invalid(format!("not in the room {room}")));
    }
    if !ctx.state.muc.joins.contains_key(room) {
        join_room(ctx, room, None, None, None);
    }
    ctx.state
        .muc
        .outbox
        .entry(room.clone())
        .or_default()
        .push(message);
    Ok(())
}

/// Send a groupchat message and store it. Returns its origin-id.
///
/// If we are not in the room yet (for example right after login, while the autojoin
/// runs), the message waits in the outbox and goes out when the join completes. The
/// send starts the join if none runs.
pub(crate) fn send(ctx: &mut Ctx<'_>, room: &BareJid, body: String) -> Result<String, ClientError> {
    send_with_payload(ctx, room, body, None)
}

/// Send a groupchat message with an extra payload (for example an XEP-0066 OOB URL), and
/// store it. Returns its origin-id. See `send`.
pub(crate) fn send_with_payload(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    body: String,
    payload: Option<Element>,
) -> Result<String, ClientError> {
    let mut out = Outgoing::default();
    out.payloads.extend(payload);
    send_message(ctx, room, body, out)
}

/// Send a groupchat message with extra payloads and references, and store it. Returns
/// its origin-id, which is also its `id` attribute. See `send`.
pub(crate) fn send_message(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    body: String,
    out: Outgoing,
) -> Result<String, ClientError> {
    let origin_id = new_id();
    let mut message = Message::groupchat(Jid::from(room.clone()))
        .with_body("".into(), body.clone())
        .with_payload(OriginId {
            id: origin_id.clone(),
        });
    message.payloads.extend(message_ext::outgoing_payloads(ctx));
    message.payloads.extend(out.payloads);
    message.id = Some(Id(origin_id.clone()));
    send_to_room(ctx, room, message)?;
    let sender_nick = ctx
        .state
        .muc
        .nicks
        .get(room)
        .cloned()
        .or_else(|| ctx.state.muc.joins.get(room).map(|j| j.nick.clone()))
        .unwrap_or_default();

    let peer = room.to_string();
    let sender = format!("{room}/{sender_nick}");
    let new = NewMessage {
        kind: MessageKind::Groupchat,
        key_kind: KeyKind::OriginId,
        key: &origin_id,
        direction: Direction::Out,
        peer: &peer,
        sender: &sender,
        body: &body,
        timestamp: None,
        extras: MessageExtras {
            message_id: Some(origin_id.clone()),
            origin_id: Some(origin_id.clone()),
            ..out.extras
        },
    };
    if let Err(e) = queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        ctx.store_error("store a sent message", e);
    }
    mark_room(ctx, room);
    Ok(origin_id)
}

/// A message from a room. Returns true if this module handled it.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(from) = &message.from else {
        return false;
    };
    let room = from.to_bare();
    if !is_room(ctx, &room) {
        return false;
    }
    match message.type_ {
        MessageType::Groupchat => {
            if let Some((_, subject)) = message.get_best_subject(vec![]) {
                set_subject(ctx, &room, subject);
            }
            let ids = MessageIds::of(message, &room);
            store(ctx, &room, message, ids, delay_ms(message), true);
        }
        MessageType::Error => log::warn!("error message from the room {from}"),
        // Private messages between occupants, and mediated invites.
        _ => {
            let ids = MessageIds::of(message, ctx.account);
            if !store_private(ctx, message, &ids, delay_ms(message), true) {
                log::debug!("dropped a message from {from}: not a private message");
            }
        }
    }
    true
}

fn set_subject(ctx: &mut Ctx<'_>, room: &BareJid, subject: &str) {
    let subject = (!subject.is_empty()).then_some(subject);
    db(
        ctx,
        "store a subject",
        ctx.store.conn().execute(
            "UPDATE rooms SET subject = ?3 WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, room.as_str(), subject],
        ),
    );
    ctx.changed(ViewKey::Timeline(room.clone()));
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
}

/// Store a groupchat message from a room archive (MAM). `archive_id` is the MAM result
/// id, which is the stanza-id of the room. The MAM module calls it.
pub(crate) fn store_archived(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    message: &Message,
    archive_id: &str,
    timestamp: Option<i64>,
) {
    let mut ids = MessageIds::of(message, room);
    ids.stanza_id = Some(archive_id.to_owned());
    // History is not news: it goes to the store and the views, but not to the events.
    store(
        ctx,
        room,
        message,
        ids,
        timestamp.or_else(|| delay_ms(message)),
        false,
    );
}

/// Store a groupchat message. With `live`, also report it as `MessageReceived`.
fn store(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    message: &Message,
    ids: MessageIds,
    timestamp: Option<i64>,
    live: bool,
) {
    let Some(from) = &message.from else {
        return;
    };
    // A message from the room itself, with no nick, is a status text. Skip it.
    let Some(nick) = from.resource() else {
        return;
    };
    let direction = if our_nick(ctx, room).as_deref() == Some(nick.as_str()) {
        Direction::Out
    } else {
        Direction::In
    };
    let sender_jid = from.to_string();
    let incoming = Incoming {
        message,
        kind: MessageKind::Groupchat,
        direction,
        peer: room,
        sender: &sender_jid,
        timestamp,
    };
    // Corrections, retractions, reactions, and markers change earlier messages.
    if message_ext::intercept(ctx, &incoming) {
        return;
    }
    let Some(body) = message_ext::body(message) else {
        return;
    };
    let peer = room.to_string();

    // Our own message comes back from the room with its origin-id. It has a stanza-id now.
    if let (Some(stanza_id), Some(origin_id)) = (&ids.stanza_id, &ids.origin_id) {
        match queries::upgrade_to_stanza_id(
            ctx.store.conn(),
            ctx.account_id,
            origin_id,
            direction,
            &peer,
            stanza_id,
        ) {
            Ok(true) => {
                ctx.changed(ViewKey::Timeline(room.clone()));
                return;
            }
            Ok(false) => {}
            Err(e) => ctx.store_error("update a message key", e),
        }
    }
    let extras = message_ext::extras(message, &ids);
    let Some((key_kind, key)) = ids.key() else {
        log::debug!("groupchat message from {from} has no stanza-id or origin-id. Not stored.");
        return;
    };
    let new = NewMessage {
        kind: MessageKind::Groupchat,
        key_kind,
        key: &key,
        direction,
        peer: &peer,
        sender: &sender_jid,
        body: &body,
        timestamp,
        extras,
    };
    match queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        Ok(Some(stored)) => {
            message_ext::after_store(ctx, &incoming, &stored);
            if live {
                ctx.emit(ClientEvent::MessageReceived(stored));
            }
            ctx.changed(ViewKey::Timeline(room.clone()));
            mark_room(ctx, room);
        }
        Ok(None) => log::debug!("message {key} is stored already"),
        Err(e) => ctx.store_error("store a message", e),
    }
}

/// A presence from a room occupant. Returns true if this module handled it.
pub(crate) fn on_presence(ctx: &mut Ctx<'_>, presence: &Presence) -> bool {
    let Some(from) = &presence.from else {
        return false;
    };
    let room = from.to_bare();
    if !is_room(ctx, &room) {
        return false;
    }
    let nick = from.resource().map(|r| r.as_str().to_owned());
    match presence.type_ {
        PresenceType::Error => on_error(ctx, &room, nick.as_deref(), presence),
        PresenceType::Unavailable => {
            if let Some(nick) = nick {
                on_unavailable(ctx, &room, &nick, presence);
            }
        }
        PresenceType::None => {
            if let Some(nick) = nick {
                on_available(ctx, &room, &nick, presence);
            }
        }
        _ => {}
    }
    true
}

fn muc_user(presence: &Presence) -> Option<MucUser> {
    presence
        .payloads
        .iter()
        .find_map(|p| MucUser::try_from(p.clone()).ok())
}

/// The error of a join, or of a later presence.
fn on_error(ctx: &mut Ctx<'_>, room: &BareJid, nick: Option<&str>, presence: &Presence) {
    let error = presence
        .payloads
        .iter()
        .find_map(|p| StanzaError::try_from(p.clone()).ok());
    let text = error
        .as_ref()
        .map_or_else(|| "error".to_owned(), error_text);
    // The error of a join comes from the room, or from the nick that we tried.
    if let Some(join) = ctx.state.muc.joins.get(room)
        && nick.is_none_or(|n| n == join.nick)
    {
        let join = ctx.state.muc.joins.remove(room).expect("join is there");
        if !join.changing_nick {
            set_joined(ctx, room, false);
        }
        for reply in join.replies {
            let _ = reply.send(Err(ClientError::Server(text.clone())));
        }
        ctx.emit(ClientEvent::Notice(format!("Cannot join {room}: {text}")));
        drop_outbox(ctx, room);
        mark_room(ctx, room);
    } else {
        ctx.emit(ClientEvent::Notice(format!("Error from {room}: {text}")));
    }
}

fn on_unavailable(ctx: &mut Ctx<'_>, room: &BareJid, nick: &str, presence: &Presence) {
    let user = muc_user(presence);
    let has = |s: Status| user.as_ref().is_some_and(|u| u.status.contains(&s));
    let ours = our_nick(ctx, room);
    let pending_nick = ctx.state.muc.joins.get(room).map(|j| j.nick.as_str());
    let is_self =
        has(Status::SelfPresence) || ours.as_deref() == Some(nick) || pending_nick == Some(nick);

    let new_nick = if has(Status::NewNick) {
        user.as_ref()
            .and_then(|u| u.items.first())
            .and_then(|i| i.nick.clone())
    } else {
        None
    };
    delete_occupant(ctx, room, nick);
    if let Some(new_nick) = new_nick {
        // A nick change. The presence with the new nick follows.
        if is_self && ctx.state.muc.nicks.contains_key(room) {
            ctx.state.muc.nicks.insert(room.clone(), new_nick);
        }
    } else if is_self {
        let was_in = ctx.state.muc.nicks.remove(room).is_some();
        let joining = ctx.state.muc.joins.remove(room);
        for reply in joining.into_iter().flat_map(|j| j.replies) {
            let _ = reply.send(Err(ClientError::Server("removed from the room".into())));
        }
        drop_outbox(ctx, room);
        set_joined(ctx, room, false);
        clear_occupants(ctx, room);
        if was_in {
            let reason = user
                .as_ref()
                .and_then(|u| u.items.first())
                .and_then(|i| i.reason.as_ref())
                .map(|r| format!(": {}", r.0))
                .unwrap_or_default();
            let what = if has(Status::Banned) {
                "banned from"
            } else if has(Status::Kicked) {
                "kicked from"
            } else {
                "removed from"
            };
            ctx.emit(ClientEvent::Notice(format!(
                "You were {what} {room}{reason}"
            )));
        }
    }
    mark_room(ctx, room);
}

fn delete_occupant(ctx: &Ctx<'_>, room: &BareJid, nick: &str) {
    db(
        ctx,
        "remove an occupant",
        ctx.store.conn().execute(
            "DELETE FROM occupants WHERE account_id = ?1 AND room = ?2 AND nick = ?3",
            params![ctx.account_id, room.as_str(), nick],
        ),
    );
}

fn on_available(ctx: &mut Ctx<'_>, room: &BareJid, nick: &str, presence: &Presence) {
    let user = muc_user(presence);
    let has = |s: Status| user.as_ref().is_some_and(|u| u.status.contains(&s));
    let pending_nick = ctx.state.muc.joins.get(room).map(|j| j.nick.as_str());
    let is_self = has(Status::SelfPresence)
        || ctx.state.muc.nicks.get(room).map(String::as_str) == Some(nick)
        || pending_nick == Some(nick);
    // Presences of a room that we left, or a join that we do not know, are stale.
    if !is_joined_or_joining(ctx, room) {
        return;
    }
    let item = user.as_ref().and_then(|u| u.items.first());
    let affiliation = item.map_or("none", |i| affiliation_str(&i.affiliation));
    let role = item.map_or("participant", |i| role_str(&i.role));
    let real_jid = item
        .and_then(|i| i.jid.as_ref())
        .map(|jid| jid.to_bare().to_string());
    let show = presence.show.as_ref().map(show_str);
    db(
        ctx,
        "store an occupant",
        ctx.store.conn().execute(
            "INSERT INTO occupants (account_id, room, nick, real_jid, affiliation, role, show)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (account_id, room, nick) DO UPDATE SET
                real_jid = excluded.real_jid, affiliation = excluded.affiliation,
                role = excluded.role, show = excluded.show",
            params![
                ctx.account_id,
                room.as_str(),
                nick,
                real_jid,
                affiliation,
                role,
                show
            ],
        ),
    );
    if is_self && let Some(join) = ctx.state.muc.joins.remove(room) {
        ctx.state.muc.nicks.insert(room.clone(), nick.to_owned());
        set_joined(ctx, room, true);
        if has(Status::RoomHasBeenCreated) {
            // The join completes when the room is configured and unlocked.
            unlock_room(ctx, room, join.replies);
        } else {
            for reply in join.replies {
                let _ = reply.send(Ok(()));
            }
        }
        if !join.changing_nick {
            mam::catch_up_room(ctx, room);
        }
        for message in ctx.state.muc.outbox.remove(room).unwrap_or_default() {
            ctx.send(message);
        }
    }
    mark_room(ctx, room);
}

/// True while messages wait for a room join.
pub(crate) fn has_outbox(state: &State) -> bool {
    state.outbox.values().any(|m| !m.is_empty())
}

/// Drop the queued messages of a room whose join failed, and say so.
fn drop_outbox(ctx: &mut Ctx<'_>, room: &BareJid) {
    if let Some(messages) = ctx.state.muc.outbox.remove(room) {
        ctx.emit(ClientEvent::Notice(format!(
            "{} message(s) to {room} were not sent: the join failed",
            messages.len()
        )));
    }
}

/// Configure a new room (XEP-0045, 10.1.3). Until then the room is locked. A Chord room
/// is a channel, so it is persistent (it stays when the last occupant leaves) and it keeps
/// an archive for MAM. The server keeps its defaults for the other fields.
fn unlock_room(ctx: &mut Ctx<'_>, room: &BareJid, replies: Vec<Reply>) {
    let query: Element = format!(
        "<query xmlns='{NS_MUC_OWNER}'><x xmlns='jabber:x:data' type='submit'>\
         <field var='FORM_TYPE'><value>http://jabber.org/protocol/muc#roomconfig</value></field>\
         <field var='muc#roomconfig_persistentroom'><value>1</value></field>\
         <field var='muc#roomconfig_enablearchiving'><value>1</value></field>\
         </x></query>"
    )
    .parse()
    .expect("static XML");
    let iq = Iq::Set {
        from: None,
        to: Some(Jid::from(room.clone())),
        id: String::new(),
        payload: query,
    };
    ctx.request(
        iq,
        super::Pending::Muc(Pending::InstantRoom(room.clone(), replies)),
    );
}

fn affiliation_str(a: &Affiliation) -> &'static str {
    match a {
        Affiliation::Owner => "owner",
        Affiliation::Admin => "admin",
        Affiliation::Member => "member",
        Affiliation::Outcast => "outcast",
        Affiliation::None => "none",
    }
}

fn role_str(r: &Role) -> &'static str {
    match r {
        Role::Moderator => "moderator",
        Role::Participant => "participant",
        Role::Visitor => "visitor",
        Role::None => "none",
    }
}

fn show_str(s: &Show) -> &'static str {
    match s {
        Show::Away => "away",
        Show::Chat => "chat",
        Show::Dnd => "dnd",
        Show::Xa => "xa",
    }
}

/// A short text for a stanza error, for a notice or a `ClientError::Server`.
pub(crate) fn error_text(error: &StanzaError) -> String {
    let condition = match &error.defined_condition {
        DefinedCondition::Conflict => "conflict: the nick is in use",
        DefinedCondition::NotAuthorized => "not-authorized: the room needs a password",
        DefinedCondition::RegistrationRequired => {
            "registration-required: only members can join the room"
        }
        DefinedCondition::Forbidden => "forbidden: not allowed, or banned",
        DefinedCondition::ItemNotFound => "item-not-found",
        DefinedCondition::ServiceUnavailable => "service-unavailable",
        DefinedCondition::NotAllowed => "not-allowed",
        DefinedCondition::PolicyViolation => "policy-violation",
        DefinedCondition::RemoteServerNotFound => "remote-server-not-found",
        DefinedCondition::JidMalformed => "jid-malformed",
        DefinedCondition::NotAcceptable => "not-acceptable",
        _ => "error",
    };
    match error.texts.values().next() {
        Some(text) if !text.is_empty() => format!("{condition} ({text})"),
        _ => condition.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::FeatureState;
    use crate::features::testing::Harness;
    use crate::store::queries::messages_with;
    use xmpp_parsers::message::Message;
    use xmpp_parsers::muc::user::Item;
    use xmpp_parsers::stanza::Stanza;
    use xmpp_parsers::stanza_error::ErrorType;
    use xmpp_parsers::stanza_id::StanzaId;

    const ROOM: &str = "dev@rooms.chord.localhost";
    const ACCOUNT: &str = crate::features::testing::ACCOUNT;

    fn room() -> BareJid {
        BareJid::new(ROOM).unwrap()
    }

    fn jid(s: &str) -> Jid {
        Jid::new(s).unwrap()
    }

    type Answer = oneshot::Receiver<Result<(), ClientError>>;

    fn join(h: &mut Harness, nick: &str) -> Answer {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Join {
                    room: room(),
                    nick: nick.into(),
                    password: None,
                    reply,
                },
            )
        });
        answer
    }

    fn occupant_presence(nick: &str, codes: Vec<Status>, item: Item) -> Presence {
        Presence::available()
            .with_from(jid(&format!("{ROOM}/{nick}")))
            .with_payload(MucUser::new().with_statuses(codes).with_items(vec![item]))
    }

    fn self_presence(nick: &str) -> Presence {
        occupant_presence(
            nick,
            vec![Status::SelfPresence],
            Item::new(Affiliation::Member, Role::Participant),
        )
    }

    fn groupchat(nick: &str, body: &str, stanza_id: Option<&str>, origin: Option<&str>) -> Message {
        let mut m = Message::groupchat(jid("alice@chord.localhost/chord"))
            .with_body("".into(), body.into());
        m.from = Some(jid(&format!("{ROOM}/{nick}")));
        if let Some(id) = stanza_id {
            m = m.with_payload(StanzaId {
                id: id.into(),
                by: jid(ROOM),
            });
        }
        if let Some(id) = origin {
            m = m.with_payload(OriginId { id: id.into() });
        }
        m
    }

    fn occupant_nicks(h: &Harness) -> Vec<String> {
        let mut stmt = h
            .store
            .conn()
            .prepare("SELECT nick FROM occupants WHERE room = ?1 ORDER BY nick")
            .unwrap();
        stmt.query_map([ROOM], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn joined_flag(h: &Harness) -> bool {
        h.store
            .conn()
            .query_row("SELECT joined FROM rooms WHERE jid = ?1", [ROOM], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            != 0
    }

    /// Join the room and complete the join.
    fn joined(h: &mut Harness, nick: &str) {
        let mut answer = join(h, nick);
        h.with_ctx(|ctx| on_presence(ctx, &self_presence(nick)));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        h.take_sent();
        h.take_dirty();
    }

    #[test]
    fn join_sends_presence_with_no_history_and_completes_on_self_presence() {
        let mut h = Harness::new();
        let mut answer = join(&mut h, "alice");
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/alice"))));
        let muc = p
            .payloads
            .iter()
            .find_map(|e| Muc::try_from(e.clone()).ok())
            .unwrap();
        assert_eq!(muc.history.unwrap().maxstanzas, Some(0));
        assert_eq!(answer.try_recv().unwrap(), None, "no answer yet");

        // Another occupant comes first, then our own presence.
        let bob = occupant_presence(
            "bob",
            vec![],
            Item::new(Affiliation::Owner, Role::Moderator)
                .with_jid(jid("bob@chord.localhost/phone").try_into_full().unwrap()),
        );
        h.with_ctx(|ctx| assert!(on_presence(ctx, &bob)));
        assert!(!joined_flag(&h));
        assert_eq!(answer.try_recv().unwrap(), None);
        h.with_ctx(|ctx| assert!(on_presence(ctx, &self_presence("alice"))));

        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(joined_flag(&h));
        assert_eq!(occupant_nicks(&h), ["alice", "bob"]);
        let (real, aff, role): (Option<String>, String, String) = h
            .store
            .conn()
            .query_row(
                "SELECT real_jid, affiliation, role FROM occupants WHERE nick = 'bob'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(real.as_deref(), Some("bob@chord.localhost"));
        assert_eq!((aff.as_str(), role.as_str()), ("owner", "moderator"));
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::MemberList(room())));
        assert!(dirty.contains(&ViewKey::ChannelList(ChannelScope::Home)));
    }

    #[test]
    fn join_calls_are_not_repeated_for_a_room_that_we_are_in() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let mut answer = join(&mut h, "alice");
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn join_conflict_answers_with_the_error() {
        let mut h = Harness::new();
        let mut answer = join(&mut h, "alice");
        let error = Presence::error()
            .with_from(jid(&format!("{ROOM}/alice")))
            .with_payload(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::Conflict,
                "en",
                "",
            ));
        h.with_ctx(|ctx| assert!(on_presence(ctx, &error)));
        let Some(Err(ClientError::Server(text))) = answer.try_recv().unwrap() else {
            panic!("expected a server error")
        };
        assert!(text.starts_with("conflict"), "{text}");
        assert!(!joined_flag(&h));
        assert!(h.state.muc.joins.is_empty());
        assert!(
            h.effects
                .iter()
                .any(|e| matches!(e, super::super::Effect::Emit(ClientEvent::Notice(n)) if n.contains("Cannot join")))
        );
    }

    #[test]
    fn join_errors_for_password_membership_and_ban() {
        for (condition, expect) in [
            (DefinedCondition::NotAuthorized, "not-authorized"),
            (
                DefinedCondition::RegistrationRequired,
                "registration-required",
            ),
            (DefinedCondition::Forbidden, "forbidden"),
        ] {
            let mut h = Harness::new();
            let mut answer = join(&mut h, "alice");
            // An error from the bare room JID counts too.
            let error = Presence::error()
                .with_from(jid(ROOM))
                .with_payload(StanzaError::new(ErrorType::Auth, condition, "en", "no"));
            h.with_ctx(|ctx| on_presence(ctx, &error));
            let Some(Err(ClientError::Server(text))) = answer.try_recv().unwrap() else {
                panic!("expected a server error")
            };
            assert!(text.starts_with(expect), "{text}");
        }
    }

    #[test]
    fn join_is_lost_at_a_new_session() {
        let mut h = Harness::new();
        let mut answer = join(&mut h, "alice");
        h.with_ctx(|ctx| {
            let next = next_session(ctx);
            *ctx.state = FeatureState::default();
            ctx.state.muc = next;
        });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn offline_join_fails() {
        let (reply, mut answer) = oneshot::channel();
        offline(Command::Join {
            room: room(),
            nick: "a".into(),
            password: None,
            reply,
        });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn a_new_room_is_configured_persistent_with_an_archive() {
        // Join, and the room is new: the join waits for the configuration answer.
        let created_presence = || {
            occupant_presence(
                "alice",
                vec![Status::SelfPresence, Status::RoomHasBeenCreated],
                Item::new(Affiliation::Owner, Role::Moderator),
            )
        };
        let configure = |h: &mut Harness| {
            // The join also starts the MAM catch-up of the room. Find the configuration.
            let iqs = h.sent_iqs();
            let Some(Iq::Set { to, payload, .. }) = iqs.iter().find(
                |iq| matches!(iq, Iq::Set { payload, .. } if payload.is("query", NS_MUC_OWNER)),
            ) else {
                panic!("{iqs:?}")
            };
            assert_eq!(to.as_ref(), Some(&jid(ROOM)));
            let form = String::from(payload);
            assert!(form.contains("muc#roomconfig_persistentroom"), "{form}");
            assert!(form.contains("muc#roomconfig_enablearchiving"), "{form}");
        };

        // Success: the join completes after the configuration answer.
        let mut h = Harness::new();
        let mut answer = join(&mut h, "alice");
        h.with_ctx(|ctx| on_presence(ctx, &created_presence()));
        assert_eq!(answer.try_recv().unwrap(), None, "the room is still locked");
        configure(&mut h);
        h.answer(|p| matches!(p, super::super::Pending::Muc(_)), None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));

        // Error: the join fails, with a notice.
        let mut h = Harness::new();
        let mut answer = join(&mut h, "alice");
        h.with_ctx(|ctx| on_presence(ctx, &created_presence()));
        configure(&mut h);
        h.respond(
            |p| matches!(p, super::super::Pending::Muc(_)),
            IqResponse::Error(StanzaError::new(
                ErrorType::Auth,
                DefinedCondition::Forbidden,
                "en",
                "",
            )),
        );
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
        assert!(
            h.effects
                .iter()
                .any(|e| matches!(e, super::super::Effect::Emit(ClientEvent::Notice(n)) if n.contains("unlock")))
        );
    }

    #[test]
    fn occupants_join_change_status_and_leave() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let carol = occupant_presence("carol", vec![], Item::new(Affiliation::None, Role::Visitor))
            .with_show(Show::Away);
        h.with_ctx(|ctx| on_presence(ctx, &carol));
        assert_eq!(occupant_nicks(&h), ["alice", "carol"]);
        let show: Option<String> = h
            .store
            .conn()
            .query_row("SELECT show FROM occupants WHERE nick = 'carol'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(show.as_deref(), Some("away"));

        // Carol changes her nick to karol (303), then the new presence follows.
        let rename = Presence::unavailable()
            .with_from(jid(&format!("{ROOM}/carol")))
            .with_payload(
                MucUser::new()
                    .with_statuses(vec![Status::NewNick])
                    .with_items(vec![
                        Item::new(Affiliation::None, Role::Visitor).with_nick("karol"),
                    ]),
            );
        h.with_ctx(|ctx| on_presence(ctx, &rename));
        assert_eq!(occupant_nicks(&h), ["alice"]);
        let karol = occupant_presence("karol", vec![], Item::new(Affiliation::None, Role::Visitor));
        h.with_ctx(|ctx| on_presence(ctx, &karol));
        assert_eq!(occupant_nicks(&h), ["alice", "karol"]);

        let gone = Presence::unavailable().with_from(jid(&format!("{ROOM}/karol")));
        h.with_ctx(|ctx| on_presence(ctx, &gone));
        assert_eq!(occupant_nicks(&h), ["alice"]);
        assert!(joined_flag(&h));
    }

    #[test]
    fn our_own_nick_change_updates_the_nick() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let rename = Presence::unavailable()
            .with_from(jid(&format!("{ROOM}/alice")))
            .with_payload(
                MucUser::new()
                    .with_statuses(vec![Status::NewNick, Status::SelfPresence])
                    .with_items(vec![
                        Item::new(Affiliation::Member, Role::Participant).with_nick("ally"),
                    ]),
            );
        h.with_ctx(|ctx| on_presence(ctx, &rename));
        assert!(joined_flag(&h), "a nick change is not a leave");
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("ally")));
        assert_eq!(occupant_nicks(&h), ["ally"]);
        assert_eq!(h.state.muc.nicks.get(&room()).unwrap(), "ally");
    }

    #[test]
    fn kicked_and_banned_mark_the_room_not_joined_and_notify() {
        for (code, word) in [("307", "kicked"), ("301", "banned")] {
            let mut h = Harness::new();
            joined(&mut h, "alice");
            let xml = format!(
                "<presence xmlns='jabber:client' from='{ROOM}/alice' type='unavailable'>\
                 <x xmlns='http://jabber.org/protocol/muc#user'>\
                 <item affiliation='none' role='none'><reason>spam</reason></item>\
                 <status code='{code}'/><status code='110'/></x></presence>"
            );
            let removed = Presence::try_from(xml.parse::<Element>().unwrap()).unwrap();
            h.with_ctx(|ctx| on_presence(ctx, &removed));
            assert!(!joined_flag(&h));
            assert!(occupant_nicks(&h).is_empty());
            assert!(!h.state.muc.nicks.contains_key(&room()));
            assert!(h.effects.iter().any(|e| matches!(e,
                super::super::Effect::Emit(ClientEvent::Notice(n)) if n.contains(word) && n.contains("spam"))));
        }
    }

    #[test]
    fn leave_sends_unavailable_and_clears_the_room() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Leave {
                    room: room(),
                    reply,
                },
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.type_, PresenceType::Unavailable);
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/alice"))));
        assert!(!joined_flag(&h));
        assert!(occupant_nicks(&h).is_empty());
        // The echo of the leave changes nothing and raises no notice.
        let echo = Presence::unavailable()
            .with_from(jid(&format!("{ROOM}/alice")))
            .with_payload(MucUser::new().with_statuses(vec![Status::SelfPresence]));
        h.with_ctx(|ctx| assert!(on_presence(ctx, &echo)));
        assert!(
            !h.effects
                .iter()
                .any(|e| matches!(e, super::super::Effect::Emit(_)))
        );
    }

    #[test]
    fn groupchat_message_is_stored_with_the_room_stanza_id() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let mut m = groupchat("bob", "hello room", Some("room-1"), None);
        // A stanza-id from another entity does not count.
        m = m.with_payload(StanzaId {
            id: "forged".into(),
            by: jid("evil@example.org"),
        });
        h.with_ctx(|ctx| assert!(on_message(ctx, &m)));
        let stored = messages_with(h.store.conn(), h.account_id, ROOM).unwrap();
        assert_eq!(stored.len(), 1);
        let s = &stored[0];
        assert_eq!(s.kind, MessageKind::Groupchat);
        assert_eq!((s.key_kind, s.key.as_str()), (KeyKind::StanzaId, "room-1"));
        assert_eq!(s.direction, Direction::In);
        assert_eq!(s.sender, format!("{ROOM}/bob"));
        assert_eq!(s.body, "hello room");
        assert!(h.take_dirty().contains(&ViewKey::Timeline(room())));
        assert!(h.effects.iter().any(|e| matches!(
            e,
            super::super::Effect::Emit(ClientEvent::MessageReceived(_))
        )));

        // The same message again adds no row.
        h.with_ctx(|ctx| on_message(ctx, &m));
        assert_eq!(
            messages_with(h.store.conn(), h.account_id, ROOM)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn reflected_own_message_upgrades_the_origin_id_row() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let id = h
            .with_ctx(|ctx| send(ctx, &room(), "my words".into()))
            .unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.type_, MessageType::Groupchat);
        assert_eq!(m.to, Some(jid(ROOM)));
        assert_eq!(m.id, Some(Id(id.clone())));
        let before = messages_with(h.store.conn(), h.account_id, ROOM).unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(
            (before[0].key_kind, before[0].direction),
            (KeyKind::OriginId, Direction::Out)
        );
        assert_eq!(before[0].sender, format!("{ROOM}/alice"));

        let reflected = groupchat("alice", "my words", Some("room-7"), Some(&id));
        h.with_ctx(|ctx| assert!(on_message(ctx, &reflected)));
        let after = messages_with(h.store.conn(), h.account_id, ROOM).unwrap();
        assert_eq!(after.len(), 1, "no second row");
        assert_eq!(
            (after[0].key_kind, after[0].key.as_str()),
            (KeyKind::StanzaId, "room-7")
        );
        assert_eq!(after[0].timestamp, before[0].timestamp);
    }

    #[test]
    fn send_to_a_known_room_waits_for_the_join_and_send_chat_routes_by_room() {
        let mut h = Harness::new();
        ensure_room_for_test(&mut h);
        // Not in the room yet: the send starts a join and queues the message.
        let id = h
            .with_ctx(|ctx| send_chat(ctx, Jid::from(room()), "early".into()))
            .unwrap();
        let sent = h.take_sent();
        assert!(
            matches!(&sent[..], [Stanza::Presence(_)]),
            "only the join presence goes out: {sent:?}"
        );
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("alice")));
        let sent = h.take_sent();
        let queued = sent.iter().find_map(|s| match s {
            Stanza::Message(m) if m.type_ == MessageType::Groupchat => Some(m),
            _ => None,
        });
        assert_eq!(queued.and_then(|m| m.id.clone()), Some(Id(id)), "{sent:?}");

        // A room that we do not know is an error.
        let err = h
            .with_ctx(|ctx| {
                send(
                    ctx,
                    &BareJid::new("nope@rooms.chord.localhost").unwrap(),
                    "x".into(),
                )
            })
            .unwrap_err();
        assert!(matches!(err, ClientError::Invalid(_)), "{err:?}");
        // A full JID of the room is a private message. Bob is not an occupant.
        let err = h
            .with_ctx(|ctx| send_chat(ctx, jid(&format!("{ROOM}/bob")), "x".into()))
            .unwrap_err();
        assert!(matches!(err, ClientError::Invalid(_)), "{err:?}");
        // A contact is a normal chat.
        h.with_ctx(|ctx| send_chat(ctx, jid("bob@chord.localhost"), "hi".into()))
            .unwrap();
        let sent = h.take_sent();
        assert!(matches!(&sent[..], [Stanza::Message(m)] if m.type_ == MessageType::Chat));
    }

    fn ensure_room_for_test(h: &mut Harness) {
        h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    }

    #[test]
    fn subject_goes_to_the_room_row() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let mut m = Message::groupchat(jid("alice@chord.localhost/chord"));
        m.from = Some(jid(&format!("{ROOM}/bob")));
        m.subjects.insert(Default::default(), "New topic".into());
        h.with_ctx(|ctx| assert!(on_message(ctx, &m)));
        let subject: Option<String> = h
            .store
            .conn()
            .query_row("SELECT subject FROM rooms WHERE jid = ?1", [ROOM], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(subject.as_deref(), Some("New topic"));
        assert!(
            messages_with(h.store.conn(), h.account_id, ROOM)
                .unwrap()
                .is_empty()
        );
        // An empty subject clears it.
        m.subjects.insert(Default::default(), String::new());
        h.with_ctx(|ctx| on_message(ctx, &m));
        let subject: Option<String> = h
            .store
            .conn()
            .query_row("SELECT subject FROM rooms WHERE jid = ?1", [ROOM], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(subject, None);
    }

    #[test]
    fn unknown_rooms_are_not_stored() {
        let mut h = Harness::new();
        let mut other = groupchat("bob", "hi", Some("s-1"), None);
        other.from = Some(jid("other@rooms.chord.localhost/bob"));
        h.with_ctx(|ctx| assert!(!on_message(ctx, &other)));

        joined(&mut h, "alice");
        // A status text from the room itself is not stored.
        let mut status = groupchat("bob", "The room is public", Some("s-3"), None);
        status.from = Some(jid(ROOM));
        h.with_ctx(|ctx| on_message(ctx, &status));
        assert!(
            messages_with(h.store.conn(), h.account_id, ROOM)
                .unwrap()
                .is_empty()
        );
    }

    /// A private message from an occupant. Our server stamps the stanza-id.
    fn private(nick: &str, body: &str, stanza_id: Option<&str>) -> Message {
        let mut m =
            Message::chat(jid("alice@chord.localhost/chord")).with_body("".into(), body.into());
        m.from = Some(jid(&format!("{ROOM}/{nick}")));
        m.payloads.push(Element::builder("x", NS_MUC_USER).build());
        if let Some(id) = stanza_id {
            m = m.with_payload(StanzaId {
                id: id.into(),
                by: jid(ACCOUNT),
            });
        }
        m
    }

    fn peer_rows(h: &Harness, peer: &str) -> Vec<queries::StoredMessage> {
        messages_with(h.store.conn(), h.account_id, peer).unwrap()
    }

    #[test]
    fn incoming_private_message_is_a_chat_row_of_the_occupant() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let m = private("bob", "psst", Some("pm-1"));
        h.with_ctx(|ctx| assert!(on_message(ctx, &m)));
        let peer = format!("{ROOM}/bob");
        let rows = peer_rows(&h, &peer);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, MessageKind::Chat);
        assert_eq!(rows[0].direction, Direction::In);
        assert_eq!(rows[0].sender, peer);
        assert!(peer_rows(&h, ROOM).is_empty(), "not in the room timeline");
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::PrivateTimeline(room(), "bob".into())));
        assert!(!dirty.contains(&ViewKey::Timeline(room())));
        assert!(h.effects.iter().any(|e| matches!(
            e,
            super::super::Effect::Emit(ClientEvent::MessageReceived(_))
        )));
        // Without the muc#user element, a chat from a full JID of the room is private too.
        let mut plain = private("bob", "again", Some("pm-2"));
        plain.payloads.retain(|p| !p.is("x", NS_MUC_USER));
        h.with_ctx(|ctx| on_message(ctx, &plain));
        assert_eq!(peer_rows(&h, &peer).len(), 2);
    }

    #[test]
    fn private_message_carbons_and_archive_go_to_the_occupant_row() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        // A sent carbon: from our account, to the occupant.
        let mut sent = Message::chat(jid(&format!("{ROOM}/bob")))
            .with_body("".into(), "from my phone".into())
            .with_payload(OriginId { id: "o-1".into() });
        sent.payloads
            .push(Element::builder("x", NS_MUC_USER).build());
        sent.from = Some(jid("alice@chord.localhost/phone"));
        h.with_ctx(|ctx| crate::features::chat::on_message(ctx, &sent));
        let rows = peer_rows(&h, &format!("{ROOM}/bob"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].direction, Direction::Out);
        assert_eq!(rows[0].sender, format!("{ROOM}/alice"));
        assert!(peer_rows(&h, ROOM).is_empty());
        // The archive returns a message from an occupant.
        let archived = private("bob", "old", None);
        h.with_ctx(|ctx| {
            crate::features::chat::store_archived(ctx, &archived, "arch-1", Some(1000))
        });
        assert_eq!(peer_rows(&h, &format!("{ROOM}/bob")).len(), 2);
        assert!(peer_rows(&h, ROOM).is_empty());
    }

    #[test]
    fn send_private_sends_a_chat_with_muc_user_and_stores_it() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        // Bob is not an occupant yet.
        let err = h
            .with_ctx(|ctx| send_private(ctx, &room(), "bob", "hi".into()))
            .unwrap_err();
        assert!(matches!(err, ClientError::Invalid(_)), "{err:?}");
        h.with_ctx(|ctx| {
            on_presence(
                ctx,
                &occupant_presence(
                    "bob",
                    vec![],
                    Item::new(Affiliation::Member, Role::Participant),
                ),
            )
        });
        h.take_dirty();
        let id = h
            .with_ctx(|ctx| send_private(ctx, &room(), "bob", "hi bob".into()))
            .unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = &sent[..] else {
            panic!("{sent:?}");
        };
        assert_eq!(m.type_, MessageType::Chat);
        assert_eq!(m.to, Some(jid(&format!("{ROOM}/bob"))));
        assert_eq!(m.id, Some(Id(id.clone())));
        assert!(m.payloads.iter().any(|p| p.is("x", NS_MUC_USER)));
        let rows = peer_rows(&h, &format!("{ROOM}/bob"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].direction, Direction::Out);
        assert_eq!(rows[0].key, id);
        assert!(peer_rows(&h, ROOM).is_empty());
        assert!(
            h.take_dirty()
                .contains(&ViewKey::PrivateTimeline(room(), "bob".into()))
        );
        // A room that we left is an error.
        let other = BareJid::new("other@rooms.chord.localhost").unwrap();
        let err = h
            .with_ctx(|ctx| send_private(ctx, &other, "bob", "x".into()))
            .unwrap_err();
        assert!(matches!(err, ClientError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn archived_messages_use_the_archive_id() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let old = groupchat("bob", "long ago", None, None);
        h.with_ctx(|ctx| store_archived(ctx, &room(), &old, "arch-1", Some(1_700_000_000_000)));
        // Our own old message has an origin-id row: the archive gives it its key.
        let id = h.with_ctx(|ctx| send(ctx, &room(), "mine".into())).unwrap();
        let mine = groupchat("alice", "mine", None, Some(&id));
        h.with_ctx(|ctx| store_archived(ctx, &room(), &mine, "arch-2", None));
        h.with_ctx(|ctx| store_archived(ctx, &room(), &old, "arch-1", None));

        let stored = messages_with(h.store.conn(), h.account_id, ROOM).unwrap();
        assert_eq!(stored.len(), 2, "{stored:?}");
        assert_eq!(stored[0].key, "arch-1");
        assert_eq!(stored[0].timestamp, 1_700_000_000_000);
        assert_eq!(stored[0].direction, Direction::In);
        assert_eq!(
            (
                stored[1].key_kind,
                stored[1].key.as_str(),
                stored[1].direction
            ),
            (KeyKind::StanzaId, "arch-2", Direction::Out)
        );
    }

    #[test]
    fn rooms_of_the_last_session_are_joined_again() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        // A room that was left is not joined again.
        let other = BareJid::new("old@rooms.chord.localhost").unwrap();
        h.with_ctx(|ctx| ensure_room(ctx, &other, Some("alice"), None));
        h.with_ctx(|ctx| {
            let next = next_session(ctx);
            *ctx.state = FeatureState::default();
            ctx.state.muc = next;
            crate::store::queries::clear_volatile(ctx.store, ctx.account_id).unwrap();
            on_connected(ctx);
        });
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/alice"))));
    }

    #[test]
    fn stale_presence_after_leaving_is_ignored() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Leave {
                    room: room(),
                    reply,
                },
            )
        });
        let late = occupant_presence(
            "bob",
            vec![],
            Item::new(Affiliation::None, Role::Participant),
        );
        h.with_ctx(|ctx| on_presence(ctx, &late));
        assert!(occupant_nicks(&h).is_empty());
    }
}
