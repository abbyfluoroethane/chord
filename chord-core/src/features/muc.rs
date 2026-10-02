//! Multi-user chat (XEP-0045): join, leave, occupants, and groupchat messages.
//!
//! Private messages between occupants (section 7.5) are chat rows with the peer
//! `room@service/nick`. Their timeline is `ViewKey::PrivateTimeline`.

use std::collections::{HashMap, HashSet};

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::data_forms::DataForm;
use xmpp_parsers::disco::DiscoInfoResult;
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

mod captcha;
pub(super) mod health;
mod password;
mod rejoin;
mod status;

use super::chat::{MessageIds, delay_ms};
use super::message_ext::{self, Incoming, Outgoing};
use super::{Ctx, IqResponse, bookmarks, mam, new_id, presence as own_presence};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::secrets::SecretStore;
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
const NS_DISCO_INFO: &str = "http://jabber.org/protocol/disco#info";
/// The form type of the room information in a disco#info answer (XEP-0045, 15.5).
const NS_ROOM_INFO: &str = "http://jabber.org/protocol/muc#roominfo";
/// The disco#info node that answers with our reserved nick (XEP-0045, 7.12).
const NODE_RESERVED_NICK: &str = "x-roomuser-item";
/// Mediated invitations that we remember for a possible fallback. Older ones go.
const MAX_PENDING_INVITES: usize = 64;

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

/// The role of an occupant in a room (XEP-0045, section 5.1). `set_room_role` sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum RoomRole {
    /// Out of the room: a kick.
    None,
    /// Can read, cannot speak: a mute.
    Visitor,
    /// Can speak: a grant of voice.
    Participant,
    Moderator,
}

impl RoomRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Visitor => "visitor",
            Self::Participant => "participant",
            Self::Moderator => "moderator",
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
    /// The password to enter the room (`muc#roomconfig_roomsecret`). An empty string
    /// removes the password.
    pub password: Option<String>,
}

/// What `room_info` reads about a room, for an invite card. It is read only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct RoomCard {
    pub jid: String,
    /// The name of the room, from the disco identity.
    pub name: Option<String>,
    pub description: Option<String>,
    pub subject: Option<String>,
    /// The number of people in the room (`muc#roominfo_occupants`).
    pub occupants: Option<u32>,
    pub password_protected: bool,
    pub members_only: bool,
    /// True when any occupant may change the subject (`muc#roominfo_changesubject`).
    /// False when only moderators may, or when the room does not say.
    pub change_subject: bool,
    /// Who sees the real addresses: `non-anonymous` (everyone), `semi-anonymous`
    /// (moderators) or `anonymous` (nobody). `None` when the room does not say.
    pub anonymity: Option<String>,
}

/// Join calls that wait for the answer to the question for the reserved nick.
#[derive(Debug)]
pub(super) struct Reserving {
    password: Option<String>,
    /// The nick to use when the room reserved none.
    fallback: Option<String>,
    replies: Vec<Reply>,
}

/// A join that waits for the self-presence or an error.
#[derive(Debug)]
pub(super) struct Join {
    nick: String,
    /// True for a nick change in a room that we are in already.
    changing_nick: bool,
    replies: Vec<Reply>,
    /// Ticks since the join started. `health::on_tick` ends a join that waits too long.
    ticks: u8,
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
    /// Subject changes that wait for the room: the message id, the room, and the reply.
    /// The echo of the subject or an error message answers them.
    subjects: HashMap<String, (BareJid, Reply)>,
    /// Joins that wait for the reserved nick of the room (XEP-0045, 7.12).
    reserving: HashMap<BareJid, Reserving>,
    /// The self-ping of the rooms (XEP-0410).
    pub(super) ping: health::PingState,
    /// Rooms that the service removed us from for a reason that passes (332, 333). Each
    /// one joins again after a wait.
    rejoin: rejoin::Rejoins,
    /// Rooms whose disco#info we read again after a configuration change.
    refreshing: HashSet<BareJid>,
    /// Mediated invitations that wait for an error, by message id. A room that refuses
    /// one gets a direct invitation instead.
    invites: HashMap<String, PendingInvite>,
    /// The keychain that holds the room passwords, when the client has one.
    secrets: Option<std::sync::Arc<dyn SecretStore>>,
}

/// A mediated invitation that we sent and that nobody answered yet.
#[derive(Debug)]
struct PendingInvite {
    room: BareJid,
    jid: BareJid,
    reason: Option<String>,
}

impl State {
    /// Keep the room passwords in `secrets`.
    pub(crate) fn set_secret_store(&mut self, secrets: std::sync::Arc<dyn SecretStore>) {
        self.secrets = Some(secrets);
    }
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
        /// True for a direct invitation (XEP-0249), false for a mediated one.
        direct: bool,
        reply: Reply,
    },
    /// The configuration form of a room, for `configure_room`.
    SettingsForm(BareJid, RoomSettings, Reply),
    /// The disco#info of a room, for `room_info`.
    Card(BareJid, oneshot::Sender<Result<RoomCard, ClientError>>),
    /// The answer of a room to the question for our reserved nick. The join follows.
    ReservedNick(BareJid),
    /// The answer to `grant_membership`. Nobody waits for it.
    Grant {
        room: BareJid,
        jid: BareJid,
    },
    /// The answer to a XEP-0410 ping of our own occupant JID in the room.
    SelfPing(BareJid),
    /// The disco#info of a room that changed its configuration (status 104).
    Refresh(BareJid),
    /// The answer to a CAPTCHA that we sent to a room (XEP-0158).
    Captcha(Reply),
}

/// A command from the public API.
pub(crate) enum Command {
    /// The disco#info of a room.
    Card {
        room: BareJid,
        reply: oneshot::Sender<Result<RoomCard, ClientError>>,
    },
    /// The room service of our server. It waits for service discovery.
    RoomService {
        reply: oneshot::Sender<Result<Option<BareJid>, ClientError>>,
    },
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
        /// `Some(true)` puts the room password in the bookmark, `Some(false)` keeps it out,
        /// `None` does what the bookmark did before.
        share_password: Option<bool>,
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
    /// The answer to the CAPTCHA of a join (XEP-0158), or `None` to give it up.
    Captcha {
        room: BareJid,
        answer: Option<crate::forms::Form>,
        reply: Reply,
    },
    /// A direct invitation (XEP-0249).
    InviteDirect {
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
    /// Join with the nick that the room reserved for us, else the default nick.
    JoinDefault {
        room: BareJid,
        password: Option<String>,
        fallback: Option<String>,
        reply: Reply,
    },
    SetSubject {
        room: BareJid,
        subject: String,
        reply: Reply,
    },
    SetRole {
        room: BareJid,
        nick: String,
        role: RoomRole,
        reason: Option<String>,
        reply: Reply,
    },
    Destroy {
        room: BareJid,
        reason: Option<String>,
        alternate: Option<BareJid>,
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

    /// Join a room with no nick of our own. If the room reserved a nick for us (XEP-0045,
    /// section 7.12), we use it. Else the nick is the stored one, then `fallback`, then the
    /// local part of our JID. Errors are the same as for `join_room`.
    pub async fn join_room_default_nick(
        &self,
        room: BareJid,
        password: Option<String>,
        fallback: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::JoinDefault {
            room,
            password,
            fallback,
            reply,
        })
        .await
    }

    /// Set the subject of a room that we are in (XEP-0045, section 8.1) and wait for the
    /// room to announce it. An empty subject clears it. Fails with `ClientError::Invalid`
    /// when we are not in the room, and with `ClientError::Server` when the room refuses
    /// (`forbidden`: only moderators may change the subject).
    pub async fn set_room_subject(
        &self,
        room: BareJid,
        subject: String,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::SetSubject {
            room,
            subject,
            reply,
        })
        .await
    }

    /// Set the role of an occupant, by nick (XEP-0045, sections 8.2, 8.3, 9.1 and 9.2):
    /// `None` kicks, `Visitor` mutes, `Participant` grants voice, `Moderator` makes a
    /// moderator. We need the right to do it. Fails with `ClientError::Server` when the
    /// room refuses (`forbidden`, `not-allowed`, `item-not-found`).
    pub async fn set_room_role(
        &self,
        room: BareJid,
        nick: String,
        role: RoomRole,
        reason: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::SetRole {
            room,
            nick,
            role,
            reason,
            reply,
        })
        .await
    }

    /// Destroy a room that we own (XEP-0045, section 10.9). The room tells its occupants
    /// the `reason` and the `alternate` room, if given. Fails with `ClientError::Server`
    /// when we are not an owner (`forbidden`).
    pub async fn destroy_room(
        &self,
        room: BareJid,
        reason: Option<String>,
        alternate: Option<BareJid>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::Destroy {
            room,
            reason,
            alternate,
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

    /// Leave a room. The bookmark of the room, if it has one, is retracted, so that the
    /// room does not come back at the next login. The answer waits for the server.
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
            share_password: None,
            reply,
        })
        .await
    }

    /// Like `add_bookmark`, and says if the bookmark carries the password of the room.
    /// `add_bookmark` puts it in only when an earlier bookmark of the room had it. The
    /// bookmarks are private to the account, but the password then sits on the server in
    /// clear text, so the user decides.
    pub async fn add_bookmark_with_password_choice(
        &self,
        room: BareJid,
        name: Option<String>,
        autojoin: bool,
        nick: Option<String>,
        share_password: bool,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::AddBookmark {
            room,
            name,
            autojoin,
            nick,
            share_password: Some(share_password),
            reply,
        })
        .await
    }

    /// Remove a room from the bookmarks. This does not leave the room.
    pub async fn remove_bookmark(&self, room: BareJid) -> Result<(), ClientError> {
        self.room_command(|reply| Command::RemoveBookmark { room, reply })
            .await
    }

    /// The room service of our server (the disco item with the XEP-0045 identity
    /// conference/text), for example `conference.example.org`. A new room goes there.
    /// `None` when the server has no room service. Waits for service discovery.
    pub async fn room_service(&self) -> Result<Option<BareJid>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Muc(Command::RoomService { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
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

    /// Invite a JID to a room with a direct invitation (XEP-0249): a message to the person,
    /// with no help from the room. Use it for a room that does not pass on mediated
    /// invitations. `invite_to_room` falls back to it by itself when the room answers
    /// with an error such as `forbidden` or `not-allowed`. The membership grant works as
    /// in `invite_to_room`.
    pub async fn invite_to_room_direct(
        &self,
        room: BareJid,
        jid: BareJid,
        reason: Option<String>,
    ) -> Result<(), ClientError> {
        self.room_command(|reply| Command::InviteDirect {
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

    /// Read the name, the subject, and the number of people of a room, with a disco#info
    /// query. It does not join the room. Fails when the room does not exist or when the
    /// server refuses the query.
    pub async fn room_info(&self, room: BareJid) -> Result<RoomCard, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Muc(Command::Card { room, reply }))?;
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
    for (_, (_, reply)) in ctx.state.muc.subjects.drain() {
        let _ = reply.send(Err(ClientError::NotConnected));
    }
    for (_, waiting) in ctx.state.muc.reserving.drain() {
        for reply in waiting.replies {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
    State {
        previous,
        secrets: ctx.state.muc.secrets.take(),
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
        Pending::Card(room, reply) => {
            let _ = reply.send(match response {
                IqResponse::Result(Some(query)) => room_card(&room, query),
                IqResponse::Result(None) => Err(ClientError::Server("empty answer".into())),
                IqResponse::Error(e) => Err(ClientError::Server(error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            });
        }
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
        Pending::ReservedNick(room) => {
            // An error or an empty answer means no reserved nick: use the default.
            let lost = matches!(response, IqResponse::Lost);
            let reserved = match response {
                IqResponse::Result(Some(query)) => reserved_nick(&query),
                IqResponse::Result(None) | IqResponse::Error(_) | IqResponse::Lost => None,
            };
            if let Some(waiting) = ctx.state.muc.reserving.remove(&room) {
                let Reserving {
                    password,
                    fallback,
                    replies,
                } = waiting;
                if lost {
                    for reply in replies {
                        let _ = reply.send(Err(ClientError::NotConnected));
                    }
                    return;
                }
                let nick = reserved.or(fallback).or_else(|| default_nick(ctx, &room));
                for reply in replies {
                    join_room(ctx, &room, nick.clone(), password.clone(), Some(reply));
                }
            }
        }
        Pending::SelfPing(room) => health::on_response(ctx, room, response),
        Pending::Refresh(room) => status::on_refreshed(ctx, room, response),
        Pending::Captcha(reply) => captcha::on_answer(ctx, reply, response),
        Pending::Grant { room, jid } => match response {
            IqResponse::Result(_) => log::debug!("made {jid} a member of {room}"),
            IqResponse::Error(e) => {
                // Normal when we do not own the room. The room may be open anyway.
                log::info!("cannot make {jid} a member of {room}: {}", error_text(&e));
            }
            IqResponse::Lost => {}
        },
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
            direct,
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
            if direct {
                send_direct_invitation(ctx, &room, &jid, reason);
            } else {
                send_invitation(ctx, &room, &jid, reason);
            }
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
        Command::Card { room, reply } => {
            let iq = Iq::Get {
                from: None,
                to: Some(Jid::from(room.clone())),
                id: String::new(),
                payload: format!("<query xmlns='{NS_DISCO_INFO}'/>")
                    .parse()
                    .expect("static XML"),
            };
            ctx.request(iq, super::Pending::Muc(Pending::Card(room, reply)));
        }
        Command::Join {
            room,
            nick,
            password,
            reply,
        } => join_room(ctx, &room, Some(nick), password, Some(reply)),
        Command::Leave { room, reply } => {
            // A bookmarked room would come back at the next login: retract the bookmark
            // (XEP-0402, section 3) and answer when the server has done it.
            let bookmarked = bookmarks::is_bookmarked(ctx, &room);
            match leave(ctx, &room) {
                Ok(()) if bookmarked => bookmarks::remove(ctx, room, reply),
                result => {
                    let _ = reply.send(result);
                }
            }
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
            share_password,
            reply,
        } => bookmarks::add(ctx, room, name, autojoin, nick, share_password, reply),
        Command::RemoveBookmark { room, reply } => bookmarks::remove(ctx, room, reply),
        Command::RoomService { reply } => {
            let service = ctx.state.disco.services.iter().find_map(|(jid, info)| {
                info.identities
                    .iter()
                    .any(|i| i.category == "conference" && i.type_ == "text")
                    .then(|| jid.to_bare())
            });
            let _ = reply.send(Ok(service));
        }
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
                    direct: false,
                    reply,
                };
                ctx.request(iq, super::Pending::Muc(pending));
            } else {
                send_invitation(ctx, &room, &jid, reason);
                let _ = reply.send(Ok(()));
            }
        }
        Command::Captcha {
            room,
            answer,
            reply,
        } => captcha::on_command(ctx, room, answer, reply),
        Command::InviteDirect {
            room,
            jid,
            reason,
            reply,
        } => {
            // A member enters a members-only room: grant it first, as a mediated invitation does.
            if can_grant(ctx, &room) {
                let iq = affiliation_iq(&room, &jid, RoomAffiliation::Member, None);
                let pending = Pending::InviteGrant {
                    room,
                    jid,
                    reason,
                    direct: true,
                    reply,
                };
                ctx.request(iq, super::Pending::Muc(pending));
            } else {
                send_direct_invitation(ctx, &room, &jid, reason);
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
            message.type_ = MessageType::Normal;
            message.id = Some(Id(new_id()));
            message.payloads.push(x);
            ctx.send(message);
            let _ = reply.send(Ok(()));
        }
        Command::JoinDefault {
            room,
            password,
            fallback,
            reply,
        } => {
            let has_nick = ctx.state.muc.nicks.contains_key(&room)
                || ctx.state.muc.joins.contains_key(&room)
                || room_row(ctx, &room).is_some_and(|r| r.nick.is_some());
            if has_nick {
                join_room(ctx, &room, None, password, Some(reply));
            } else if let Some(waiting) = ctx.state.muc.reserving.get_mut(&room) {
                waiting.replies.push(reply);
            } else {
                // A first join: the room may have reserved a nick for us (XEP-0045, 7.12).
                ctx.state.muc.reserving.insert(
                    room.clone(),
                    Reserving {
                        password,
                        fallback,
                        replies: vec![reply],
                    },
                );
                let iq = Iq::Get {
                    from: None,
                    to: Some(Jid::from(room.clone())),
                    id: String::new(),
                    payload: format!(
                        "<query xmlns='{NS_DISCO_INFO}' node='{NODE_RESERVED_NICK}'/>"
                    )
                    .parse()
                    .expect("static XML"),
                };
                ctx.request(iq, super::Pending::Muc(Pending::ReservedNick(room)));
            }
        }
        Command::SetSubject {
            room,
            subject,
            reply,
        } => {
            if !ctx.state.muc.nicks.contains_key(&room) {
                let _ = reply.send(Err(ClientError::Invalid(format!("not in the room {room}"))));
                return;
            }
            let id = new_id();
            let mut message = Message::groupchat(Jid::from(room.clone()));
            message.id = Some(Id(id.clone()));
            message
                .subjects
                .insert(xmpp_parsers::message::Lang(String::new()), subject);
            ctx.send(message);
            ctx.state.muc.subjects.insert(id, (room, reply));
        }
        Command::SetRole {
            room,
            nick,
            role,
            reason,
            reply,
        } => {
            let mut item = Element::builder("item", NS_MUC_ADMIN)
                .attr(nc("nick"), nick)
                .attr(nc("role"), role.as_str());
            if let Some(reason) = reason.filter(|r| !r.is_empty()) {
                item = item.append(Element::builder("reason", NS_MUC_ADMIN).append(reason));
            }
            let iq = Iq::Set {
                from: None,
                to: Some(Jid::from(room)),
                id: String::new(),
                payload: Element::builder("query", NS_MUC_ADMIN).append(item).build(),
            };
            ctx.request(iq, super::Pending::Muc(Pending::Simple(reply)));
        }
        Command::Destroy {
            room,
            reason,
            alternate,
            reply,
        } => {
            let mut destroy = Element::builder("destroy", NS_MUC_OWNER);
            if let Some(alternate) = alternate {
                destroy = destroy.attr(nc("jid"), alternate.to_string());
            }
            if let Some(reason) = reason.filter(|r| !r.is_empty()) {
                destroy = destroy.append(Element::builder("reason", NS_MUC_OWNER).append(reason));
            }
            let iq = Iq::Set {
                from: None,
                to: Some(Jid::from(room)),
                id: String::new(),
                payload: Element::builder("query", NS_MUC_OWNER)
                    .append(destroy)
                    .build(),
            };
            ctx.request(iq, super::Pending::Muc(Pending::Simple(reply)));
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

/// The card of a room from its disco#info answer. An answer with no conference identity
/// is not a room.
fn room_card(room: &BareJid, query: Element) -> Result<RoomCard, ClientError> {
    let info =
        DiscoInfoResult::try_from(query).map_err(|_| ClientError::Server("bad answer".into()))?;
    let identity = info.identities.iter().find(|i| i.category == "conference");
    let Some(identity) = identity else {
        return Err(ClientError::Invalid("the address is not a room".into()));
    };
    let form = info
        .extensions
        .iter()
        .find(|f| f.form_type() == Some(NS_ROOM_INFO));
    let value = |var: &str| {
        form.and_then(|f| f.fields.iter().find(|f| f.var.as_deref() == Some(var)))
            .and_then(|f| f.values.first().cloned())
            .filter(|v| !v.trim().is_empty())
    };
    let feature = |name: &str| info.features.contains(&format!("muc_{name}"));
    Ok(RoomCard {
        jid: room.to_string(),
        name: identity.name.clone().filter(|n| !n.trim().is_empty()),
        description: value("muc#roominfo_description"),
        subject: value("muc#roominfo_subject"),
        occupants: value("muc#roominfo_occupants").and_then(|v| v.trim().parse().ok()),
        password_protected: feature("passwordprotected"),
        members_only: feature("membersonly"),
        change_subject: value("muc#roominfo_changesubject")
            .is_some_and(|v| matches!(v.trim(), "1" | "true")),
        anonymity: ["nonanonymous", "semianonymous", "anonymous"]
            .into_iter()
            .find(|name| feature(name))
            .map(|name| {
                match name {
                    "nonanonymous" => "non-anonymous",
                    "semianonymous" => "semi-anonymous",
                    _ => "anonymous",
                }
                .to_owned()
            }),
    })
}

/// The reserved nick in the disco#info answer of the node `x-roomuser-item`: the name of
/// its identity (XEP-0045, 7.12).
fn reserved_nick(query: &Element) -> Option<String> {
    query
        .children()
        .find(|c| c.name() == "identity")
        .and_then(|i| i.attr("name"))
        .filter(|n| !n.trim().is_empty())
        .map(str::to_owned)
}

/// The nick for a room with no nick of ours: the local part of our JID.
fn default_nick(ctx: &Ctx<'_>, room: &BareJid) -> Option<String> {
    room_row(ctx, room)
        .and_then(|row| row.nick)
        .or_else(|| ctx.account.node().map(|n| n.as_str().to_owned()))
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

/// Make `jid` a member of `room`, and ignore the answer. A space uses this for its
/// members: a members-only room lets in only a member. It fails if we cannot grant it.
pub(crate) fn grant_membership(ctx: &mut Ctx<'_>, room: &BareJid, jid: &BareJid) {
    let iq = affiliation_iq(room, jid, RoomAffiliation::Member, None);
    let pending = Pending::Grant {
        room: room.clone(),
        jid: jid.clone(),
    };
    ctx.request(iq, super::Pending::Muc(pending));
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
    let reason_copy = reason.clone();
    let mut invite = Element::builder("invite", NS_MUC_USER).attr(nc("to"), jid.to_string());
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        invite = invite.append(Element::builder("reason", NS_MUC_USER).append(reason));
    }
    let x = Element::builder("x", NS_MUC_USER).append(invite).build();
    let mut message = Message::new(Some(Jid::from(room.clone())));
    // A room takes an invitation in a normal message. A chat message is refused.
    message.type_ = MessageType::Normal;
    let id = new_id();
    message.id = Some(Id(id.clone()));
    message.payloads.push(x);
    // Keep it for a while: a room that refuses it gets a direct invitation instead.
    if ctx.state.muc.invites.len() >= MAX_PENDING_INVITES {
        ctx.state.muc.invites.clear();
    }
    ctx.state.muc.invites.insert(
        id,
        PendingInvite {
            room: room.clone(),
            jid: jid.clone(),
            reason: reason_copy,
        },
    );
    ctx.send(message);
}

/// Send a direct invitation (XEP-0249): a message to the person, with the room in it. It
/// does not need the room to pass it on, so it works in a room that refuses mediated
/// invitations. The password of the room, if we have one, goes with it.
fn send_direct_invitation(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    jid: &BareJid,
    reason: Option<String>,
) {
    let mut x = Element::builder("x", NS_CONFERENCE).attr(nc("jid"), room.to_string());
    if let Some(reason) = reason.filter(|r| !r.is_empty()) {
        x = x.attr(nc("reason"), reason);
    }
    if let Some(password) = stored_password(ctx, room) {
        x = x.attr(nc("password"), password);
    }
    let mut message = Message::new(Some(Jid::from(jid.clone())));
    message.type_ = MessageType::Normal;
    message.id = Some(Id(new_id()));
    message.payloads.push(x.build());
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
    if let Some(password) = &settings.password {
        if has("muc#roomconfig_passwordprotectedroom") {
            x = x.append(field(
                "muc#roomconfig_passwordprotectedroom",
                flag(!password.is_empty()),
            ));
        }
        if has("muc#roomconfig_roomsecret") {
            x = x.append(field("muc#roomconfig_roomsecret", password));
        }
    }
    Element::builder("query", NS_MUC_OWNER)
        .append(x.build())
        .build()
}

/// Whether a message is an invitation to a room: mediated (XEP-0045, 7.8.2) or direct
/// (XEP-0249). An archived invitation is old news: it is not a chat message.
pub(crate) fn is_invitation(message: &Message) -> bool {
    message.payloads.iter().any(|x| {
        (x.is("x", NS_MUC_USER)
            && (x.has_child("invite", NS_MUC_USER) || x.has_child("decline", NS_MUC_USER)))
            || x.is("x", NS_CONFERENCE)
    })
}

/// An invitation to a room: mediated (XEP-0045, 7.8.2) or direct (XEP-0249). Emits
/// `ClientEvent::RoomInvite` and returns true. Chord never joins on its own.
fn on_invitation(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(from) = &message.from else {
        return false;
    };
    // The copy of an invitation that another session of ours sent (XEP-0280) is not an
    // invitation for us.
    let ours = from.to_bare() == *ctx.account;
    for x in &message.payloads {
        // The person that we invited said no (XEP-0045, 7.8.2). Only a room that we know can
        // say it.
        if x.is("x", NS_MUC_USER)
            && let Some(decline) = x.get_child("decline", NS_MUC_USER)
            && from.resource().is_none()
            && is_room(ctx, &from.to_bare())
        {
            let who = decline
                .attr("from")
                .and_then(|f| f.parse::<Jid>().ok())
                .map_or_else(|| "Someone".to_owned(), |j| j.to_bare().to_string());
            let reason = decline
                .get_child("reason", NS_MUC_USER)
                .map(Element::text)
                .filter(|r| !r.trim().is_empty())
                .map(|r| format!(": {r}"))
                .unwrap_or_default();
            ctx.emit(ClientEvent::Notice(format!(
                "{who} declined your invitation to {}{reason}",
                from.to_bare()
            )));
            return true;
        }
        if x.is("x", NS_MUC_USER)
            && let Some(invite) = x.get_child("invite", NS_MUC_USER)
        {
            if ours {
                return true;
            }
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
            if ours {
                return true;
            }
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
    if let Command::Card { reply, .. } = command {
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
    if let Command::RoomService { reply } = command {
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
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
    | Command::InviteDirect { reply, .. }
    | Command::Captcha { reply, .. }
    | Command::DeclineInvite { reply, .. }
    | Command::Configure { reply, .. }
    | Command::JoinDefault { reply, .. }
    | Command::SetSubject { reply, .. }
    | Command::SetRole { reply, .. }
    | Command::Destroy { reply, .. }
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
    password::load(ctx, room, room_row(ctx, room).and_then(|row| row.password))
}

/// Whether the bookmark of a room carries its password.
pub(crate) fn password_shared(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    password::is_shared(ctx, room)
}

/// Say whether the bookmark of a room carries its password.
pub(crate) fn set_password_shared(ctx: &Ctx<'_>, room: &BareJid, shared: bool) {
    password::set_shared(ctx, room, shared);
}

/// Store the password of a bookmark that came from the account (see `password::on_bookmark`).
pub(crate) fn on_bookmark_password(ctx: &Ctx<'_>, room: &BareJid, password: Option<&str>) {
    password::on_bookmark(ctx, room, password);
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
    // With a keychain the password goes there, and the column gets a marker.
    let in_keychain = password.is_some_and(|p| password::store_secret(ctx, room, p));
    let password = if in_keychain { None } else { password };
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
    if in_keychain {
        password::mark_in_keychain(ctx, room);
    }
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
pub(crate) fn our_nick(ctx: &Ctx<'_>, room: &BareJid) -> Option<String> {
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
    // A password that the caller gives is stored. One that we load is stored already.
    let given = password.clone();
    let password = password.or_else(|| password::load(ctx, room, row.and_then(|r| r.password)));

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
    ensure_room(ctx, room, Some(&nick), given.as_deref());
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
            ticks: 0,
        },
    );
    mark_room(ctx, room);
}

/// Presence for a room: the presence of the account (caps, avatar hash, show value and
/// status text).
fn room_presence(ctx: &mut Ctx<'_>) -> Presence {
    own_presence::current(ctx)
}

/// Send our changed presence to every room that we are in, to room/nick.
pub(crate) fn send_presence_to_rooms(ctx: &mut Ctx<'_>, presence: &Presence) {
    let rooms: Vec<(BareJid, String)> = ctx
        .state
        .muc
        .nicks
        .iter()
        .map(|(room, nick)| (room.clone(), nick.clone()))
        .collect();
    for (room, nick) in rooms {
        if let Ok(full) = room.with_resource_str(&nick) {
            ctx.send(presence.clone().with_to(Jid::from(full)));
        }
    }
}

/// The XEP-0421 occupant-id in a list of payloads.
pub(crate) fn occupant_id(payloads: &[Element]) -> Option<String> {
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
    rejoin::cancel(ctx, room);
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

/// Send a link with an XEP-0066 out-of-band element, so that clients show the file
/// inline (an "embed"), for example a GIF. The body is the URL too, for clients without
/// XEP-0066. It picks a room or a chat as `send_chat` does, and stores the message.
pub(crate) fn send_link(ctx: &mut Ctx<'_>, to: Jid, url: String) -> Result<String, ClientError> {
    let oob = xmpp_parsers::oob::Oob {
        url: url.clone(),
        desc: None,
    };
    let room = to.to_bare();
    if !is_room(ctx, &room) {
        if to.resource().is_none() && is_muc_service(ctx, &room) {
            return Err(ClientError::Invalid(format!(
                "{room} is a room: join it before you send to it"
            )));
        }
        return Ok(super::chat::send_with_oob(ctx, to, url, Some(oob)));
    }
    let mut out = Outgoing::default();
    out.extras.oob_url = Some(url.clone());
    out.payloads.push(oob.into());
    match to.resource() {
        Some(nick) => send_private_message(ctx, &room, nick.as_str(), url, out),
        None => send_message(ctx, &room, url, out),
    }
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
    health::heard_from(ctx, &room);
    // A room that holds our join asks for a CAPTCHA (XEP-0158).
    if from.resource().is_none() && captcha::on_challenge(ctx, &room, &message.payloads) {
        return true;
    }
    match message.type_ {
        MessageType::Groupchat => {
            if from.resource().is_none() {
                // A message from the room: a configuration change (XEP-0045, 10.2.1).
                let codes = status::codes(&message.payloads);
                status::on_room_codes(ctx, &room, &codes, true);
            }
            if let Some((_, subject)) = message.get_best_subject(vec![]) {
                set_subject(ctx, &room, subject);
                // The room announces our own change: the change is done.
                if from
                    .resource()
                    .is_some_and(|n| is_ours(ctx, &room, n.as_str(), message))
                {
                    let ids: Vec<String> = ctx
                        .state
                        .muc
                        .subjects
                        .iter()
                        .filter(|(_, (r, _))| *r == room)
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in ids {
                        if let Some((_, reply)) = ctx.state.muc.subjects.remove(&id) {
                            let _ = reply.send(Ok(()));
                        }
                    }
                }
            }
            let ids = MessageIds::of(message, &room);
            store(ctx, &room, message, ids, delay_ms(message), true);
        }
        MessageType::Error => {
            let waiting = message
                .id
                .as_ref()
                .and_then(|id| ctx.state.muc.subjects.remove(&id.0));
            if let Some((_, reply)) = waiting {
                let error = message
                    .payloads
                    .iter()
                    .find_map(|p| StanzaError::try_from(p.clone()).ok());
                let _ = reply.send(Err(ClientError::Server(match &error {
                    Some(e) if e.defined_condition == DefinedCondition::Forbidden => {
                        "forbidden: only moderators may change the subject".to_owned()
                    }
                    Some(e) => error_text(e),
                    None => "error".to_owned(),
                })));
            } else if message
                .payloads
                .iter()
                .any(|x| x.is("x", NS_MUC_USER) && x.has_child("invite", NS_MUC_USER))
            {
                // The room refused our invitation, for example when we are not in it.
                let error = message
                    .payloads
                    .iter()
                    .find_map(|p| StanzaError::try_from(p.clone()).ok());
                let sent = message
                    .id
                    .as_ref()
                    .and_then(|id| ctx.state.muc.invites.remove(&id.0));
                let direct_helps = error.as_ref().is_some_and(|e| {
                    matches!(
                        e.defined_condition,
                        DefinedCondition::Forbidden
                            | DefinedCondition::NotAllowed
                            | DefinedCondition::NotAcceptable
                            | DefinedCondition::ServiceUnavailable
                    )
                });
                if let Some(invite) = sent.filter(|_| direct_helps) {
                    // The room does not pass invitations on. The person can still get one
                    // from us (XEP-0249).
                    ctx.emit(ClientEvent::Notice(format!(
                        "The room {room} does not pass on invitations. Chord sent {} a direct invitation",
                        invite.jid
                    )));
                    send_direct_invitation(ctx, &invite.room, &invite.jid, invite.reason);
                } else {
                    let text = error
                        .as_ref()
                        .map_or_else(|| "error".to_owned(), error_text);
                    ctx.emit(ClientEvent::Notice(format!(
                        "The room {room} refused the invitation: {text}"
                    )));
                }
            } else {
                log::warn!("error message from the room {from}");
            }
        }
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
            // XEP-0421: who wrote it, whatever the nick is now or becomes later.
            if let Some(occupant) = occupant_id(&message.payloads) {
                db(
                    ctx,
                    "store the occupant id of a message",
                    ctx.store.conn().execute(
                        "UPDATE messages SET occupant_id = ?2 WHERE id = ?1",
                        params![stored.rowid, occupant],
                    ),
                );
            }
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
    health::heard_from(ctx, &room);
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
    status::muc_user(&presence.payloads)
}

/// The error of a join, or of a later presence.
fn on_error(ctx: &mut Ctx<'_>, room: &BareJid, nick: Option<&str>, presence: &Presence) {
    // The error can carry a CAPTCHA (XEP-0158). The join waits for the answer.
    if captcha::on_challenge(ctx, room, &presence.payloads) {
        return;
    }
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

/// The `destroy` child of the `x` element of a presence: the room is gone (XEP-0045,
/// 10.9). `xmpp-parsers` has no type for it, so this reads the raw element. Gives the
/// reason and the alternate room.
fn destroy_of(presence: &Presence) -> Option<(Option<String>, Option<BareJid>)> {
    let destroy = presence
        .payloads
        .iter()
        .filter(|p| p.is("x", NS_MUC_USER))
        .find_map(|x| x.get_child("destroy", NS_MUC_USER))?;
    let reason = destroy
        .get_child("reason", NS_MUC_USER)
        .map(Element::text)
        .filter(|r| !r.trim().is_empty());
    let alternate = destroy
        .attr("jid")
        .and_then(|j| j.parse::<Jid>().ok())
        .map(|j| j.to_bare());
    Some((reason, alternate))
}

/// The owner destroyed the room. We are out of it for good: stop the session state of the
/// room, retract its bookmark (it would join a room that is gone at each login), and say
/// why. The stored messages stay. Each occupant gets the same presence: report it once.
fn on_destroyed(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    reason: Option<String>,
    alternate: Option<BareJid>,
) {
    if !is_joined_or_joining(ctx, room) {
        return;
    }
    ctx.state.muc.nicks.remove(room);
    rejoin::cancel(ctx, room);
    if let Some(join) = ctx.state.muc.joins.remove(room) {
        for reply in join.replies {
            let _ = reply.send(Err(ClientError::Server("the room was destroyed".into())));
        }
    }
    drop_outbox(ctx, room);
    set_joined(ctx, room, false);
    clear_occupants(ctx, room);
    if bookmarks::is_bookmarked(ctx, room) {
        // Nobody waits for the answer.
        let (reply, _) = oneshot::channel();
        bookmarks::remove(ctx, room.clone(), reply);
    }
    ctx.emit(ClientEvent::RoomDestroyed {
        room: room.clone(),
        reason,
        alternate,
    });
    mark_room(ctx, room);
}

fn on_unavailable(ctx: &mut Ctx<'_>, room: &BareJid, nick: &str, presence: &Presence) {
    if let Some((reason, alternate)) = destroy_of(presence) {
        on_destroyed(ctx, room, reason, alternate);
        return;
    }
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
            let codes = status::codes(&presence.payloads);
            let reason = user
                .as_ref()
                .and_then(|u| u.items.first())
                .and_then(|i| i.reason.as_ref())
                .map(|r| r.0.as_str());
            ctx.emit(ClientEvent::Notice(status::removal_text(
                room, &codes, reason,
            )));
            // A service that shuts down, or fails, comes back: join again after a wait.
            if status::is_temporary(&codes) {
                rejoin::schedule(ctx, room);
            }
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
    let status = presence
        .statuses
        .values()
        .next()
        .filter(|s| !s.trim().is_empty())
        .cloned();
    // XEP-0421: the id of the occupant outlives a nick change, and a nick that another
    // person takes later has another id.
    let occupant = occupant_id(&presence.payloads);
    db(
        ctx,
        "store an occupant",
        ctx.store.conn().execute(
            "INSERT INTO occupants
                (account_id, room, nick, real_jid, affiliation, role, show, occupant_id, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (account_id, room, nick) DO UPDATE SET
                real_jid = excluded.real_jid, affiliation = excluded.affiliation,
                role = excluded.role, show = excluded.show, status = excluded.status,
                occupant_id = COALESCE(excluded.occupant_id, occupant_id)",
            params![
                ctx.account_id,
                room.as_str(),
                nick,
                real_jid,
                affiliation,
                role,
                show,
                occupant,
                status
            ],
        ),
    );
    if !is_self {
        let real = item.and_then(|i| i.jid.as_ref()).map(|jid| jid.to_bare());
        super::avatars::on_occupant(ctx, presence, real);
    }
    if is_self {
        remember_nick(ctx, room, nick);
        if let Some(id) = occupant_id(&presence.payloads) {
            ctx.state.muc.occupant_ids.insert(room.clone(), id);
        }
    }
    if is_self {
        // A status 100 in our own presence says that the room is non-anonymous.
        status::on_room_codes(ctx, room, &status::codes(&presence.payloads), false);
    }
    if is_self && let Some(join) = ctx.state.muc.joins.remove(room) {
        ctx.state.muc.nicks.insert(room.clone(), nick.to_owned());
        rejoin::cancel(ctx, room);
        set_joined(ctx, room, true);
        // The service can give another nick than the one that we asked for (status 210,
        // XEP-0045, 7.2.7). Keep the nick that we have, so that the next join uses it.
        if has(Status::AssignedNick) || join.nick != nick {
            ensure_room(ctx, room, Some(nick), None);
            ctx.emit(ClientEvent::Notice(format!(
                "{room} gave you the nick {nick}"
            )));
        }
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
        DefinedCondition::ItemNotFound => {
            "item-not-found: the room does not exist, or it is locked"
        }
        DefinedCondition::ServiceUnavailable => "service-unavailable: the room is full",
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
mod room_tests;

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
    fn an_occupant_with_a_real_jid_starts_an_avatar_fetch_and_we_do_not_for_ourselves() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let item = Item::new(Affiliation::Member, Role::Participant)
            .with_jid(jid::FullJid::new("bob@chord.localhost/phone").unwrap());
        let bob = occupant_presence("bobby", vec![], item);
        h.with_ctx(|ctx| on_presence(ctx, &bob));
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to().unwrap().as_str(), "bob@chord.localhost");
        // Our own presence starts nothing.
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("alice")));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn occupants_join_change_status_and_leave() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let mut carol =
            occupant_presence("carol", vec![], Item::new(Affiliation::None, Role::Visitor))
                .with_show(Show::Away);
        carol.set_status("", "At lunch");
        h.with_ctx(|ctx| on_presence(ctx, &carol));
        assert_eq!(occupant_nicks(&h), ["alice", "carol"]);
        let show_status = |h: &Harness, nick: &str| -> (Option<String>, Option<String>) {
            h.store
                .conn()
                .query_row(
                    "SELECT show, status FROM occupants WHERE nick = ?1",
                    [nick],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        assert_eq!(
            show_status(&h, "carol"),
            (Some("away".into()), Some("At lunch".into()))
        );
        // A new presence without a status text clears it.
        let back = occupant_presence("carol", vec![], Item::new(Affiliation::None, Role::Visitor));
        h.with_ctx(|ctx| on_presence(ctx, &back));
        assert_eq!(show_status(&h, "carol"), (None, None));

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
    fn send_link_carries_the_url_as_body_and_oob_in_a_room_and_a_chat() {
        use xmpp_parsers::oob::Oob;
        let oob_of = |m: &Message| {
            m.payloads
                .iter()
                .find_map(|p| Oob::try_from(p.clone()).ok())
                .map(|o| o.url)
        };
        let url = "https://static.klipy.com/a/b.gif";
        let mut h = Harness::new();
        joined(&mut h, "alice");
        h.with_ctx(|ctx| send_link(ctx, Jid::from(room()), url.into()))
            .unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = &sent[..] else {
            panic!("{sent:?}");
        };
        assert_eq!(m.type_, MessageType::Groupchat);
        assert_eq!(m.bodies.get("").map(|b| b.as_str()), Some(url));
        assert_eq!(oob_of(m).as_deref(), Some(url));

        h.with_ctx(|ctx| send_link(ctx, jid("bob@example.org"), url.into()))
            .unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = &sent[..] else {
            panic!("{sent:?}");
        };
        assert_eq!(m.type_, MessageType::Chat);
        assert_eq!(oob_of(m).as_deref(), Some(url));
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
        // A chat message is refused by the room.
        assert_eq!(message.type_, MessageType::Normal);
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
    fn an_invitation_that_we_sent_from_another_session_is_no_event() {
        let mut h = Harness::new();
        let direct = from_sender(
            ACCOUNT,
            &format!("<x xmlns='jabber:x:conference' jid='{ROOM}'/>"),
        );
        let mediated = from_sender(
            ACCOUNT,
            &format!("<x xmlns='{NS_MUC_USER}'><invite to='bob@example.org'/></x>"),
        );
        for message in [direct, mediated] {
            h.with_ctx(|ctx| assert!(on_message(ctx, &message)));
        }
        assert!(invites(&h).is_empty());
    }

    #[test]
    fn an_archived_invitation_is_no_chat_message() {
        let mut h = Harness::new();
        let mut message = from_sender(
            ROOM,
            &format!("<x xmlns='{NS_MUC_USER}'><invite from='alice@example.org'/></x>"),
        );
        message.type_ = MessageType::Normal;
        message.bodies.insert(
            xmpp_parsers::message::Lang(String::new()),
            "alice invites you".to_owned(),
        );
        assert!(is_invitation(&message));
        h.with_ctx(|ctx| super::super::chat::store_archived(ctx, &message, "srv-1", Some(5)));
        let rows = messages_with(h.store.conn(), h.account_id, ROOM).unwrap();
        assert!(rows.is_empty());
        let peer = messages_with(h.store.conn(), h.account_id, ACCOUNT).unwrap();
        assert!(peer.is_empty());
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
        assert_eq!(messages[0].type_, MessageType::Normal);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn a_refused_invitation_becomes_a_notice() {
        let mut h = Harness::new();
        let mut refused = Message::new(Some(jid(ACCOUNT)));
        refused.type_ = MessageType::Error;
        refused.from = Some(jid(ROOM));
        refused.payloads.push(
            format!("<x xmlns='{NS_MUC_USER}'><invite to='bob@example.org'/></x>")
                .parse()
                .unwrap(),
        );
        refused.payloads.push(
            StanzaError::new(
                ErrorType::Modify,
                DefinedCondition::NotAcceptable,
                "en",
                "not in the room",
            )
            .into(),
        );
        h.with_ctx(|ctx| {
            ensure_room(ctx, &room(), Some("alice"), None);
            on_message(ctx, &refused)
        });
        assert!(h.effects.iter().any(|e| matches!(e,
            super::super::Effect::Emit(ClientEvent::Notice(n))
                if n.contains("refused the invitation") && n.contains("not-acceptable"))));
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
                password: None,
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

    fn room_info_answer(h: &mut Harness, response: IqResponse) -> Result<RoomCard, ClientError> {
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Card {
                    room: room(),
                    reply,
                },
            )
        });
        let iqs = h.sent_iqs();
        assert_eq!(iqs.len(), 1);
        h.respond(is_muc, response);
        answer.try_recv().unwrap().expect("an answer")
    }

    #[test]
    fn room_info_reads_the_name_the_subject_and_the_occupants() {
        let mut h = Harness::new();
        let query: Element = "<query xmlns='http://jabber.org/protocol/disco#info'>
              <identity category='conference' type='text' name='Dev talk'/>
              <feature var='muc_membersonly'/>
              <x xmlns='jabber:x:data' type='result'>
                <field var='FORM_TYPE' type='hidden'>
                  <value>http://jabber.org/protocol/muc#roominfo</value></field>
                <field var='muc#roominfo_subject'><value>Build day</value></field>
                <field var='muc#roominfo_occupants'><value>7</value></field>
              </x></query>"
            .parse()
            .unwrap();
        let card = room_info_answer(&mut h, IqResponse::Result(Some(query))).unwrap();
        assert_eq!(card.jid, ROOM);
        assert_eq!(card.name.as_deref(), Some("Dev talk"));
        assert_eq!(card.subject.as_deref(), Some("Build day"));
        assert_eq!(card.occupants, Some(7));
        assert!(card.members_only);
        assert!(!card.password_protected);
    }

    #[test]
    fn room_info_works_without_the_room_form() {
        let mut h = Harness::new();
        let query: Element = "<query xmlns='http://jabber.org/protocol/disco#info'>
              <identity category='conference' type='text'/></query>"
            .parse()
            .unwrap();
        let card = room_info_answer(&mut h, IqResponse::Result(Some(query))).unwrap();
        assert_eq!(card.name, None);
        assert_eq!(card.occupants, None);
    }

    #[test]
    fn room_info_refuses_an_address_that_is_not_a_room() {
        let mut h = Harness::new();
        let query: Element = "<query xmlns='http://jabber.org/protocol/disco#info'>
              <identity category='client' type='pc'/></query>"
            .parse()
            .unwrap();
        let result = room_info_answer(&mut h, IqResponse::Result(Some(query)));
        assert!(matches!(result, Err(ClientError::Invalid(_))));
    }

    #[test]
    fn room_info_passes_on_an_error_and_a_lost_session() {
        let mut h = Harness::new();
        let error = IqResponse::Error(StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::ItemNotFound,
            "en",
            "",
        ));
        assert!(matches!(
            room_info_answer(&mut h, error),
            Err(ClientError::Server(_))
        ));
        assert_eq!(
            room_info_answer(&mut h, IqResponse::Lost),
            Err(ClientError::NotConnected)
        );
    }

    #[test]
    fn a_destroyed_room_reports_the_reason_and_the_alternate_and_retracts_the_bookmark() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        h.with_ctx(|ctx| {
            ctx.store
                .conn()
                .execute("UPDATE rooms SET bookmarked = 1, autojoin = 1", [])
                .unwrap();
        });
        let xml = format!(
            "<presence xmlns='jabber:client' from='{ROOM}/alice' type='unavailable'>\
             <x xmlns='{NS_MUC_USER}'><item affiliation='none' role='none'/>\
             <destroy jid='new@rooms.chord.localhost'><reason>Moved away</reason></destroy>\
             </x></presence>"
        );
        let gone = Presence::try_from(xml.parse::<Element>().unwrap()).unwrap();
        h.with_ctx(|ctx| on_presence(ctx, &gone));
        assert!(!joined_flag(&h));
        assert!(occupant_nicks(&h).is_empty());
        assert!(!h.state.muc.nicks.contains_key(&room()));
        let destroyed: Vec<_> = h
            .effects
            .iter()
            .filter_map(|e| match e {
                super::super::Effect::Emit(ClientEvent::RoomDestroyed {
                    room,
                    reason,
                    alternate,
                }) => Some((room.clone(), reason.clone(), alternate.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            destroyed,
            [(
                room(),
                Some("Moved away".to_owned()),
                Some(BareJid::new("new@rooms.chord.localhost").unwrap())
            )]
        );
        // The retract of the bookmark went out.
        let iqs = h.sent_iqs();
        assert!(
            iqs.iter()
                .any(|iq| matches!(iq, Iq::Set { payload, .. } if payload.name() == "pubsub")),
            "{iqs:?}"
        );
        // A second presence of the same destroy (one per occupant) says nothing more.
        h.effects.clear();
        h.with_ctx(|ctx| on_presence(ctx, &gone));
        assert!(!h.effects.iter().any(|e| matches!(
            e,
            super::super::Effect::Emit(ClientEvent::RoomDestroyed { .. })
        )));
    }

    #[test]
    fn a_destroy_without_a_reason_or_an_alternate_is_still_a_destroy() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let xml = format!(
            "<presence xmlns='jabber:client' from='{ROOM}/alice' type='unavailable'>\
             <x xmlns='{NS_MUC_USER}'><item affiliation='none' role='none'/><destroy/></x></presence>"
        );
        let gone = Presence::try_from(xml.parse::<Element>().unwrap()).unwrap();
        h.with_ctx(|ctx| on_presence(ctx, &gone));
        assert!(h.effects.iter().any(|e| matches!(
            e,
            super::super::Effect::Emit(ClientEvent::RoomDestroyed {
                reason: None,
                alternate: None,
                ..
            })
        )));
    }

    #[test]
    fn the_condition_of_a_server_error_is_its_first_word() {
        let cases = [
            (
                "not-authorized: the room needs a password",
                Some("not-authorized"),
            ),
            ("conflict: the nick is in use (taken)", Some("conflict")),
            ("item-not-found", Some("item-not-found")),
            ("empty answer", None),
            ("", None),
        ];
        for (text, expect) in cases {
            assert_eq!(
                ClientError::Server(text.into()).condition(),
                expect,
                "{text}"
            );
        }
        assert_eq!(ClientError::NotConnected.condition(), None);
        let error = StanzaError::new(ErrorType::Auth, DefinedCondition::NotAuthorized, "en", "");
        assert_eq!(
            ClientError::Server(error_text(&error)).condition(),
            Some("not-authorized")
        );
    }

    #[test]
    fn join_with_a_password_sends_it_and_a_wrong_one_fails_with_not_authorized() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Join {
                    room: room(),
                    nick: "alice".into(),
                    password: Some("hunter2".into()),
                    reply,
                },
            )
        });
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        let x = p
            .payloads
            .iter()
            .find(|e| e.is("x", "http://jabber.org/protocol/muc"))
            .expect("the muc element");
        let password = x.get_child("password", "http://jabber.org/protocol/muc");
        assert_eq!(password.map(Element::text).as_deref(), Some("hunter2"));
        let error = Presence::error()
            .with_from(jid(ROOM))
            .with_payload(StanzaError::new(
                ErrorType::Auth,
                DefinedCondition::NotAuthorized,
                "en",
                "",
            ));
        h.with_ctx(|ctx| on_presence(ctx, &error));
        let Some(Err(e)) = answer.try_recv().unwrap() else {
            panic!("expected an error")
        };
        assert_eq!(e.condition(), Some("not-authorized"));
    }

    fn subject_command(h: &mut Harness, subject: &str) -> (Answer, String) {
        let answer = command(h, |reply| Command::SetSubject {
            room: room(),
            subject: subject.into(),
            reply,
        });
        let messages = sent_messages(h);
        let [message] = messages.as_slice() else {
            panic!("{messages:?}")
        };
        assert_eq!(message.type_, MessageType::Groupchat);
        assert_eq!(message.to, Some(jid(ROOM)));
        assert!(message.bodies.is_empty());
        (answer, message.id.clone().expect("an id").0)
    }

    #[test]
    fn set_subject_sends_a_subject_only_message_and_waits_for_the_echo() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let (mut answer, _) = subject_command(&mut h, "Build day");
        let sent_subject = h.state.muc.subjects.len();
        assert_eq!(sent_subject, 1);
        assert!(
            answer.try_recv().unwrap().is_none(),
            "the room has not answered"
        );
        // The room announces the subject from our nick.
        let mut echo = groupchat("alice", "", None, None);
        echo.bodies.clear();
        echo.subjects.insert(
            xmpp_parsers::message::Lang(String::new()),
            "Build day".into(),
        );
        h.with_ctx(|ctx| on_message(ctx, &echo));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(h.state.muc.subjects.is_empty());
    }

    #[test]
    fn a_subject_from_another_occupant_does_not_finish_our_change() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        let (mut answer, _) = subject_command(&mut h, "Mine");
        let mut other = groupchat("bob", "", None, None);
        other.bodies.clear();
        other
            .subjects
            .insert(xmpp_parsers::message::Lang(String::new()), "Theirs".into());
        h.with_ctx(|ctx| on_message(ctx, &other));
        assert!(answer.try_recv().unwrap().is_none());
    }

    #[test]
    fn set_subject_maps_forbidden_to_a_plain_error_and_needs_the_room() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::SetSubject {
            room: room(),
            subject: "x".into(),
            reply,
        });
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        joined(&mut h, "alice");
        let (mut answer, id) = subject_command(&mut h, "Nope");
        let mut refused = Message::new(Some(jid(ROOM)));
        refused.type_ = MessageType::Error;
        refused.from = Some(jid(ROOM));
        refused.id = Some(Id(id));
        refused
            .payloads
            .push(StanzaError::new(ErrorType::Auth, DefinedCondition::Forbidden, "en", "").into());
        h.with_ctx(|ctx| on_message(ctx, &refused));
        let Some(Err(ClientError::Server(text))) = answer.try_recv().unwrap() else {
            panic!("expected a server error")
        };
        assert!(text.starts_with("forbidden"), "{text}");
        assert!(text.contains("moderators"), "{text}");
    }

    #[test]
    fn set_role_sends_the_nick_and_the_role_and_maps_errors() {
        for (role, word) in [
            (RoomRole::None, "none"),
            (RoomRole::Visitor, "visitor"),
            (RoomRole::Participant, "participant"),
            (RoomRole::Moderator, "moderator"),
        ] {
            let mut h = Harness::new();
            let mut answer = command(&mut h, |reply| Command::SetRole {
                room: room(),
                nick: "bob".into(),
                role,
                reason: Some("spam".into()),
                reply,
            });
            let iqs = h.sent_iqs();
            let Some(Iq::Set { to, payload, .. }) = iqs.first() else {
                panic!("{iqs:?}")
            };
            assert_eq!(to.as_ref(), Some(&jid(ROOM)));
            let item = payload.get_child("item", NS_MUC_ADMIN).expect("item");
            assert_eq!(item.attr("nick"), Some("bob"));
            assert_eq!(item.attr("role"), Some(word));
            assert_eq!(item.attr("jid"), None);
            assert_eq!(
                item.get_child("reason", NS_MUC_ADMIN).unwrap().text(),
                "spam"
            );
            h.answer(is_muc, None);
            assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        }
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::SetRole {
            room: room(),
            nick: "bob".into(),
            role: RoomRole::None,
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
    fn destroy_room_sends_the_reason_and_the_alternate() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::Destroy {
            room: room(),
            reason: Some("Moved".into()),
            alternate: Some(BareJid::new("new@rooms.chord.localhost").unwrap()),
            reply,
        });
        let iqs = h.sent_iqs();
        let Some(Iq::Set { to, payload, .. }) = iqs.first() else {
            panic!("{iqs:?}")
        };
        assert_eq!(to.as_ref(), Some(&jid(ROOM)));
        assert!(payload.is("query", NS_MUC_OWNER));
        let destroy = payload.get_child("destroy", NS_MUC_OWNER).expect("destroy");
        assert_eq!(destroy.attr("jid"), Some("new@rooms.chord.localhost"));
        assert_eq!(
            destroy.get_child("reason", NS_MUC_OWNER).unwrap().text(),
            "Moved"
        );
        h.answer(is_muc, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    fn join_default(h: &mut Harness) -> Answer {
        command(h, |reply| Command::JoinDefault {
            room: room(),
            password: None,
            fallback: None,
            reply,
        })
    }

    #[test]
    fn a_first_join_asks_for_the_reserved_nick_and_uses_it() {
        let mut h = Harness::new();
        let mut answer = join_default(&mut h);
        let iqs = h.sent_iqs();
        let Some(Iq::Get { to, payload, .. }) = iqs.first() else {
            panic!("{iqs:?}")
        };
        assert_eq!(to.as_ref(), Some(&jid(ROOM)));
        assert!(payload.is("query", NS_DISCO_INFO));
        assert_eq!(payload.attr("node"), Some("x-roomuser-item"));
        // A second join call waits for the same answer.
        let mut second = join_default(&mut h);
        assert!(h.take_sent().is_empty());
        let reserved: Element = format!(
            "<query xmlns='{NS_DISCO_INFO}' node='x-roomuser-item'>\
             <identity category='conference' name='Alice Reserved' type='text'/></query>"
        )
        .parse()
        .unwrap();
        h.answer(is_muc, Some(reserved));
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/Alice Reserved"))));
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("Alice Reserved")));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(second.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn without_a_reserved_nick_the_join_uses_the_local_part_of_the_account() {
        let mut h = Harness::new();
        let _answer = join_default(&mut h);
        h.take_sent();
        let error = IqResponse::Error(StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::ItemNotFound,
            "en",
            "",
        ));
        h.respond(is_muc, error);
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        let node = BareJid::new(ACCOUNT).unwrap().node().unwrap().to_string();
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/{node}"))));
    }

    #[test]
    fn the_fallback_nick_beats_the_local_part_when_the_room_reserved_none() {
        let mut h = Harness::new();
        let mut answer = command(&mut h, |reply| Command::JoinDefault {
            room: room(),
            password: None,
            fallback: Some("Display Name".into()),
            reply,
        });
        h.take_sent();
        h.respond(
            is_muc,
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ItemNotFound,
                "en",
                "",
            )),
        );
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/Display Name"))));
        h.with_ctx(|ctx| on_presence(ctx, &self_presence("Display Name")));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn a_room_with_a_stored_nick_joins_without_asking() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("stored"), None));
        let _answer = join_default(&mut h);
        let sent = h.take_sent();
        let [Stanza::Presence(p)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(p.to, Some(jid(&format!("{ROOM}/stored"))));
    }

    #[test]
    fn room_info_reads_whether_occupants_may_change_the_subject() {
        let mut h = Harness::new();
        let query = |value: &str| -> Element {
            format!(
                "<query xmlns='{NS_DISCO_INFO}'>\
                 <identity category='conference' type='text' name='Dev'/>\
                 <x xmlns='jabber:x:data' type='result'>\
                 <field var='FORM_TYPE' type='hidden'><value>{NS_ROOM_INFO}</value></field>\
                 <field var='muc#roominfo_changesubject'><value>{value}</value></field>\
                 </x></query>"
            )
            .parse()
            .unwrap()
        };
        let card = room_info_answer(&mut h, IqResponse::Result(Some(query("1")))).unwrap();
        assert!(card.change_subject);
        let card = room_info_answer(&mut h, IqResponse::Result(Some(query("0")))).unwrap();
        assert!(!card.change_subject);
    }

    #[test]
    fn configure_room_sets_and_clears_the_password() {
        for (password, flag) in [("hunter2", "1"), ("", "0")] {
            let mut h = Harness::new();
            let mut answer = command(&mut h, |reply| Command::Configure {
                room: room(),
                settings: RoomSettings {
                    password: Some(password.into()),
                    ..RoomSettings::default()
                },
                reply,
            });
            h.sent_iqs();
            let form: Element = format!(
                "<query xmlns='{NS_MUC_OWNER}'><x xmlns='jabber:x:data' type='form'>\
                 <field var='muc#roomconfig_passwordprotectedroom' type='boolean'/>\
                 <field var='muc#roomconfig_roomsecret' type='text-private'/></x></query>"
            )
            .parse()
            .unwrap();
            h.answer(is_muc, Some(form));
            let iqs = h.sent_iqs();
            let Some(Iq::Set { payload, .. }) = iqs.first() else {
                panic!("{iqs:?}")
            };
            let x = payload.get_child("x", NS_DATA).unwrap();
            let value = |var: &str| {
                x.children()
                    .find(|f| f.attr("var") == Some(var))
                    .map(|f| f.get_child("value", NS_DATA).unwrap().text())
            };
            assert_eq!(
                value("muc#roomconfig_passwordprotectedroom").as_deref(),
                Some(flag)
            );
            assert_eq!(
                value("muc#roomconfig_roomsecret").as_deref(),
                Some(password)
            );
            h.answer(is_muc, None);
            assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        }
    }
}
