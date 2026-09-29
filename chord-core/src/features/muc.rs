//! Multi-user chat (XEP-0045): join, leave, occupants, and groupchat messages.
//!
//! Private messages between occupants (section 7.5) are chat rows with the peer
//! `room@service/nick`. Their timeline is `ViewKey::PrivateTimeline`.

use std::collections::{HashMap, HashSet};

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::data_forms::DataForm;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::muc::Muc;
use xmpp_parsers::muc::muc::History;
use xmpp_parsers::muc::user::{Affiliation, MucUser, Role, Status};
use xmpp_parsers::presence::{Presence, Show, Type as PresenceType};
use xmpp_parsers::stanza_error::{DefinedCondition, StanzaError};
use xmpp_parsers::stanza_id::OriginId;

use super::chat::{MessageIds, delay_ms};
use super::message_ext::{self, Incoming, Outgoing};
use super::{Ctx, IqResponse, avatars, bookmarks, mam, new_id, presence as own_presence};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::store::queries::{self, Direction, KeyKind, MessageExtras, MessageKind, NewMessage};
use crate::views::{ChannelScope, ViewKey};

const NS_MUC_OWNER: &str = "http://jabber.org/protocol/muc#owner";
pub(crate) const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";
/// XEP-0421 occupant identifiers.
const NS_OCCUPANT_ID: &str = "urn:xmpp:occupant-id:0";
const NS_MUC_ADMIN: &str = "http://jabber.org/protocol/muc#admin";
/// XEP-0249 direct invitations.
const NS_CONFERENCE: &str = "jabber:x:conference";
const NS_DATA: &str = "jabber:x:data";

type Reply = oneshot::Sender<Result<(), ClientError>>;
type PrivateReply = oneshot::Sender<Result<String, ClientError>>;
fn nc(name: &str) -> NcName {
    NcName::try_from(name.to_owned()).expect("a valid attribute name")
}

type MembersReply = oneshot::Sender<Result<Vec<(BareJid, Option<String>)>, ClientError>>;

/// The affiliation of a JID with a room (XEP-0045, section 5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum RoomAffiliation {
    Owner,
    Admin,
    Member,
    None,
    Outcast,
}

impl RoomAffiliation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
            Self::None => "none",
            Self::Outcast => "outcast",
        }
    }
}

/// Room settings that `configure_room` changes. A `None` field stays as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct RoomSettings {
    /// The name of the room (`muc#roomconfig_roomname`).
    pub name: Option<String>,
    /// True lists the room in the room directory (`muc#roomconfig_publicroom`).
    pub public: Option<bool>,
    /// True lets only members enter (`muc#roomconfig_membersonly`).
    pub members_only: Option<bool>,
}

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
    /// Every nick that we used in a room during this session, the current one included.
    /// A message from one of them is ours.
    used_nicks: HashMap<BareJid, HashSet<String>>,
    /// Our XEP-0421 occupant-id in a room, from our own presence. It survives nick changes.
    occupant_ids: HashMap<BareJid, String>,
    /// The rooms that were joined before this session. `on_connected` joins them again.
    previous: Vec<BareJid>,
    /// Messages for rooms that we are joining. They go out when the join completes.
    outbox: HashMap<BareJid, Vec<Message>>,
    /// IQs that need our presence in a room (for example a XEP-0425 moderation), with the
    /// feature that gets the answer. They go out when the join completes.
    iq_outbox: HashMap<BareJid, Vec<(Iq, super::Pending)>>,
    /// A nick change that arrived while the join ran. It runs when the join completes.
    pending_nick: HashMap<BareJid, (String, Reply)>,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The answer to the configuration of a new room, with the join replies that wait
    /// for it: others cannot enter the room until it is unlocked.
    /// The configuration form of a room that we created. The joins wait.
    RoomConfigForm(BareJid, Vec<Reply>),
    InstantRoom(BareJid, Vec<Reply>),
    /// The answer to a XEP-0425 moderation request.
    Moderate(Reply),
    /// The answer to an IQ that needs only a success or an error.
    Simple(Reply),
    /// The list of JIDs with one affiliation.
    Affiliations(MembersReply),
    /// The membership grant of an invitation. The invitation goes out after the answer.
    InviteGrant {
        room: BareJid,
        jid: BareJid,
        reason: Option<String>,
        reply: Reply,
    },
    /// The configuration form of a room, for `configure_room`.
    SettingsForm(BareJid, RoomSettings, Reply),
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
    ChangeNick {
        room: BareJid,
        nick: String,
        reply: Reply,
    },
    SetAffiliation {
        room: BareJid,
        jid: BareJid,
        affiliation: RoomAffiliation,
        reason: Option<String>,
        reply: Reply,
    },
    ListAffiliations {
        room: BareJid,
        affiliation: RoomAffiliation,
        reply: MembersReply,
    },
    Invite {
        room: BareJid,
        jid: BareJid,
        reason: Option<String>,
        reply: Reply,
    },
    DeclineInvite {
        room: BareJid,
        from: BareJid,
        reason: Option<String>,
        reply: Reply,
    },
    Configure {
        room: BareJid,
        settings: RoomSettings,
        reply: Reply,
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

    /// Change our nick in a room that we are in (XEP-0045, section 7.6) and wait for the
    /// answer of the room. Fails with `ClientError::Invalid` when we are not in the room,
    /// and with `ClientError::Server` when the room refuses: nick in use (`conflict`) or
    /// nick change not allowed (`not-acceptable`). The stored nick changes only on success.
    pub async fn change_nick(&self, room: BareJid, new_nick: String) -> Result<(), ClientError> {
        self.room_command(|reply| Command::ChangeNick {
            room,
            nick: new_nick,
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

    /// Set the affiliation of a JID with a room (XEP-0045, sections 9.3, 9.5 and 10.3).
    /// We need the right to do it: an admin sets member and outcast, an owner sets all.
    /// Fails with `ClientError::Server` when the room refuses (`forbidden`, `not-allowed`).
    pub async fn set_room_affiliation(
        &self,
        room: BareJid,
        jid: BareJid,
        affiliation: RoomAffiliation,
        reason: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::SetAffiliation {
            room,
            jid,
            affiliation,
            reason,
            reply,
        })
        .await
    }

    /// List the JIDs with one affiliation in a room, with their nick when the room has
    /// one (XEP-0045, section 9.4).
    pub async fn room_affiliations(
        &self,
        room: BareJid,
        affiliation: RoomAffiliation,
    ) -> Result<Vec<(BareJid, Option<String>)>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Muc(Command::ListAffiliations {
            room,
            affiliation,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Invite a JID to a room with a mediated invitation (XEP-0045, section 7.8.2).
    /// When we are owner or admin of the room, Chord makes the JID a member first, so that
    /// it can enter a members-only room. Chord does not know if the room is members-only,
    /// so it grants membership in every room where we have the right. Without the right,
    /// Chord only sends the invitation.
    pub async fn invite_to_room(
        &self,
        room: BareJid,
        jid: BareJid,
        reason: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::Invite {
            room,
            jid,
            reason,
            reply,
        })
        .await
    }

    /// Decline a mediated invitation that a `ClientEvent::RoomInvite` reported.
    pub async fn decline_room_invite(
        &self,
        room: BareJid,
        from: BareJid,
        reason: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::DeclineInvite {
            room,
            from,
            reason,
            reply,
        })
        .await
    }

    /// Change settings of a room that we own. Chord fetches the configuration form and
    /// submits only the fields that the form has and that `settings` sets.
    pub async fn configure_room(
        &self,
        room: BareJid,
        settings: RoomSettings,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::Configure {
            room,
            settings,
            reply,
        })
        .await
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
        Pending::RoomConfigForm(room, replies) => {
            let form = match response {
                IqResponse::Result(Some(query)) => query
                    .get_child("x", "jabber:x:data")
                    .and_then(|x| DataForm::try_from(x.clone()).ok()),
                IqResponse::Result(None) | IqResponse::Error(_) => None,
                IqResponse::Lost => {
                    for reply in replies {
                        let _ = reply.send(Err(ClientError::NotConnected));
                    }
                    return;
                }
            };
            submit_room_config(ctx, &room, form.as_ref(), replies);
        }
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
        Pending::Moderate(reply) | Pending::Simple(reply) => {
            let _ = reply.send(match response {
                IqResponse::Result(_) => Ok(()),
                IqResponse::Error(e) => Err(ClientError::Server(error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            });
        }
        Pending::Affiliations(reply) => {
            let _ = reply.send(match response {
                IqResponse::Result(query) => Ok(query
                    .map(|q| {
                        q.children()
                            .filter(|c| c.is("item", NS_MUC_ADMIN))
                            .filter_map(|item| {
                                let jid = BareJid::new(item.attr("jid")?).ok()?;
                                Some((jid, item.attr("nick").map(str::to_owned)))
                            })
                            .collect()
                    })
                    .unwrap_or_default()),
                IqResponse::Error(e) => Err(ClientError::Server(error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            });
        }
        Pending::InviteGrant {
            room,
            jid,
            reason,
            reply,
        } => {
            match response {
                IqResponse::Result(_) => {}
                IqResponse::Error(e) => {
                    // The invitation still helps in an open room.
                    ctx.emit(ClientEvent::Notice(format!(
                        "Cannot make {jid} a member of {room}: {}",
                        error_text(&e)
                    )));
                }
                IqResponse::Lost => {
                    let _ = reply.send(Err(ClientError::NotConnected));
                    return;
                }
            }
            send_invitation(ctx, &room, &jid, reason);
            let _ = reply.send(Ok(()));
        }
        Pending::SettingsForm(room, settings, reply) => {
            let form = match response {
                IqResponse::Result(Some(query)) => query
                    .get_child("x", NS_DATA)
                    .and_then(|x| DataForm::try_from(x.clone()).ok()),
                IqResponse::Result(None) => None,
                IqResponse::Error(e) => {
                    let _ = reply.send(Err(ClientError::Server(error_text(&e))));
                    return;
                }
                IqResponse::Lost => {
                    let _ = reply.send(Err(ClientError::NotConnected));
                    return;
                }
            };
            let Some(form) = form else {
                let _ = reply.send(Err(ClientError::Server(
                    "the room has no configuration form".to_owned(),
                )));
                return;
            };
            let payload = settings_submit(&form, &settings);
            let iq = Iq::Set {
                from: None,
                to: Some(Jid::from(room)),
                id: String::new(),
                payload,
            };
            ctx.request(iq, super::Pending::Muc(Pending::Simple(reply)));
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
        Command::ChangeNick { room, nick, reply } => {
            if ctx.state.muc.nicks.contains_key(&room) {
                join_room(ctx, &room, Some(nick), None, Some(reply));
            } else if ctx.state.muc.joins.contains_key(&room) {
                // The join runs (for example the autojoin after login): change after it.
                if let Some((_, old)) = ctx.state.muc.pending_nick.insert(room, (nick, reply)) {
                    let _ = old.send(Err(ClientError::Invalid("a newer nick change".into())));
                }
            } else if room_row(ctx, &room).is_some() {
                // A known room that we are not in: join it with the new nick.
                join_room(ctx, &room, Some(nick), None, Some(reply));
            } else {
                let _ = reply.send(Err(ClientError::Invalid(format!("not in the room {room}"))));
            }
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
        Command::SetAffiliation {
            room,
            jid,
            affiliation,
            reason,
            reply,
        } => {
            let iq = affiliation_iq(&room, &jid, affiliation, reason.as_deref());
            ctx.request(iq, super::Pending::Muc(Pending::Simple(reply)));
        }
        Command::ListAffiliations {
            room,
            affiliation,
            reply,
        } => {
            let payload: Element = format!(
                "<query xmlns='{NS_MUC_ADMIN}'><item affiliation='{}'/></query>",
                affiliation.as_str()
            )
            .parse()
            .expect("static XML");
            let iq = Iq::Get {
                from: None,
                to: Some(Jid::from(room)),
                id: String::new(),
                payload,
            };
            ctx.request(iq, super::Pending::Muc(Pending::Affiliations(reply)));
        }
        Command::Invite {
            room,
            jid,
            reason,
            reply,
        } => {
            if can_grant(ctx, &room) {
                let iq = affiliation_iq(&room, &jid, RoomAffiliation::Member, None);
                let pending = Pending::InviteGrant {
                    room,
                    jid,
                    reason,
                    reply,
                };
                ctx.request(iq, super::Pending::Muc(pending));
            } else {
                send_invitation(ctx, &room, &jid, reason);
                let _ = reply.send(Ok(()));
            }
        }
        Command::DeclineInvite {
            room,
            from,
            reason,
            reply,
        } => {
            let mut decline =
                Element::builder("decline", NS_MUC_USER).attr(nc("to"), from.to_string());
            if let Some(reason) = reason.filter(|r| !r.is_empty()) {
                decline = decline.append(Element::builder("reason", NS_MUC_USER).append(reason));
            }
            let x = Element::builder("x", NS_MUC_USER).append(decline).build();
            let mut message = Message::new(Some(Jid::from(room)));
            message.id = Some(Id(new_id()));
            message.payloads.push(x);
            ctx.send(message);
            let _ = reply.send(Ok(()));
        }
        Command::Configure {
            room,
            settings,
            reply,
        } => {
            let iq = Iq::Get {
                from: None,
                to: Some(Jid::from(room.clone())),
                id: String::new(),
                payload: format!("<query xmlns='{NS_MUC_OWNER}'/>")
                    .parse()
                    .expect("static XML"),
            };
            ctx.request(
                iq,
                super::Pending::Muc(Pending::SettingsForm(room, settings, reply)),
            );
        }
    }
}

/// True when we are owner or admin of a room that we are in.
fn can_grant(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    let Some(nick) = ctx.state.muc.nicks.get(room) else {
        return false;
    };
    let affiliation: Option<String> = ctx
        .store
        .conn()
        .query_row(
            "SELECT affiliation FROM occupants WHERE account_id = ?1 AND room = ?2 AND nick = ?3",
            params![ctx.account_id, room.as_str(), nick],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    matches!(affiliation.as_deref(), Some("owner" | "admin"))
}

/// The IQ that sets one affiliation (XEP-0045, sections 9.3, 9.5 and 10.3).
fn affiliation_iq(
    room: &BareJid,
    jid: &BareJid,
    affiliation: RoomAffiliation,
    reason: Option<&str>,
) -> Iq {
    let mut item = Element::builder("item", NS_MUC_ADMIN)
        .attr(nc("affiliation"), affiliation.as_str())
        .attr(nc("jid"), jid.to_string());
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        item = item.append(Element::builder("reason", NS_MUC_ADMIN).append(reason));
    }
    Iq::Set {
        from: None,
        to: Some(Jid::from(room.clone())),
        id: String::new(),
        payload: Element::builder("query", NS_MUC_ADMIN).append(item).build(),
    }
}

/// Send a mediated invitation (XEP-0045, section 7.8.2).
fn send_invitation(ctx: &mut Ctx<'_>, room: &BareJid, jid: &BareJid, reason: Option<String>) {
    let mut invite = Element::builder("invite", NS_MUC_USER).attr(nc("to"), jid.to_string());
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        invite = invite.append(Element::builder("reason", NS_MUC_USER).append(reason));
    }
    let x = Element::builder("x", NS_MUC_USER).append(invite).build();
    let mut message = Message::new(Some(Jid::from(room.clone())));
    message.id = Some(Id(new_id()));
    message.payloads.push(x);
    ctx.send(message);
}

/// The submit form of `configure_room`: only fields that the form has and that the
/// settings set.
fn settings_submit(form: &DataForm, settings: &RoomSettings) -> Element {
    let has = |var: &str| form.fields.iter().any(|f| f.var.as_deref() == Some(var));
    let flag = |value: bool| if value { "1" } else { "0" };
    let field = |var: &str, value: &str| {
        Element::builder("field", NS_DATA)
            .attr(nc("var"), var)
            .append(Element::builder("value", NS_DATA).append(value))
    };
    let mut x = Element::builder("x", NS_DATA)
        .attr(nc("type"), "submit")
        .append(field(
            "FORM_TYPE",
            "http://jabber.org/protocol/muc#roomconfig",
        ));
    if let Some(name) = &settings.name
        && has("muc#roomconfig_roomname")
    {
        x = x.append(field("muc#roomconfig_roomname", name));
    }
    if let Some(public) = settings.public
        && has("muc#roomconfig_publicroom")
    {
        x = x.append(field("muc#roomconfig_publicroom", flag(public)));
    }
    if let Some(members_only) = settings.members_only
        && has("muc#roomconfig_membersonly")
    {
        x = x.append(field("muc#roomconfig_membersonly", flag(members_only)));
    }
    Element::builder("query", NS_MUC_OWNER)
        .append(x.build())
        .build()
}

/// An invitation to a room: mediated (XEP-0045, 7.8.2) or direct (XEP-0249). Emits
/// `ClientEvent::RoomInvite` and returns true. Chord never joins on its own.
fn on_invitation(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(from) = &message.from else {
        return false;
    };
    for x in &message.payloads {
        if x.is("x", NS_MUC_USER)
            && let Some(invite) = x.get_child("invite", NS_MUC_USER)
        {
            let inviter = invite
                .attr("from")
                .and_then(|f| f.parse::<Jid>().ok())
                .unwrap_or_else(|| from.clone());
            let reason = invite
                .get_child("reason", NS_MUC_USER)
                .map(Element::text)
                .filter(|r| !r.is_empty());
            let password = x
                .get_child("password", NS_MUC_USER)
                .map(Element::text)
                .filter(|p| !p.is_empty());
            ctx.emit(ClientEvent::RoomInvite {
                room: from.to_bare(),
                from: inviter,
                reason,
                password,
            });
            return true;
        }
        if x.is("x", NS_CONFERENCE)
            && let Some(room) = x.attr("jid").and_then(|j| j.parse::<Jid>().ok())
        {
            let non_empty = |name: &str| x.attr(name).filter(|v| !v.is_empty()).map(str::to_owned);
            ctx.emit(ClientEvent::RoomInvite {
                room: room.to_bare(),
                from: from.clone(),
                reason: non_empty("reason"),
                password: non_empty("password"),
            });
            return true;
        }
    }
    false
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    if let Command::SendPrivate { reply, .. } = command {
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
    if let Command::ListAffiliations { reply, .. } = command {
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
    let (Command::Join { reply, .. }
    | Command::SetAffiliation { reply, .. }
    | Command::Invite { reply, .. }
    | Command::DeclineInvite { reply, .. }
    | Command::Configure { reply, .. }
    | Command::Leave { reply, .. }
    | Command::ChangeNick { reply, .. }
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
    let mut presence = room_presence(ctx).with_to(target);
    // A nick change is a plain presence to the new nick (XEP-0045, 7.6).
    if current.is_none() {
        let mut muc = Muc::new().with_history(History::new().with_maxstanzas(0));
        muc.password = password;
        presence.payloads.push(muc.into());
    }
    ctx.send(presence);
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

/// Available presence for a room: our caps, and the hash of our avatar when the store
/// has it (XEP-0153), like the presence of the account.
fn room_presence(ctx: &mut Ctx<'_>) -> Presence {
    match avatars::load(ctx.store, ctx.account_id, ctx.account) {
        Ok(Some(avatar)) => avatars::vcard_presence(Some(&avatar.hash)),
        Ok(None) => own_presence::initial(),
        Err(e) => {
            ctx.store_error("read our avatar", e);
            own_presence::initial()
        }
    }
}

/// The XEP-0421 occupant-id in a list of payloads.
fn occupant_id(payloads: &[Element]) -> Option<String> {
    payloads
        .iter()
        .find(|e| e.is("occupant-id", NS_OCCUPANT_ID))
        .and_then(|e| e.attr("id"))
        .map(str::to_owned)
}

fn remember_nick(ctx: &mut Ctx<'_>, room: &BareJid, nick: &str) {
    ctx.state
        .muc
        .used_nicks
        .entry(room.clone())
        .or_default()
        .insert(nick.to_owned());
}

/// Whether a groupchat message from `nick` is ours. The occupant-id decides when the
/// message has one and we know ours. Else our current nick or an earlier one decides.
pub(crate) fn is_ours(ctx: &Ctx<'_>, room: &BareJid, nick: &str, message: &Message) -> bool {
    if let (Some(ours), Some(theirs)) = (
        ctx.state.muc.occupant_ids.get(room),
        occupant_id(&message.payloads),
    ) {
        return *ours == theirs;
    }
    our_nick(ctx, room).as_deref() == Some(nick)
        || ctx
            .state
            .muc
            .used_nicks
            .get(room)
            .is_some_and(|nicks| nicks.contains(nick))
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

/// True if the domain of `jid` is a MUC service of our server (XEP-0045 disco identity
/// `conference/text`).
fn is_muc_service(ctx: &Ctx<'_>, jid: &BareJid) -> bool {
    let domain = jid.domain().as_str();
    ctx.state.disco.services.iter().any(|(service, info)| {
        service.domain().as_str() == domain
            && info
                .identities
                .iter()
                .any(|i| i.category == "conference" && i.type_ == "text")
    })
}

/// Send a message to a room or a contact. A room is a room that the account knows.
pub(crate) fn send_chat(ctx: &mut Ctx<'_>, to: Jid, body: String) -> Result<String, ClientError> {
    let room = to.to_bare();
    if !is_room(ctx, &room) {
        // A room on a MUC service that we have not joined: a chat message would not
        // reach it. Service discovery names the MUC services.
        if to.resource().is_none() && is_muc_service(ctx, &room) {
            return Err(ClientError::Invalid(format!(
                "{room} is a room: join it before you send to it"
            )));
        }
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
    send_private_message(ctx, room, nick, body, Outgoing::default())
}

/// Send a private message with extra payloads and references (for example a reply), and
/// store it. Returns its origin-id. See `send_private`.
pub(crate) fn send_private_message(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    nick: &str,
    body: String,
    out: Outgoing,
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
    message.payloads.extend(message_ext::outgoing_payloads(ctx));
    message.payloads.extend(out.payloads);
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
            ..out.extras
        },
    };
    if let Err(e) = queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        ctx.store_error("store a sent message", e);
    }
    super::chat_states::on_sent(ctx, &peer);
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
/// reactions, retractions, and markers work as in a chat.
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
    let peer = format!("{room}/{nick}");
    let sender = match direction {
        Direction::In => from.to_string(),
        Direction::Out => format!(
            "{room}/{}",
            our_nick(ctx, &room).unwrap_or_else(|| ctx.account.to_string())
        ),
    };
    let incoming = Incoming {
        message,
        kind: MessageKind::Chat,
        direction,
        peer: &peer,
        sender: &sender,
        timestamp,
    };
    // Corrections, retractions, reactions, and markers change earlier messages.
    if message_ext::intercept(ctx, &incoming) {
        return true;
    }
    let Some(body) = message_ext::body(message) else {
        return true;
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
            message_ext::after_store(ctx, &incoming, &stored, live);
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
    super::chat_states::on_sent(ctx, &peer);
    mark_room(ctx, room);
    Ok(origin_id)
}

/// A message from a room. Returns true if this module handled it.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(from) = &message.from else {
        return false;
    };
    if message.type_ != MessageType::Error && on_invitation(ctx, message) {
        return true;
    }
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
    // A message from the room itself, with no nick, is a status text. Skip it. It can also
    // say that a moderator retracted a message (XEP-0425).
    let Some(nick) = from.resource() else {
        let sender_jid = from.to_string();
        let incoming = Incoming {
            message,
            kind: MessageKind::Groupchat,
            direction: Direction::In,
            peer: room.as_str(),
            sender: &sender_jid,
            timestamp,
        };
        super::retraction::on_moderation(ctx, &incoming);
        return;
    };
    let direction = if is_ours(ctx, room, nick.as_str(), message) {
        Direction::Out
    } else {
        Direction::In
    };
    let sender_jid = from.to_string();
    let incoming = Incoming {
        message,
        kind: MessageKind::Groupchat,
        direction,
        peer: room.as_str(),
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
            message_ext::after_store(ctx, &incoming, &stored, live);
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
        if join.changing_nick {
            // The nick change failed. Keep the nick that we have.
            if let Some(old) = ctx.state.muc.nicks.get(room).cloned() {
                ensure_room(ctx, room, Some(&old), None);
            }
        } else {
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
            remember_nick(ctx, room, nick);
            remember_nick(ctx, room, &new_nick);
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
    if is_self {
        remember_nick(ctx, room, nick);
        if let Some(id) = occupant_id(&presence.payloads) {
            ctx.state.muc.occupant_ids.insert(room.clone(), id);
        }
    }
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
        for (iq, then) in ctx.state.muc.iq_outbox.remove(room).unwrap_or_default() {
            ctx.request(iq, then);
        }
        if let Some((nick, reply)) = ctx.state.muc.pending_nick.remove(room) {
            join_room(ctx, room, Some(nick), None, Some(reply));
        }
    }
    mark_room(ctx, room);
}

/// True while messages wait for a room join.
pub(crate) fn has_outbox(state: &State) -> bool {
    state.outbox.values().any(|m| !m.is_empty())
}

/// Drop the queued messages of a room whose join failed, and say so.
/// Whether we are in `room`, or know it well enough to join it again (a stored room).
pub(crate) fn knows_room(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    ctx.state.muc.nicks.contains_key(room) || room_row(ctx, room).is_some()
}

/// Send an IQ that needs our presence in `room`. While the join runs, the IQ waits, and
/// the send starts the join if none runs. Check `knows_room` first.
pub(crate) fn request_in_room(ctx: &mut Ctx<'_>, room: &BareJid, iq: Iq, then: super::Pending) {
    if ctx.state.muc.nicks.contains_key(room) {
        ctx.request(iq, then);
        return;
    }
    if !ctx.state.muc.joins.contains_key(room) {
        join_room(ctx, room, None, None, None);
    }
    ctx.state
        .muc
        .iq_outbox
        .entry(room.clone())
        .or_default()
        .push((iq, then));
}

fn drop_outbox(ctx: &mut Ctx<'_>, room: &BareJid) {
    if let Some((_, reply)) = ctx.state.muc.pending_nick.remove(room) {
        let _ = reply.send(Err(ClientError::Invalid(
            "the join of the room failed".into(),
        )));
    }
    // A waiting IQ gets an error answer, so that its command does not wait forever.
    for (_, then) in ctx.state.muc.iq_outbox.remove(room).unwrap_or_default() {
        let error = xmpp_parsers::stanza_error::StanzaError::new(
            xmpp_parsers::stanza_error::ErrorType::Cancel,
            xmpp_parsers::stanza_error::DefinedCondition::NotAcceptable,
            "en",
            "the join of the room failed",
        );
        super::on_iq_response(ctx, then, IqResponse::Error(error));
    }
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
/// The room configuration fields that Chord turns on in a room that it creates, when the
/// form of the server has them. `enablearchiving` is the Prosody name for the archive,
/// `mam` the ejabberd name. ejabberd rejects a form with a field that it does not know.
const ROOM_CONFIG_ON: &[&str] = &[
    "muc#roomconfig_persistentroom",
    "muc#roomconfig_enablearchiving",
    "mam",
];

/// Unlock a room that we created (XEP-0045, 10.1). Ask for its configuration form first,
/// so that the submit has only fields that the server knows.
fn unlock_room(ctx: &mut Ctx<'_>, room: &BareJid, replies: Vec<Reply>) {
    let query: Element = format!("<query xmlns='{NS_MUC_OWNER}'/>")
        .parse()
        .expect("static XML");
    let iq = Iq::Get {
        from: None,
        to: Some(Jid::from(room.clone())),
        id: String::new(),
        payload: query,
    };
    ctx.request(
        iq,
        super::Pending::Muc(Pending::RoomConfigForm(room.clone(), replies)),
    );
}

/// Submit the configuration of a new room. With no form from the server, submit an empty
/// form: that accepts the default configuration (an instant room, XEP-0045, 10.1.2).
fn submit_room_config(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    form: Option<&DataForm>,
    replies: Vec<Reply>,
) {
    let mut fields = String::new();
    if let Some(form) = form {
        fields.push_str(
            "<field var='FORM_TYPE'><value>http://jabber.org/protocol/muc#roomconfig</value></field>",
        );
        for field in &form.fields {
            if let Some(var) = field.var.as_deref()
                && ROOM_CONFIG_ON.contains(&var)
            {
                fields.push_str(&format!("<field var='{var}'><value>1</value></field>"));
            }
        }
    }
    let query: Element = format!(
        "<query xmlns='{NS_MUC_OWNER}'><x xmlns='jabber:x:data' type='submit'>{fields}</x></query>"
    )
    .parse()
    .expect("the field names are static XML");
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
            // Chord asks for the form first. The server offers the Prosody fields.
            let form: Element = "<query xmlns='http://jabber.org/protocol/muc#owner'>\
                <x xmlns='jabber:x:data' type='form'>\
                <field var='FORM_TYPE' type='hidden'><value>http://jabber.org/protocol/muc#roomconfig</value></field>\
                <field var='muc#roomconfig_persistentroom' type='boolean'><value>0</value></field>\
                <field var='muc#roomconfig_enablearchiving' type='boolean'><value>0</value></field>\
                </x></query>"
                .parse()
                .unwrap();
            h.answer(
                |p| matches!(p, super::super::Pending::Muc(Pending::RoomConfigForm(..))),
                Some(form),
            );
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

    #[test]
    fn a_moderation_from_the_room_retracts_a_message() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let m = groupchat("bob", "buy now", Some("s-9"), None);
        h.with_ctx(|ctx| on_message(ctx, &m));
        let mut retract = Message::groupchat(jid(ACCOUNT));
        retract.from = Some(jid(ROOM));
        retract.payloads.push(
            Element::builder("retract", "urn:xmpp:message-retract:1")
                .attr(
                    xmpp_parsers::minidom::rxml::NcName::try_from("id").unwrap(),
                    "s-9",
                )
                .append(Element::builder("moderated", "urn:xmpp:message-moderate:1").build())
                .build(),
        );
        h.with_ctx(|ctx| on_message(ctx, &retract));
        let rows = peer_rows(&h, ROOM);
        assert_eq!(rows.len(), 1);
        let retracted: Option<i64> = h
            .store
            .conn()
            .query_row("SELECT retracted_at FROM messages", [], |r| r.get(0))
            .unwrap();
        assert!(retracted.is_some());
    }

    fn occupant_id_element(id: &str) -> Element {
        format!("<occupant-id xmlns='{NS_OCCUPANT_ID}' id='{id}'/>")
            .parse()
            .unwrap()
    }

    fn with_occupant_id(mut message: Message, id: &str) -> Message {
        message.payloads.push(occupant_id_element(id));
        message
    }

    fn joined_with_occupant_id(h: &mut Harness, nick: &str, id: &str) {
        let mut answer = join(h, nick);
        let mut p = self_presence(nick);
        p.payloads.push(occupant_id_element(id));
        h.with_ctx(|ctx| on_presence(ctx, &p));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        h.take_sent();
    }

    fn incoming_directions(h: &mut Harness, messages: Vec<Message>) -> Vec<Direction> {
        for m in messages {
            h.with_ctx(|ctx| on_message(ctx, &m));
        }
        messages_with(h.store.conn(), h.account_id, ROOM)
            .unwrap()
            .iter()
            .map(|m| m.direction)
            .collect()
    }

    #[test]
    fn join_presence_carries_caps_and_our_avatar_hash() {
        use xmpp_parsers::vcard_update::VCardUpdate;
        let payloads = |h: &mut Harness| {
            let sent = h.take_sent();
            let [Stanza::Presence(p)] = sent.as_slice() else {
                panic!("{sent:?}")
            };
            p.payloads.clone()
        };
        let mut h = Harness::new();
        let _answer = join(&mut h, "alice");
        let none = payloads(&mut h);
        assert!(
            none.iter()
                .any(|e| e.is("c", "http://jabber.org/protocol/caps"))
        );
        assert!(none.iter().any(|e| Muc::try_from(e.clone()).is_ok()));
        assert!(
            !none
                .iter()
                .any(|e| VCardUpdate::try_from(e.clone()).is_ok())
        );

        let hash = "0123456789abcdef0123456789abcdef01234567";
        h.store
            .conn()
            .execute(
                "INSERT INTO avatars (account_id, owner, hash, mime, data)
                 VALUES (?1, ?2, ?3, 'image/png', x'00')",
                params![h.account_id, ACCOUNT, hash],
            )
            .unwrap();
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
        h.take_sent();
        let _answer = join(&mut h, "alice");
        let with = payloads(&mut h);
        let photo = with
            .iter()
            .find_map(|e| VCardUpdate::try_from(e.clone()).ok())
            .and_then(|u| u.photo)
            .and_then(|p| p.data);
        assert_eq!(photo.map(|d| d[0]), Some(0x01));
        assert!(with.iter().any(|e| Muc::try_from(e.clone()).is_ok()));
    }

    fn change_nick(h: &mut Harness, nick: &str) -> Answer {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::ChangeNick {
                    room: room(),
                    nick: nick.into(),
                    reply,
                },
            )
        });
        answer
    }

    #[test]
    fn change_nick_sends_a_plain_presence_and_completes_after_the_303() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let mut answer = change_nick(&mut h, "alicia");
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/alicia"))));
        assert!(!p.payloads.iter().any(|e| Muc::try_from(e.clone()).is_ok()));
        assert_eq!(answer.try_recv().unwrap(), None);

        let rename = Presence::unavailable()
            .with_from(jid(&format!("{ROOM}/alice")))
            .with_payload(
                MucUser::new()
                    .with_statuses(vec![Status::NewNick, Status::SelfPresence])
                    .with_items(vec![
                        Item::new(Affiliation::Member, Role::Participant).with_nick("alicia"),
                    ]),
            );
        h.with_ctx(|ctx| on_presence(ctx, &rename));
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("alicia")));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        h.with_ctx(|ctx| {
            assert_eq!(
                ctx.state.muc.nicks.get(&room()).map(String::as_str),
                Some("alicia")
            );
            assert_eq!(our_nick(ctx, &room()).as_deref(), Some("alicia"));
        });
        assert_eq!(occupant_nicks(&h), ["alicia"]);
    }

    #[test]
    fn change_nick_conflict_keeps_the_old_nick_and_outside_a_room_fails() {
        let mut h = Harness::new();
        let mut answer = change_nick(&mut h, "alicia");
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));

        joined(&mut h, "alice");
        let mut answer = change_nick(&mut h, "bob");
        h.take_sent();
        let error = Presence::error()
            .with_from(jid(&format!("{ROOM}/bob")))
            .with_payload(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::Conflict,
                "en",
                "",
            ));
        h.with_ctx(|ctx| on_presence(ctx, &error));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
        let stored: String = h
            .store
            .conn()
            .query_row("SELECT nick FROM rooms WHERE jid = ?1", [ROOM], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stored, "alice");
        h.with_ctx(|ctx| {
            assert_eq!(
                ctx.state.muc.nicks.get(&room()).map(String::as_str),
                Some("alice")
            );
        });
    }

    #[test]
    fn a_message_from_an_earlier_nick_is_ours() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let mut answer = change_nick(&mut h, "alicia");
        let rename = Presence::unavailable()
            .with_from(jid(&format!("{ROOM}/alice")))
            .with_payload(
                MucUser::new()
                    .with_statuses(vec![Status::NewNick, Status::SelfPresence])
                    .with_items(vec![
                        Item::new(Affiliation::Member, Role::Participant).with_nick("alicia"),
                    ]),
            );
        h.with_ctx(|ctx| on_presence(ctx, &rename));
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("alicia")));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let directions = incoming_directions(
            &mut h,
            vec![
                groupchat("alice", "old nick", Some("s1"), None),
                groupchat("alicia", "new nick", Some("s2"), None),
                groupchat("bob", "other", Some("s3"), None),
            ],
        );
        assert_eq!(directions, [Direction::Out, Direction::Out, Direction::In]);
    }

    #[test]
    fn our_occupant_id_marks_a_message_as_ours_whatever_the_nick() {
        let mut h = Harness::new();
        joined_with_occupant_id(&mut h, "alice", "occ-me");
        h.with_ctx(|ctx| {
            assert_eq!(
                ctx.state.muc.occupant_ids.get(&room()).map(String::as_str),
                Some("occ-me")
            );
        });
        let directions = incoming_directions(
            &mut h,
            vec![
                // A nick that we never used in this session, as the archive shows it.
                with_occupant_id(groupchat("alice-old", "mine", Some("s1"), None), "occ-me"),
                with_occupant_id(groupchat("bob", "theirs", Some("s2"), None), "occ-bob"),
                // Somebody took a nick of ours: the occupant-id says it is not us.
                with_occupant_id(groupchat("alice", "imposter", Some("s3"), None), "occ-x"),
            ],
        );
        assert_eq!(directions, [Direction::Out, Direction::In, Direction::In]);
    }

    fn joined_as(h: &mut Harness, nick: &str, affiliation: Affiliation) {
        let mut answer = join(h, nick);
        h.with_ctx(|ctx| {
            on_presence(
                ctx,
                &occupant_presence(
                    nick,
                    vec![Status::SelfPresence],
                    Item::new(affiliation, Role::Participant),
                ),
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        h.take_sent();
        h.take_dirty();
    }

    fn command<T>(
        h: &mut Harness,
        make: impl FnOnce(oneshot::Sender<Result<T, ClientError>>) -> Command,
    ) -> oneshot::Receiver<Result<T, ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, make(reply)));
        answer
    }

    fn is_muc(p: &super::super::Pending) -> bool {
        matches!(p, super::super::Pending::Muc(_))
    }

    fn sent_messages(h: &mut Harness) -> Vec<Message> {
        h.take_sent()
            .into_iter()
            .filter_map(|s| match s {
                Stanza::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn forbidden() -> IqResponse {
        IqResponse::Error(StanzaError::new(
            ErrorType::Auth,
            DefinedCondition::Forbidden,
            "en",
            "",
        ))
    }

    #[test]
    fn set_affiliation_sends_an_admin_item_and_maps_errors() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::SetAffiliation {
            room: room(),
            jid: BareJid::new("bob@example.org").unwrap(),
            affiliation: RoomAffiliation::Member,
            reason: Some("welcome".into()),
            reply,
        });
        let iqs = h.sent_iqs();
        let Some(Iq::Set { to, payload, .. }) = iqs.first() else {
            panic!("{iqs:?}")
        };
        assert_eq!(to.as_ref(), Some(&jid(ROOM)));
        assert!(payload.is("query", NS_MUC_ADMIN));
        let item = payload.get_child("item", NS_MUC_ADMIN).expect("item");
        assert_eq!(item.attr("affiliation"), Some("member"));
        assert_eq!(item.attr("jid"), Some("bob@example.org"));
        assert_eq!(
            item.get_child("reason", NS_MUC_ADMIN).unwrap().text(),
            "welcome"
        );
        h.answer(is_muc, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));

        let mut answer = command(&mut h, |reply| Command::SetAffiliation {
            room: room(),
            jid: BareJid::new("bob@example.org").unwrap(),
            affiliation: RoomAffiliation::Outcast,
            reason: None,
            reply,
        });
        h.respond(is_muc, forbidden());
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
    }

    #[test]
    fn room_affiliations_asks_for_one_affiliation_and_reads_the_items() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::ListAffiliations {
            room: room(),
            affiliation: RoomAffiliation::Member,
            reply,
        });
        let iqs = h.sent_iqs();
        let Some(Iq::Get { payload, .. }) = iqs.first() else {
            panic!("{iqs:?}")
        };
        let item = payload.get_child("item", NS_MUC_ADMIN).expect("item");
        assert_eq!(item.attr("affiliation"), Some("member"));
        let result: Element = format!(
            "<query xmlns='{NS_MUC_ADMIN}'>\
             <item affiliation='member' jid='bob@example.org' nick='bob'/>\
             <item affiliation='member' jid='carol@example.org'/></query>"
        )
        .parse()
        .unwrap();
        h.answer(is_muc, Some(result));
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Ok(vec![
                (BareJid::new("bob@example.org").unwrap(), Some("bob".into())),
                (BareJid::new("carol@example.org").unwrap(), None),
            ]))
        );
    }

    fn invite(h: &mut Harness) -> oneshot::Receiver<Result<(), ClientError>> {
        command(h, |reply| Command::Invite {
            room: room(),
            jid: BareJid::new("bob@example.org").unwrap(),
            reason: Some("join us".into()),
            reply,
        })
    }

    fn check_invitation(message: &Message) {
        assert_eq!(message.to.as_ref(), Some(&jid(ROOM)));
        let x = message
            .payloads
            .iter()
            .find(|p| p.is("x", NS_MUC_USER))
            .expect("x");
        let invite = x.get_child("invite", NS_MUC_USER).expect("invite");
        assert_eq!(invite.attr("to"), Some("bob@example.org"));
        assert_eq!(
            invite.get_child("reason", NS_MUC_USER).unwrap().text(),
            "join us"
        );
    }

    #[test]
    fn an_owner_grants_membership_before_the_invitation() {
        let mut h = Harness::new();
        joined_as(&mut h, "alice", Affiliation::Owner);
        let mut answer = invite(&mut h);
        // Only the grant goes out at first.
        let sent = h.take_sent();
        assert_eq!(sent.len(), 1, "{sent:?}");
        let Stanza::Iq(Iq::Set { payload, .. }) = &sent[0] else {
            panic!("{sent:?}")
        };
        let item = payload.get_child("item", NS_MUC_ADMIN).unwrap();
        assert_eq!(item.attr("affiliation"), Some("member"));
        assert_eq!(item.attr("jid"), Some("bob@example.org"));
        assert_eq!(answer.try_recv().unwrap(), None);
        h.answer(is_muc, None);
        let messages = sent_messages(&mut h);
        assert_eq!(messages.len(), 1);
        check_invitation(&messages[0]);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn a_failed_grant_still_sends_the_invitation() {
        let mut h = Harness::new();
        joined_as(&mut h, "alice", Affiliation::Admin);
        let mut answer = invite(&mut h);
        h.take_sent();
        h.respond(is_muc, forbidden());
        assert_eq!(sent_messages(&mut h).len(), 1);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn without_the_right_the_invitation_goes_out_alone() {
        let mut h = Harness::new();
        joined_as(&mut h, "alice", Affiliation::Member);
        let mut answer = invite(&mut h);
        let sent = h.take_sent();
        assert_eq!(sent.len(), 1, "{sent:?}");
        let Stanza::Message(message) = &sent[0] else {
            panic!("{sent:?}")
        };
        check_invitation(message);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        // A room that we are not in: the same.
        let mut h = Harness::new();
        let mut answer = invite(&mut h);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(sent_messages(&mut h).len(), 1);
    }

    fn from_sender(from: &str, payload: &str) -> Message {
        let mut message = Message::new(Some(jid(ACCOUNT)));
        message.from = Some(jid(from));
        message.payloads.push(payload.parse().unwrap());
        message
    }

    fn invites(h: &Harness) -> Vec<ClientEvent> {
        h.effects
            .iter()
            .filter_map(|e| match e {
                super::super::Effect::Emit(e @ ClientEvent::RoomInvite { .. }) => Some(e.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_mediated_invitation_emits_an_event_and_does_not_join() {
        let mut h = Harness::new();
        let message = from_sender(
            ROOM,
            "<x xmlns='http://jabber.org/protocol/muc#user'>\
             <invite from='alice@example.org/phone'><reason>Come</reason></invite>\
             <password>secret</password></x>",
        );
        h.with_ctx(|ctx| assert!(on_message(ctx, &message)));
        assert_eq!(
            invites(&h),
            [ClientEvent::RoomInvite {
                room: room(),
                from: jid("alice@example.org/phone"),
                reason: Some("Come".into()),
                password: Some("secret".into()),
            }]
        );
        assert!(h.take_sent().is_empty());
        assert!(!h.with_ctx(|ctx| is_room(ctx, &room())));
    }

    #[test]
    fn a_direct_invitation_emits_an_event() {
        let mut h = Harness::new();
        let message = from_sender(
            "alice@example.org/phone",
            &format!("<x xmlns='jabber:x:conference' jid='{ROOM}' password='pw' reason='Hi'/>"),
        );
        h.with_ctx(|ctx| assert!(on_message(ctx, &message)));
        assert_eq!(
            invites(&h),
            [ClientEvent::RoomInvite {
                room: room(),
                from: jid("alice@example.org/phone"),
                reason: Some("Hi".into()),
                password: Some("pw".into()),
            }]
        );
    }

    #[test]
    fn decline_sends_a_decline_to_the_room() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::DeclineInvite {
            room: room(),
            from: BareJid::new("alice@example.org").unwrap(),
            reason: Some("no".into()),
            reply,
        });
        let messages = sent_messages(&mut h);
        let decline = messages[0].payloads[0]
            .get_child("decline", NS_MUC_USER)
            .unwrap();
        assert_eq!(decline.attr("to"), Some("alice@example.org"));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn configure_room_submits_only_the_fields_that_the_form_has() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::Configure {
            room: room(),
            settings: RoomSettings {
                name: Some("General".into()),
                public: Some(true),
                members_only: Some(false),
            },
            reply,
        });
        let iqs = h.sent_iqs();
        assert!(matches!(&iqs[0], Iq::Get { payload, .. } if payload.is("query", NS_MUC_OWNER)));
        // The form has no public room field.
        let form: Element = format!(
            "<query xmlns='{NS_MUC_OWNER}'><x xmlns='jabber:x:data' type='form'>\
             <field var='FORM_TYPE' type='hidden'>\
             <value>http://jabber.org/protocol/muc#roomconfig</value></field>\
             <field var='muc#roomconfig_roomname' type='text-single'/>\
             <field var='muc#roomconfig_membersonly' type='boolean'/></x></query>"
        )
        .parse()
        .unwrap();
        h.answer(is_muc, Some(form));
        let iqs = h.sent_iqs();
        let Some(Iq::Set { to, payload, .. }) = iqs.first() else {
            panic!("{iqs:?}")
        };
        assert_eq!(to.as_ref(), Some(&jid(ROOM)));
        let x = payload.get_child("x", NS_DATA).unwrap();
        assert_eq!(x.attr("type"), Some("submit"));
        let value = |var: &str| {
            x.children()
                .find(|f| f.attr("var") == Some(var))
                .map(|f| f.get_child("value", NS_DATA).unwrap().text())
        };
        assert_eq!(value("muc#roomconfig_roomname").as_deref(), Some("General"));
        assert_eq!(value("muc#roomconfig_membersonly").as_deref(), Some("0"));
        assert_eq!(value("muc#roomconfig_publicroom"), None);
        assert!(value("FORM_TYPE").is_some());
        h.answer(is_muc, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn a_send_to_an_unjoined_room_fails_instead_of_a_chat() {
        use xmpp_parsers::disco::{DiscoInfoResult, Identity};
        let mut h = Harness::new();
        let info = DiscoInfoResult {
            node: None,
            identities: vec![Identity::new("conference", "text", "en", "Rooms")],
            features: Default::default(),
            extensions: vec![],
        };
        let service = jid(ROOM).to_bare().domain().to_string();
        h.state.disco.services.push((jid(&service), info));
        let result = h.with_ctx(|ctx| send_chat(ctx, jid(ROOM), "hi".into()));
        assert!(matches!(result, Err(ClientError::Invalid(_))), "{result:?}");
        assert!(h.take_sent().is_empty(), "no chat message goes out");
        // A contact on the account domain still gets a chat message.
        let result = h.with_ctx(|ctx| send_chat(ctx, jid("bob@chord.localhost"), "hi".into()));
        assert!(result.is_ok());
    }
}
