//! Records and enums that mirror the public data types of `chord-core`.
//!
//! JIDs cross the boundary as strings. Each type has a `From` impl from the core type.

use chord_core::actor as core_actor;
use chord_core::features::avatars::Avatar as CoreAvatar;
use chord_core::features::notify as core_notify;
use chord_core::features::push::PushRegistration as CorePushRegistration;
use chord_core::features::roster::{Contact as CoreContact, Subscription as CoreSubscription};
use chord_core::features::spaces::{JoinOutcome as CoreJoinOutcome, SpaceInfo as CoreSpaceInfo};
use chord_core::session::ConnectError;
use chord_core::store::queries as core_queries;
use chord_core::views as core_views;

// ---- Views ----

/// One message, ready to show. `id` is the "m:<row id>" id that commands take.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TimelineItem {
    pub id: String,
    /// The stanza-id from the server or the room, if known.
    pub stanza_id: Option<String>,
    /// The XEP-0359 origin-id, if the message has one.
    pub origin_id: Option<String>,
    pub sender: String,
    pub sender_name: String,
    pub avatar: Option<String>,
    pub body: String,
    /// Unix time in ms.
    pub timestamp: i64,
    pub outgoing: bool,
    pub same_sender_as_previous: bool,
    pub edited: bool,
    pub retracted: bool,
    pub reactions: Vec<ReactionSummary>,
    pub reply_to: Option<ReplyPreview>,
    pub attachment: Option<String>,
    pub status: DeliveryStatus,
}

/// The reactions with one emoji.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

/// A short view of the message that a reply quotes.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReplyPreview {
    pub id: Option<String>,
    pub sender_name: String,
    pub body: String,
}

/// How far an outgoing message got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DeliveryStatus {
    Sent,
    Received,
    Displayed,
}

/// The kind of a channel.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChannelKind {
    Direct,
    Room,
    /// Private messages with a room occupant. The `jid` of the item is `room/nick`.
    PrivateMessage {
        room: String,
        nick: String,
    },
}

/// Which channels a list shows.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChannelScope {
    Home,
    Space { service: String, node: String },
}

/// A room or a direct chat in the channel list.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ChannelItem {
    pub jid: String,
    pub name: String,
    pub kind: ChannelKind,
    pub category: Option<String>,
    pub joined: bool,
    pub last_activity: Option<i64>,
    pub unread: u32,
}

/// A space in the space rail.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpaceItem {
    pub service: String,
    pub node: String,
    pub name: String,
    pub avatar: Option<String>,
}

/// A member of a room, or one side of a direct chat.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MemberItem {
    pub id: String,
    pub name: String,
    pub jid: Option<String>,
    pub role: String,
    pub affiliation: String,
    pub show: Option<String>,
    pub online: bool,
    pub avatar: Option<String>,
}

impl From<core_views::TimelineItem> for TimelineItem {
    fn from(i: core_views::TimelineItem) -> Self {
        Self {
            id: i.id,
            stanza_id: i.stanza_id,
            origin_id: i.origin_id,
            sender: i.sender,
            sender_name: i.sender_name,
            avatar: i.avatar,
            body: i.body,
            timestamp: i.timestamp,
            outgoing: i.outgoing,
            same_sender_as_previous: i.same_sender_as_previous,
            edited: i.edited,
            retracted: i.retracted,
            reactions: i.reactions.into_iter().map(Into::into).collect(),
            reply_to: i.reply_to.map(Into::into),
            attachment: i.attachment,
            status: i.status.into(),
        }
    }
}

impl From<core_views::ReactionSummary> for ReactionSummary {
    fn from(r: core_views::ReactionSummary) -> Self {
        Self {
            emoji: r.emoji,
            count: r.count,
            mine: r.mine,
        }
    }
}

impl From<core_views::ReplyPreview> for ReplyPreview {
    fn from(r: core_views::ReplyPreview) -> Self {
        Self {
            id: r.id,
            sender_name: r.sender_name,
            body: r.body,
        }
    }
}

impl From<core_views::DeliveryStatus> for DeliveryStatus {
    fn from(s: core_views::DeliveryStatus) -> Self {
        match s {
            core_views::DeliveryStatus::Sent => Self::Sent,
            core_views::DeliveryStatus::Received => Self::Received,
            core_views::DeliveryStatus::Displayed => Self::Displayed,
        }
    }
}

impl From<core_views::ChannelKind> for ChannelKind {
    fn from(k: core_views::ChannelKind) -> Self {
        match k {
            core_views::ChannelKind::Direct => Self::Direct,
            core_views::ChannelKind::Room => Self::Room,
            core_views::ChannelKind::PrivateMessage { room, nick } => {
                Self::PrivateMessage { room, nick }
            }
        }
    }
}

impl From<ChannelScope> for core_views::ChannelScope {
    fn from(s: ChannelScope) -> Self {
        match s {
            ChannelScope::Home => Self::Home,
            ChannelScope::Space { service, node } => Self::Space { service, node },
        }
    }
}

impl From<core_views::ChannelItem> for ChannelItem {
    fn from(i: core_views::ChannelItem) -> Self {
        Self {
            jid: i.jid,
            name: i.name,
            kind: i.kind.into(),
            category: i.category,
            joined: i.joined,
            last_activity: i.last_activity,
            unread: i.unread,
        }
    }
}

impl From<core_views::SpaceItem> for SpaceItem {
    fn from(i: core_views::SpaceItem) -> Self {
        Self {
            service: i.service,
            node: i.node,
            name: i.name,
            avatar: i.avatar,
        }
    }
}

impl From<core_views::MemberItem> for MemberItem {
    fn from(i: core_views::MemberItem) -> Self {
        Self {
            id: i.id,
            name: i.name,
            jid: i.jid,
            role: i.role,
            affiliation: i.affiliation,
            show: i.show,
            online: i.online,
            avatar: i.avatar,
        }
    }
}

// ---- Diffs ----

/// Define the diff enum of one item type, and its conversion from the core `ListDiff`.
macro_rules! diff_type {
    ($(#[$meta:meta])* $name:ident, $item:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, uniffi::Enum)]
        pub enum $name {
            Insert { index: u32, item: $item },
            Update { index: u32, item: $item },
            Remove { index: u32 },
            /// Replace the whole list.
            Reset { items: Vec<$item> },
        }

        impl From<core_views::ListDiff<core_views::$item>> for $name {
            fn from(d: core_views::ListDiff<core_views::$item>) -> Self {
                match d {
                    core_views::ListDiff::Insert { index, item } => Self::Insert {
                        index: index as u32,
                        item: item.into(),
                    },
                    core_views::ListDiff::Update { index, item } => Self::Update {
                        index: index as u32,
                        item: item.into(),
                    },
                    core_views::ListDiff::Remove { index } => Self::Remove {
                        index: index as u32,
                    },
                    core_views::ListDiff::Reset(items) => Self::Reset {
                        items: items.into_iter().map(Into::into).collect(),
                    },
                }
            }
        }
    };
}

diff_type!(
    /// A change to the timeline list.
    TimelineDiff, TimelineItem
);
diff_type!(
    /// A change to the channel list.
    ChannelDiff, ChannelItem
);
diff_type!(
    /// A change to the space list.
    SpaceDiff, SpaceItem
);
diff_type!(
    /// A change to the member list.
    MemberDiff, MemberItem
);

// ---- Events ----

/// Why a login failed or a session stopped.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ConnectFailure {
    AuthFailed { detail: String },
    Unreachable { detail: String },
    TlsInvalid { detail: String },
    Timeout,
}

impl From<ConnectError> for ConnectFailure {
    fn from(e: ConnectError) -> Self {
        match e {
            ConnectError::AuthFailed(f) => Self::AuthFailed {
                detail: f.to_string(),
            },
            ConnectError::Unreachable(detail) => Self::Unreachable { detail },
            ConnectError::TlsInvalid(detail) => Self::TlsInvalid { detail },
            ConnectError::Timeout => Self::Timeout,
        }
    }
}

/// The connection state, as a UI shows it.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ConnectionState {
    Connecting,
    Connected {
        bound_jid: String,
        resumed: bool,
    },
    /// The connection is lost. The session reconnects by itself.
    Suspended,
    /// The first login failed.
    LoginFailed {
        failure: ConnectFailure,
    },
    /// The server rejected the credentials on a reconnect. Ask for new credentials.
    AuthFailed {
        detail: String,
    },
    Disconnected,
}

impl From<core_actor::ConnectionState> for ConnectionState {
    fn from(s: core_actor::ConnectionState) -> Self {
        use core_actor::ConnectionState as C;
        match s {
            C::Connecting => Self::Connecting,
            C::Connected { bound_jid, resumed } => Self::Connected {
                bound_jid: bound_jid.to_string(),
                resumed,
            },
            C::Suspended => Self::Suspended,
            C::LoginFailed(e) => Self::LoginFailed { failure: e.into() },
            C::AuthFailed(f) => Self::AuthFailed {
                detail: f.to_string(),
            },
            C::Disconnected => Self::Disconnected,
        }
    }
}

/// The kind of a stored message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MessageKind {
    Chat,
    Groupchat,
}

/// Which id keys a stored message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum KeyKind {
    StanzaId,
    OriginId,
}

/// The direction of a stored message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Direction {
    In,
    Out,
}

/// A chat message, as stored.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StoredMessage {
    pub rowid: i64,
    pub kind: MessageKind,
    pub key_kind: KeyKind,
    pub key: String,
    pub direction: Direction,
    pub peer: String,
    pub sender: String,
    pub body: String,
    /// Unix time in ms.
    pub timestamp: i64,
}

impl From<core_queries::StoredMessage> for StoredMessage {
    fn from(m: core_queries::StoredMessage) -> Self {
        Self {
            rowid: m.rowid,
            kind: match m.kind {
                core_queries::MessageKind::Chat => MessageKind::Chat,
                core_queries::MessageKind::Groupchat => MessageKind::Groupchat,
            },
            key_kind: match m.key_kind {
                core_queries::KeyKind::StanzaId => KeyKind::StanzaId,
                core_queries::KeyKind::OriginId => KeyKind::OriginId,
            },
            key: m.key,
            direction: match m.direction {
                core_queries::Direction::In => Direction::In,
                core_queries::Direction::Out => Direction::Out,
            },
            peer: m.peer,
            sender: m.sender,
            body: m.body,
            timestamp: m.timestamp,
        }
    }
}

/// An event from the client.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ClientEvent {
    ConnectionState {
        state: ConnectionState,
    },
    /// A new live chat message arrived. Archive messages reach the views only.
    MessageReceived {
        message: StoredMessage,
    },
    /// A feature has a notice for the user, for example a failed room join.
    Notice {
        text: String,
    },
    /// A contact asks to see our presence. Call `approve_subscription` or `deny_subscription`.
    SubscriptionRequest {
        jid: String,
    },
    /// A live message that should notify the user.
    Notification {
        notification: Notification,
    },
    /// An event that this binding version does not know.
    Unknown,
}

/// How much a chat, room, or private chat may notify.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NotificationLevel {
    All,
    Mentions,
    None,
}

impl From<core_notify::NotificationLevel> for NotificationLevel {
    fn from(l: core_notify::NotificationLevel) -> Self {
        match l {
            core_notify::NotificationLevel::All => Self::All,
            core_notify::NotificationLevel::Mentions => Self::Mentions,
            core_notify::NotificationLevel::None => Self::None,
        }
    }
}

impl From<NotificationLevel> for core_notify::NotificationLevel {
    fn from(l: NotificationLevel) -> Self {
        match l {
            NotificationLevel::All => Self::All,
            NotificationLevel::Mentions => Self::Mentions,
            NotificationLevel::None => Self::None,
        }
    }
}

/// The notification level of a peer, and the end of its mute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NotificationSetting {
    pub level: NotificationLevel,
    /// Unix time in ms.
    pub mute_until: Option<i64>,
}

impl From<core_notify::NotificationSetting> for NotificationSetting {
    fn from(s: core_notify::NotificationSetting) -> Self {
        Self {
            level: s.level.into(),
            mute_until: s.mute_until,
        }
    }
}

/// A message that should notify the user.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Notification {
    pub peer: String,
    pub room: Option<String>,
    pub sender: String,
    pub sender_name: String,
    pub body_preview: String,
    pub mention: bool,
    pub item_id: String,
}

impl From<core_notify::Notification> for Notification {
    fn from(n: core_notify::Notification) -> Self {
        Self {
            peer: n.peer,
            room: n.room.map(|r| r.to_string()),
            sender: n.sender,
            sender_name: n.sender_name,
            body_preview: n.body_preview,
            mention: n.mention,
            item_id: n.item_id,
        }
    }
}

impl From<core_actor::ClientEvent> for ClientEvent {
    fn from(e: core_actor::ClientEvent) -> Self {
        use core_actor::ClientEvent as E;
        match e {
            E::ConnectionState(state) => Self::ConnectionState {
                state: state.into(),
            },
            E::MessageReceived(m) => Self::MessageReceived { message: m.into() },
            E::Notice(text) => Self::Notice { text },
            E::SubscriptionRequest(jid) => Self::SubscriptionRequest {
                jid: jid.to_string(),
            },
            E::Notification(n) => Self::Notification {
                notification: n.into(),
            },
            // ClientEvent is non_exhaustive.
            _ => Self::Unknown,
        }
    }
}

// ---- Command data ----

/// Our subscription state with a contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SubscriptionState {
    None,
    To,
    From,
    Both,
}

impl From<CoreSubscription> for SubscriptionState {
    fn from(s: CoreSubscription) -> Self {
        match s {
            CoreSubscription::None => Self::None,
            CoreSubscription::To => Self::To,
            CoreSubscription::From => Self::From,
            CoreSubscription::Both => Self::Both,
        }
    }
}

/// A contact of the roster.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Contact {
    pub jid: String,
    pub name: Option<String>,
    pub subscription: SubscriptionState,
    /// We asked for a subscription and wait for the answer.
    pub ask: bool,
    pub groups: Vec<String>,
    /// We pre-approved the subscription request of this contact.
    pub approved: bool,
}

impl From<CoreContact> for Contact {
    fn from(c: CoreContact) -> Self {
        Self {
            jid: c.jid.to_string(),
            name: c.name,
            subscription: c.subscription.into(),
            ask: c.ask,
            groups: c.groups,
            approved: c.approved,
        }
    }
}

/// A space that `browse_spaces` found.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpaceInfo {
    pub service: String,
    pub node: String,
    pub name: String,
    pub description: Option<String>,
    pub access_model: Option<String>,
}

impl From<CoreSpaceInfo> for SpaceInfo {
    fn from(s: CoreSpaceInfo) -> Self {
        Self {
            service: s.service,
            node: s.node,
            name: s.name,
            description: s.description,
            access_model: s.access_model,
        }
    }
}

/// The result of `join_space`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum JoinOutcome {
    Joined,
    /// The service holds a join request for the owner to approve.
    Pending,
}

impl From<CoreJoinOutcome> for JoinOutcome {
    fn from(o: CoreJoinOutcome) -> Self {
        match o {
            CoreJoinOutcome::Joined => Self::Joined,
            CoreJoinOutcome::Pending => Self::Pending,
        }
    }
}

/// The address of a space: its pubsub service and node.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SpaceRef {
    pub service: String,
    pub node: String,
}

/// A push service that the account enabled.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PushRegistration {
    pub service: String,
    pub node: String,
}

impl From<CorePushRegistration> for PushRegistration {
    fn from(p: CorePushRegistration) -> Self {
        Self {
            service: p.service,
            node: p.node,
        }
    }
}

/// One publish option of `enable_push`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FormField {
    pub name: String,
    pub value: String,
}

/// The avatar of one owner, as stored.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Avatar {
    /// SHA-1 of the image in lower case hex.
    pub hash: String,
    pub mime: Option<String>,
    /// `None` until the image has arrived.
    pub data: Option<Vec<u8>>,
}

impl From<CoreAvatar> for Avatar {
    fn from(a: CoreAvatar) -> Self {
        Self {
            hash: a.hash,
            mime: a.mime,
            data: a.data,
        }
    }
}

#[cfg(test)]
mod tests {
    use chord_core::jid::{BareJid, Jid};
    use chord_core::session::AuthFailure;

    use super::*;

    fn item(id: &str) -> core_views::ChannelItem {
        core_views::ChannelItem {
            jid: id.into(),
            name: "n".into(),
            kind: core_views::ChannelKind::Room,
            category: None,
            joined: true,
            last_activity: Some(5),
            unread: 3,
        }
    }

    #[test]
    fn channel_item_converts() {
        let c: ChannelItem = item("room@muc.example").into();
        assert_eq!(c.kind, ChannelKind::Room);
        assert_eq!((c.unread, c.last_activity), (3, Some(5)));
    }

    #[test]
    fn diffs_convert_with_u32_indexes() {
        let d: ChannelDiff = core_views::ListDiff::Insert {
            index: 2,
            item: item("a@b"),
        }
        .into();
        assert!(matches!(d, ChannelDiff::Insert { index: 2, .. }));
        let d: ChannelDiff = core_views::ListDiff::Remove { index: 1 }.into();
        assert_eq!(d, ChannelDiff::Remove { index: 1 });
        let d: ChannelDiff = core_views::ListDiff::Reset(vec![item("a@b")]).into();
        assert!(matches!(d, ChannelDiff::Reset { items } if items.len() == 1));
    }

    #[test]
    fn timeline_item_converts_nested_records() {
        let core = core_views::TimelineItem {
            id: "m:1".into(),
            stanza_id: None,
            origin_id: Some("1".into()),
            sender: "a@b/c".into(),
            sender_name: "c".into(),
            avatar: None,
            body: "hi".into(),
            timestamp: 9,
            outgoing: true,
            same_sender_as_previous: false,
            edited: true,
            retracted: false,
            reactions: vec![core_views::ReactionSummary {
                emoji: "x".into(),
                count: 2,
                mine: true,
            }],
            reply_to: Some(core_views::ReplyPreview {
                id: None,
                sender_name: "d".into(),
                body: "q".into(),
            }),
            attachment: None,
            status: core_views::DeliveryStatus::Displayed,
        };
        let t: TimelineItem = core.into();
        assert_eq!(t.status, DeliveryStatus::Displayed);
        assert_eq!(t.reactions[0].count, 2);
        assert_eq!(t.reply_to.unwrap().sender_name, "d");
    }

    #[test]
    fn events_convert() {
        let jid: Jid = "a@b/c".parse().unwrap();
        let e: ClientEvent =
            core_actor::ClientEvent::ConnectionState(core_actor::ConnectionState::Connected {
                bound_jid: jid,
                resumed: true,
            })
            .into();
        assert_eq!(
            e,
            ClientEvent::ConnectionState {
                state: ConnectionState::Connected {
                    bound_jid: "a@b/c".into(),
                    resumed: true
                }
            }
        );
        let bare: BareJid = "x@y".parse().unwrap();
        let e: ClientEvent = core_actor::ClientEvent::SubscriptionRequest(bare).into();
        assert_eq!(e, ClientEvent::SubscriptionRequest { jid: "x@y".into() });
        let e: ClientEvent = core_actor::ClientEvent::ConnectionState(
            core_actor::ConnectionState::AuthFailed(AuthFailure::NoMechanism),
        )
        .into();
        assert!(matches!(
            e,
            ClientEvent::ConnectionState {
                state: ConnectionState::AuthFailed { .. }
            }
        ));
    }

    #[test]
    fn contact_converts() {
        let c: Contact = CoreContact {
            jid: "a@b".parse().unwrap(),
            name: Some("A".into()),
            subscription: CoreSubscription::Both,
            ask: false,
            groups: vec!["g".into()],
            approved: true,
        }
        .into();
        assert_eq!(c.jid, "a@b");
        assert_eq!(c.subscription, SubscriptionState::Both);
    }
}
