//! Server-side spaces (XEP-0503): the space list and the channels of each space.
//!
//! A space is a pubsub node (XEP-0060) with `pubsub#type` `urn:xmpp:spaces:0`. The
//! tables `spaces` and `space_items` keep the spaces that we follow. The views read them.
//!
//! - Start: the pubsub service comes from the disco state. The answers of disco arrive
//!   after `on_connected`, so `disco.rs` calls `on_disco_complete` when the last service
//!   answered. `on_connected` also starts at once if disco is complete already.
//! - Start does three things. It logs each XEP-0503 feature that the service lacks. It asks
//!   for our subscriptions on the service. For each subscribed space node it asks for
//!   the disco#info (name, description, access model) and for the items. A space that we
//!   joined with an xmpp: link can live on another server, so start asks the same of each
//!   distinct service in the `spaces` table.
//! - A refresh error: `item-not-found` for a node means that the node is gone. We drop the
//!   space and tell the user. Any other error (for example `forbidden`) keeps the space
//!   and tells the user the error.
//! - Items: a room is an XEP-0402 `<conference/>` item. The item id is the room JID. Other
//!   items keep their XML in `space_items.payload`. The avatar item goes to `avatars`.
//! - Events: `on_event` keeps the tables in sync. It drops each event that does not come
//!   from the service JID of a space that we follow.
//! - Browse: `browse_spaces` pages the disco#items of the service with RSM (XEP-0059) when
//!   the service advertises it. It stops at `MAX_BROWSE_NODES` nodes, and it keeps at most
//!   `BROWSE_WINDOW` disco#info queries in flight. If the service advertises XEP-0462
//!   (`urn:xmpp:pubsub-filter:0`), the query asks only for nodes of type `urn:xmpp:spaces:0`.
//!   If it advertises XEP-0499 (`urn:xmpp:pubsub-ext-disco:0`), the query asks for the node
//!   metadata in the items, so the browse needs no disco#info per node. An item without
//!   metadata still gets a disco#info query. If the first request fails while it uses one of
//!   these extensions, the browse starts again with a plain request.
//! - Avatar: the avatar item holds XEP-0084 metadata. We store its hash and type in `avatars`
//!   with the owner `service/node`. If the info has no URL, we fetch the image from the
//!   `urn:xmpp:avatar:data` node of the service. An image at a URL goes through
//!   `Effect::Download`. `store_avatar_image` also takes an image from a caller.
//! - Join approval: `join_space` on an `authorize` node returns `Pending`. We store the row
//!   with `subscribed = 2`. The views read `subscribed = 1` only, so the space stays hidden.
//!   The service later sends a subscription notification (XEP-0060, 8.x): `subscribed` turns
//!   the row into a space and `none` drops it. Both send a `ClientEvent::Notice`. At start,
//!   the subscriptions list does the same for an answer that came while we were offline.
//!   `pending_space_joins` lists the rows. As owner, we get a `subscribe_authorization`
//!   form (`on_authorization`, a `Notice`). The service sends it to the sessions that are
//!   online at that time, so `space_join_requests` first runs the get-pending command
//!   (XEP-0060, 8.7): the service sends each form again, to this session. Then it merges the
//!   stored forms with the owner subscriptions request (8.8.1). `approve_space_join` and
//!   `deny_space_join` answer with the form (8.6).
//! - Private spaces use the `whitelist` access model. The owner makes a member with
//!   `add_space_member`, then the member calls `join_space`. Join requests (`authorize`)
//!   only work where a server offers them. Prosody 13 does not.
//! - Room access: a room of a space can be members-only. A person that the owner accepts
//!   must also be a member of each room, or the room refuses the join. `approve_space_join`
//!   and `add_space_member` set the room affiliation `member` in each room of the space.
//!   It works where we own the rooms. Other rooms refuse, and we ignore that.
//!   `add_room_to_space` also tells the room about its space (XEP-0503): it reads the room
//!   config form, and if the form has `muc#roomconfig_pubsub` it sets the field to the
//!   space node URI. A room that refuses, or that has no such field, stays as it is.
//!   `add_room_to_space` does the same for a new room: it reads the affiliations and the
//!   subscriptions of the node, and it grants each member except the owner.
//! - Subscription ids (XEP-0060, 6.2.1): the subscribe answer, the subscriptions list and the
//!   subscription event give a `subid`. We keep it in `spaces.subid`, and `leave_space` sends it
//!   with the unsubscribe. An authorization answer (8.6) names the `pubsub#subid` of the
//!   request when we know it: from the request form, or from the owner query. A message has
//!   no answer, so a request that the owner answered stays, hidden, until the service sends the
//!   subscription event. If the owner query lists it as pending again, it is open again.
//! - Members (XEP-0060, 8.9): `space_members` lists the affiliations of the node. The owner
//!   sees them all. `remove_space_member` sets `none`, and `ban_space_member` sets `outcast`.
//!   Both also end the subscription. The rooms of the space keep their own member lists.
//! - Settings of the owner: `space_config` reads the node configuration form. `configure_space`
//!   submits the name and the description (`pubsub#title`, `pubsub#description`).
//!   `set_space_avatar` and `set_space_banner` upload the image (XEP-0363, `upload.rs`, with no
//!   chat message), then publish XEP-0084 metadata with the GET URL in the avatar item
//!   (`AVATAR_ITEM`) or the banner item (`BANNER_ITEM`). The service decides how long the URL
//!   lives: XEP-0363 has no way to ask for a permanent file. The banner is stored as an item
//!   and not shown yet.
//! - New spaces ask for `pubsub#publish_model` `publishers` (XEP-0503 says not `open`) and
//!   can have a `pubsub#description`.
//! - Items: a refresh asks for pages of `ITEMS_PAGE` items with RSM (XEP-0059) until the last
//!   page, and it replaces the stored items only then. A service without RSM sends all items
//!   at once. The card asks for `max_items` and counts the rooms of the newest ones.
//! - Nodes that are no space: a node of the subscriptions list whose disco#info has a
//!   `pubsub#type` other than a space goes into `non_space_nodes`. The next start skips it, so
//!   a service with many nodes (for example Movim comments) costs no disco#info for each.
//!   A node that we do not subscribe to any more leaves the table. A join of the node by the
//!   user, where the node is a space, also clears the row.
//! - XEP-0330 items: an item with a `urn:xmpp:pubsub:subscription:0` payload names a pubsub
//!   node in the space. Chord shows rooms only, so it ignores these items on purpose. It does
//!   not store them, and a room count does not include them.

use std::collections::{HashMap, HashSet, VecDeque};

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{Connection, params};
use xmpp_parsers::avatar::Data as AvatarData;
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::disco::{DiscoInfoQuery, DiscoInfoResult, DiscoItemsQuery};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::ns;
use xmpp_parsers::pubsub::Subscription;
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::rsm::{SetQuery, SetResult};
use xmpp_parsers::stanza_error::StanzaError;

use super::avatars::{self, MAX_AVATAR_BYTES};
use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, adhoc, new_id, pubsub};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::store::Store;
use crate::views::{ChannelScope, ViewKey};

/// The `pubsub#type` of a space node.
pub const NS_SPACES: &str = "urn:xmpp:spaces:0";
/// The item id of the space avatar.
pub const AVATAR_ITEM: &str = "urn:xmpp:spaces:avatar:metadata:0";
/// The item id of the space banner.
pub const BANNER_ITEM: &str = "urn:xmpp:spaces:banner:metadata:0";
const NS_AVATAR_METADATA: &str = "urn:xmpp:avatar:metadata";
const NS_META: &str = "http://jabber.org/protocol/pubsub#meta-data";
const FEATURE_PREFIX: &str = "http://jabber.org/protocol/pubsub#";
/// The features that XEP-0503 requires from the pubsub service.
const REQUIRED_FEATURES: &[&str] = &[
    "subscribe",
    "create-nodes",
    "delete-nodes",
    "config-node",
    "meta-data",
    "delete-items",
    "retract-items",
    "multi-items",
    "item-ids",
    "manage-subscriptions",
    "retrieve-items",
];
/// The most nodes that `browse_spaces` reads from a service. It ignores the rest.
pub const MAX_BROWSE_NODES: usize = 1000;
/// The page size that `browse_spaces` asks for with RSM. A service may send less.
const PAGE_SIZE: usize = 100;
/// The most disco#info queries of one browse that wait for an answer at the same time.
const BROWSE_WINDOW: usize = 20;
const FEATURE_RSM: &str = "http://jabber.org/protocol/rsm";
/// XEP-0462: filter the nodes of a disco#items query by `pubsub#type`.
const NS_TYPE_FILTER: &str = "urn:xmpp:pubsub-filter:0";
/// XEP-0499: node metadata in the disco#items result. The XEP text names the feature
/// with and without the `:0`, so we accept both.
const NS_EXT_DISCO: &str = "urn:xmpp:pubsub-ext-disco:0";
const NS_EXT_DISCO_SHORT: &str = "urn:xmpp:pubsub-ext-disco";
const NS_AVATAR_DATA: &str = "urn:xmpp:avatar:data";
/// The most items that a space node keeps. Prosody keeps 20 by default.
const MAX_ITEMS: &str = "256";
/// The page size of the items request of a space (XEP-0059). A service may send less.
const ITEMS_PAGE: usize = 100;
/// The most items that one refresh of a space reads. The node keeps `MAX_ITEMS`.
const MAX_SYNC_ITEMS: usize = 1024;
/// The `max_items` of the card request: it counts the rooms of a space, and a card shows
/// a number, so the newest items are enough.
const CARD_ITEMS: &str = "100";
/// The largest banner that we publish, in bytes.
const MAX_BANNER_BYTES: usize = 4 * 1024 * 1024;
/// XEP-0330: a pubsub subscription item. A space can list a node that is no room. Chord
/// shows the rooms only and does not keep these items on purpose: it cannot open a node.
const NS_PUBSUB_SUBSCRIPTION: &str = "urn:xmpp:pubsub:subscription:0";

/// A space that `browse_spaces` found.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct SpaceInfo {
    /// Pubsub service JID.
    pub service: String,
    pub node: String,
    pub name: String,
    pub description: Option<String>,
    pub access_model: Option<String>,
}

/// What `space_info` reads about one space, for an invite card. It is read only.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct SpaceCard {
    /// Pubsub service JID.
    pub service: String,
    pub node: String,
    pub name: String,
    pub description: Option<String>,
    pub access_model: Option<String>,
    /// The number of rooms in the space. `None` when the service does not show the items.
    pub channels: Option<usize>,
}

/// The result of `join_space`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum JoinOutcome {
    /// We are subscribed. The space is in the space list.
    Joined,
    /// The service holds a join request for the owner to approve.
    Pending,
}

/// The access model of a new space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum SpaceAccess {
    /// Everyone can join.
    Open,
    /// The owner approves each join. The service must advertise `pubsub#access-authorize`.
    Authorize,
    /// Only members that the owner adds can join.
    Whitelist,
}

impl SpaceAccess {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Authorize => "authorize",
            Self::Whitelist => "whitelist",
        }
    }
}

/// A join request that waits for the owner of a space (XEP-0060, 8.6).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct JoinRequest {
    /// The JID that asks to join.
    pub jid: String,
    /// The subscription id, if the service gives one.
    pub subid: Option<String>,
}

/// A person with an affiliation to a space node (XEP-0060, 8.9.1). The owner sees the list.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct SpaceMember {
    pub jid: String,
    /// `owner`, `publisher`, `publish-only`, `member` or `outcast`.
    pub affiliation: String,
}

/// One field of the node configuration of a space (XEP-0060, 8.2): the variable, and its
/// values joined with a comma.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct SpaceConfigField {
    pub var: String,
    pub value: String,
}

/// A join that waits for approval: service, node, and name.
pub type PendingJoin = (String, String, String);

/// The ad-hoc command that makes the service send the pending request forms again
/// (XEP-0060, 8.7).
const GET_PENDING: &str = "http://jabber.org/protocol/pubsub#get-pending";

const FORM_SUBSCRIBE_AUTHORIZATION: &str =
    "http://jabber.org/protocol/pubsub#subscribe_authorization";
const FEATURE_ACCESS_AUTHORIZE: &str = "http://jabber.org/protocol/pubsub#access-authorize";

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The pubsub service that this session uses.
    service: Option<BareJid>,
    started: bool,
    next_browse: u64,
    browses: HashMap<u64, Browse>,
    /// The space avatar images that we fetch now, as (owner, hash).
    avatar_fetching: HashSet<(String, String)>,
    /// The subscription ids of the join requests that the owner query listed, by
    /// (service, node, jid). The answer of the owner echoes them (XEP-0060, 8.6).
    join_subids: HashMap<(String, String, String), String>,
    /// The subscription ids of the subscription list, by (service, node), until the
    /// disco#info of the node stores the space row that keeps them.
    list_subids: HashMap<(String, String), String>,
}

/// The extensions that a browse uses. The disco features of the service set them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Mode {
    rsm: bool,
    filter: bool,
    ext: bool,
}

impl Mode {
    fn any(self) -> bool {
        self.rsm || self.filter || self.ext
    }
}

#[derive(Debug)]
struct Browse {
    service: BareJid,
    mode: Mode,
    /// The pages that arrived.
    pages: usize,
    /// The `last` of the newest page, for the next request.
    after: Option<String>,
    /// Every node that we saw, to skip repeats and to keep the cap.
    known: HashSet<String>,
    /// The nodes that wait for a disco#info query.
    queue: VecDeque<String>,
    in_flight: usize,
    found: Vec<SpaceInfo>,
    reply: Reply<Vec<SpaceInfo>>,
}

/// What to do after the disco#info of a node.
#[derive(Debug)]
pub(crate) enum After {
    Nothing,
    Items,
    /// Our join worked. The subscription id is the one from the subscribe answer.
    Join(Reply<JoinOutcome>, Option<String>),
    /// Our join waits for approval. Keep the metadata, but do not follow the space.
    Pending,
}

/// What to do when a simple request succeeds.
#[derive(Debug)]
pub(crate) enum Action {
    Leave {
        service: BareJid,
        node: String,
    },
    AddRoom {
        service: BareJid,
        node: String,
        room: BareJid,
        payload: Element,
    },
    RemoveRoom {
        service: BareJid,
        node: String,
        room: BareJid,
    },
    DeleteSpace {
        service: BareJid,
        node: String,
    },
    AddMember {
        service: BareJid,
        node: String,
        member: BareJid,
    },
    /// The owner removed or banned a person. The node keeps no affiliation for them, or
    /// keeps `outcast`. Their subscription goes too.
    Unaffiliate {
        service: BareJid,
        node: String,
        member: BareJid,
    },
    /// The owner changed the name or the description.
    Configure {
        service: BareJid,
        node: String,
        name: Option<String>,
        description: Option<String>,
    },
    /// The owner published an avatar or a banner.
    Image {
        service: BareJid,
        node: String,
        item: Element,
        banner: bool,
        data: Vec<u8>,
    },
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    Subscriptions {
        service: BareJid,
    },
    NodeInfo {
        service: BareJid,
        node: String,
        after: After,
    },
    /// One page of the items of a space. The pages so far are in `items`. `after` is the
    /// `last` of the page that we asked for, to stop if the service repeats it.
    Items {
        service: BareJid,
        node: String,
        join: Option<Reply<JoinOutcome>>,
        items: Vec<(String, Option<Element>)>,
        after: Option<String>,
    },
    /// One page of the disco#items of the service.
    BrowseItems {
        browse: u64,
    },
    BrowseInfo {
        browse: u64,
        node: String,
    },
    Subscribe {
        service: BareJid,
        node: String,
        reply: Reply<JoinOutcome>,
    },
    Create {
        service: BareJid,
        node: String,
        name: String,
        description: Option<String>,
        access: SpaceAccess,
        reply: Reply<(String, String)>,
    },
    CreateSubscribe {
        service: BareJid,
        node: String,
        name: String,
        description: Option<String>,
        access: SpaceAccess,
        reply: Reply<(String, String)>,
    },
    Done {
        action: Action,
        reply: Reply<()>,
    },
    /// The service sent the pending request forms again. The owner query comes next.
    ResendForms {
        service: BareJid,
        node: String,
        reply: Reply<Vec<JoinRequest>>,
    },
    /// The pending subscriptions of a space that we own.
    JoinRequests {
        service: String,
        node: String,
        reply: Reply<Vec<JoinRequest>>,
    },
    /// The config form of a room that we put into a space: look for the pubsub field.
    RoomConfig {
        room: BareJid,
        uri: String,
    },
    /// The image of a space avatar, from the data node of the service.
    AvatarData {
        service: BareJid,
        node: String,
        hash: String,
    },
    /// The members of a space node, for a room that the owner just added. The client
    /// asks for the affiliations first, then for the subscriptions. `seen` holds the
    /// people that already got a grant.
    NodeMembers {
        service: BareJid,
        node: String,
        room: BareJid,
        subscriptions: bool,
        seen: HashSet<BareJid>,
    },
    /// The disco#info of a node for `space_info`.
    CardInfo {
        service: BareJid,
        node: String,
        reply: Reply<SpaceCard>,
    },
    /// The items of a node for `space_info`. They give the number of rooms.
    CardItems {
        card: SpaceCard,
        reply: Reply<SpaceCard>,
    },
    /// The affiliations of a space node, for the owner.
    Members {
        reply: Reply<Vec<SpaceMember>>,
    },
    /// The configuration form of a space node, for the owner.
    Config {
        reply: Reply<Vec<SpaceConfigField>>,
    },
    /// The answer does not matter.
    Ignore,
}

/// A command from the public API.
pub(crate) enum Command {
    Browse {
        reply: Reply<Vec<SpaceInfo>>,
    },
    Card {
        service: BareJid,
        node: String,
        reply: Reply<SpaceCard>,
    },
    Join {
        service: BareJid,
        node: String,
        reply: Reply<JoinOutcome>,
    },
    Leave {
        service: BareJid,
        node: String,
        reply: Reply<()>,
    },
    Create {
        name: String,
        description: Option<String>,
        access: SpaceAccess,
        reply: Reply<(String, String)>,
    },
    /// The joins that wait for approval. Reads the store, so it works offline.
    PendingJoins {
        reply: Reply<Vec<PendingJoin>>,
    },
    JoinRequests {
        service: BareJid,
        node: String,
        reply: Reply<Vec<JoinRequest>>,
    },
    /// Answer a join request: `subscribed` approves, `none` denies.
    AnswerJoin {
        service: BareJid,
        node: String,
        jid: Jid,
        state: &'static str,
        reply: Reply<()>,
    },
    AddRoom {
        service: BareJid,
        node: String,
        room: BareJid,
        name: String,
        reply: Reply<()>,
    },
    RemoveRoom {
        service: BareJid,
        node: String,
        room: BareJid,
        reply: Reply<()>,
    },
    AddMember {
        service: BareJid,
        node: String,
        member: BareJid,
        reply: Reply<()>,
    },
    Delete {
        service: BareJid,
        node: String,
        reply: Reply<()>,
    },
    Members {
        service: BareJid,
        node: String,
        reply: Reply<Vec<SpaceMember>>,
    },
    /// Set the affiliation of a person to `none` (remove) or `outcast` (ban).
    Unaffiliate {
        service: BareJid,
        node: String,
        member: BareJid,
        ban: bool,
        reply: Reply<()>,
    },
    Config {
        service: BareJid,
        node: String,
        reply: Reply<Vec<SpaceConfigField>>,
    },
    Configure {
        service: BareJid,
        node: String,
        name: Option<String>,
        description: Option<String>,
        reply: Reply<()>,
    },
    SetImage {
        service: BareJid,
        node: String,
        banner: bool,
        mime: String,
        data: Vec<u8>,
        width: u16,
        height: u16,
        reply: Reply<()>,
    },
}

impl ClientHandle {
    /// The public spaces of the pubsub service of our server.
    pub async fn browse_spaces(&self) -> Result<Vec<SpaceInfo>, ClientError> {
        self.space_call(|reply| Command::Browse { reply }).await
    }

    /// Read the name, the description, and the room count of a space. It does not join
    /// the space and it changes nothing. Fails when the node is not a space, or when the
    /// service refuses the query.
    pub async fn space_info(&self, service: &str, node: &str) -> Result<SpaceCard, ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Card {
            service,
            node,
            reply,
        })
        .await
    }

    /// Join a space: subscribe to its node. Loads its name and items before it returns.
    pub async fn join_space(&self, service: &str, node: &str) -> Result<JoinOutcome, ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Join {
            service,
            node,
            reply,
        })
        .await
    }

    /// Leave a space: unsubscribe, and forget its items.
    pub async fn leave_space(&self, service: &str, node: &str) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Leave {
            service,
            node,
            reply,
        })
        .await
    }

    /// Create a space on our pubsub service and follow it. A private space uses the
    /// `whitelist` access model. Returns the service and the node.
    pub async fn create_space(
        &self,
        name: &str,
        private: bool,
    ) -> Result<(String, String), ClientError> {
        let access = if private {
            SpaceAccess::Whitelist
        } else {
            SpaceAccess::Open
        };
        self.create_space_with(name, access).await
    }

    /// Create a space with an access model. `Authorize` needs a service that advertises
    /// `pubsub#access-authorize`. Without it, this fails with `Unsupported`.
    pub async fn create_space_with(
        &self,
        name: &str,
        access: SpaceAccess,
    ) -> Result<(String, String), ClientError> {
        self.create_space_described(name, None, access).await
    }

    /// Like `create_space_with`, with a description (`pubsub#description`). An empty
    /// description counts as none.
    pub async fn create_space_described(
        &self,
        name: &str,
        description: Option<&str>,
        access: SpaceAccess,
    ) -> Result<(String, String), ClientError> {
        let name = name.to_owned();
        let description = description.map(str::trim).filter(|d| !d.is_empty());
        let description = description.map(str::to_owned);
        self.space_call(|reply| Command::Create {
            name,
            description,
            access,
            reply,
        })
        .await
    }

    /// The spaces that we asked to join and that wait for the owner: service, node, name.
    /// This reads the store, so it works without a session.
    pub async fn pending_space_joins(&self) -> Result<Vec<PendingJoin>, ClientError> {
        self.space_call(|reply| Command::PendingJoins { reply })
            .await
    }

    /// The join requests that wait for us, the owner of an `authorize` space.
    pub async fn space_join_requests(
        &self,
        service: &str,
        node: &str,
    ) -> Result<Vec<JoinRequest>, ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::JoinRequests {
            service,
            node,
            reply,
        })
        .await
    }

    /// Approve a join request (owner only).
    pub async fn approve_space_join(
        &self,
        service: &str,
        node: &str,
        jid: &str,
    ) -> Result<(), ClientError> {
        self.answer_join(service, node, jid, "subscribed").await
    }

    /// Deny a join request (owner only).
    pub async fn deny_space_join(
        &self,
        service: &str,
        node: &str,
        jid: &str,
    ) -> Result<(), ClientError> {
        self.answer_join(service, node, jid, "none").await
    }

    async fn answer_join(
        &self,
        service: &str,
        node: &str,
        jid: &str,
        state: &'static str,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        let jid = Jid::new(jid).map_err(|e| ClientError::Invalid(format!("jid {jid}: {e}")))?;
        self.space_call(|reply| Command::AnswerJoin {
            service,
            node,
            jid,
            state,
            reply,
        })
        .await
    }

    /// Add a room to a space (owner only). The item id is the room JID.
    pub async fn add_room_to_space(
        &self,
        service: &str,
        node: &str,
        room: BareJid,
        name: &str,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let (node, name) = (node.to_owned(), name.to_owned());
        self.space_call(|reply| Command::AddRoom {
            service,
            node,
            room,
            name,
            reply,
        })
        .await
    }

    /// Remove a room from a space (owner only).
    pub async fn remove_room_from_space(
        &self,
        service: &str,
        node: &str,
        room: BareJid,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::RemoveRoom {
            service,
            node,
            room,
            reply,
        })
        .await
    }

    /// Make `member` a member of a private space (owner only). The member can then join.
    pub async fn add_space_member(
        &self,
        service: &str,
        node: &str,
        member: BareJid,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::AddMember {
            service,
            node,
            member,
            reply,
        })
        .await
    }

    /// The people with an affiliation to a space node: the owner, publishers, members and
    /// banned people (owner only, XEP-0060, 8.9.1).
    pub async fn space_members(
        &self,
        service: &str,
        node: &str,
    ) -> Result<Vec<SpaceMember>, ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Members {
            service,
            node,
            reply,
        })
        .await
    }

    /// Take the membership of `member` away (owner only): the affiliation becomes `none`
    /// and the subscription ends. The person can join again where the space is open.
    /// The rooms of the space keep their own member lists.
    pub async fn remove_space_member(
        &self,
        service: &str,
        node: &str,
        member: BareJid,
    ) -> Result<(), ClientError> {
        self.unaffiliate(service, node, member, false).await
    }

    /// Ban `member` from a space (owner only): the affiliation becomes `outcast`, and the
    /// subscription ends. The service refuses a new join of that person.
    pub async fn ban_space_member(
        &self,
        service: &str,
        node: &str,
        member: BareJid,
    ) -> Result<(), ClientError> {
        self.unaffiliate(service, node, member, true).await
    }

    async fn unaffiliate(
        &self,
        service: &str,
        node: &str,
        member: BareJid,
        ban: bool,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Unaffiliate {
            service,
            node,
            member,
            ban,
            reply,
        })
        .await
    }

    /// The node configuration form of a space, as variables and values (owner only).
    pub async fn space_config(
        &self,
        service: &str,
        node: &str,
    ) -> Result<Vec<SpaceConfigField>, ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Config {
            service,
            node,
            reply,
        })
        .await
    }

    /// Change the name or the description of a space (owner only). `None` keeps a value.
    /// An empty description clears it. A name must not be empty.
    pub async fn configure_space(
        &self,
        service: &str,
        node: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        let name = name.map(|n| n.trim().to_owned());
        if name.as_deref() == Some("") {
            return Err(ClientError::Invalid("the space name is empty".into()));
        }
        let description = description.map(|d| d.trim().to_owned());
        self.space_call(|reply| Command::Configure {
            service,
            node,
            name,
            description,
            reply,
        })
        .await
    }

    /// Set the avatar of a space (owner only). The image goes to the upload service
    /// (XEP-0363). The URL goes into XEP-0084 metadata in the avatar item of the space.
    /// `width` and `height` are the size in pixels, or 0 if the caller does not know them.
    pub async fn set_space_avatar(
        &self,
        service: &str,
        node: &str,
        mime: &str,
        data: Vec<u8>,
        width: u16,
        height: u16,
    ) -> Result<(), ClientError> {
        self.set_space_image(service, node, false, mime, data, width, height)
            .await
    }

    /// Set the banner of a space (owner only). It works like `set_space_avatar`, with
    /// the banner item and a limit of 4 MiB.
    pub async fn set_space_banner(
        &self,
        service: &str,
        node: &str,
        mime: &str,
        data: Vec<u8>,
        width: u16,
        height: u16,
    ) -> Result<(), ClientError> {
        self.set_space_image(service, node, true, mime, data, width, height)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn set_space_image(
        &self,
        service: &str,
        node: &str,
        banner: bool,
        mime: &str,
        data: Vec<u8>,
        width: u16,
        height: u16,
    ) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        let mime = mime.to_owned();
        self.space_call(|reply| Command::SetImage {
            service,
            node,
            banner,
            mime,
            data,
            width,
            height,
            reply,
        })
        .await
    }

    /// Delete a space node (owner only).
    pub async fn delete_space(&self, service: &str, node: &str) -> Result<(), ClientError> {
        let service = parse_service(service)?;
        let node = node.to_owned();
        self.space_call(|reply| Command::Delete {
            service,
            node,
            reply,
        })
        .await
    }

    async fn space_call<T>(
        &self,
        make: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Spaces(make(reply)))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

fn parse_service(service: &str) -> Result<BareJid, ClientError> {
    BareJid::new(service).map_err(|e| ClientError::Invalid(format!("service {service}: {e}")))
}

// --- Start ---

pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    if ctx.state.disco.complete {
        on_disco_complete(ctx);
    }
}

/// The disco state has all services. Find the pubsub service and load our spaces. Called
/// by `disco.rs`, and by `on_connected` if disco is complete already.
pub(crate) fn on_disco_complete(ctx: &mut Ctx<'_>) {
    if ctx.state.spaces.started {
        return;
    }
    let mut services = Vec::new();
    match spaces_service(&ctx.state.disco) {
        Some((jid, info)) => {
            let service = jid.to_bare();
            for feature in REQUIRED_FEATURES {
                if !info
                    .features
                    .contains(&format!("{FEATURE_PREFIX}{feature}"))
                {
                    log::warn!("pubsub service {service} does not advertise {feature} (XEP-0503)");
                }
            }
            ctx.state.spaces.service = Some(service.clone());
            services.push(service);
        }
        None => log::info!("the server has no pubsub service."),
    }
    // The services of the spaces that we follow, for example on other servers.
    match db::services(ctx.store.conn(), ctx.account_id) {
        Ok(known) => {
            for service in known.iter().filter_map(|s| BareJid::new(s).ok()) {
                if !services.contains(&service) {
                    services.push(service);
                }
            }
        }
        Err(e) => ctx.store_error("read the spaces", e),
    }
    if services.is_empty() {
        log::info!("no pubsub service. Spaces are off.");
        return;
    }
    ctx.state.spaces.started = true;
    for service in services {
        match pubsub_iq(false, &service, "<subscriptions/>") {
            Ok(iq) => {
                ctx.request(
                    iq,
                    FeaturePending::Spaces(Pending::Subscriptions { service }),
                );
            }
            Err(e) => log::warn!("subscriptions request: {e}"),
        }
    }
}

/// The pubsub service for spaces. A server can have more than one pubsub service. For
/// example, ejabberd for Movim has `pubsub.`, `spaces.` and `comments.`, and Movim keeps
/// its spaces on `spaces.`. So prefer `spaces.`, then `pubsub.`, then the first other one.
fn spaces_service(disco: &super::disco::State) -> Option<&(Jid, DiscoInfoResult)> {
    let rank = |jid: &Jid| {
        let domain = jid.domain().as_str();
        if domain.starts_with("spaces.") {
            0
        } else if domain.starts_with("pubsub.") {
            1
        } else {
            2
        }
    };
    disco
        .services
        .iter()
        .filter(|(_, info)| {
            info.identities
                .iter()
                .any(|i| i.category == "pubsub" && i.type_ == "service")
        })
        .min_by_key(|(jid, _)| rank(jid))
}

fn service_of(ctx: &Ctx<'_>) -> Result<BareJid, ClientError> {
    if let Some(service) = &ctx.state.spaces.service {
        return Ok(service.clone());
    }
    spaces_service(&ctx.state.disco)
        .map(|(jid, _)| jid.to_bare())
        .ok_or_else(|| ClientError::Unsupported("the server has no pubsub service".into()))
}

// --- Requests ---

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// An IQ to `service` with a `<pubsub/>` payload. `inner` is XML that the caller escaped.
fn build_iq(set: bool, owner: bool, service: &BareJid, inner: &str) -> Result<Iq, ClientError> {
    let namespace = if owner { ns::PUBSUB_OWNER } else { ns::PUBSUB };
    let xml = format!("<pubsub xmlns='{namespace}'>{inner}</pubsub>");
    let payload: Element = xml
        .parse()
        .map_err(|e| ClientError::Invalid(format!("bad pubsub request: {e}")))?;
    let to = Some(Jid::from(service.clone()));
    let id = String::new();
    Ok(if set {
        Iq::Set {
            from: None,
            to,
            id,
            payload,
        }
    } else {
        Iq::Get {
            from: None,
            to,
            id,
            payload,
        }
    })
}

fn pubsub_iq(set: bool, service: &BareJid, inner: &str) -> Result<Iq, ClientError> {
    build_iq(set, false, service, inner)
}

fn owner_iq(set: bool, service: &BareJid, inner: &str) -> Result<Iq, ClientError> {
    build_iq(set, true, service, inner)
}

fn subscribe_iq(ctx: &Ctx<'_>, service: &BareJid, node: &str) -> Result<Iq, ClientError> {
    pubsub_iq(
        true,
        service,
        &format!(
            "<subscribe node='{}' jid='{}'/>",
            xml_escape(node),
            xml_escape(ctx.account.as_str())
        ),
    )
}

/// The unsubscribe request. With the subscription id that the service gave us, it names
/// the one subscription (XEP-0060, 6.2.1): a service with more than one subscription of
/// ours to the node answers `subid-required` without it.
fn unsubscribe_iq(
    ctx: &Ctx<'_>,
    service: &BareJid,
    node: &str,
    subid: Option<&str>,
) -> Result<Iq, ClientError> {
    let subid = subid
        .map(|s| format!(" subid='{}'", xml_escape(s)))
        .unwrap_or_default();
    pubsub_iq(
        true,
        service,
        &format!(
            "<unsubscribe node='{}' jid='{}'{subid}/>",
            xml_escape(node),
            xml_escape(ctx.account.as_str())
        ),
    )
}

fn info_iq(service: &BareJid, node: &str) -> Iq {
    Iq::from_get(
        "",
        DiscoInfoQuery {
            node: Some(node.to_owned()),
        },
    )
    .with_to(Jid::from(service.clone()))
}

/// The items request of the card: the newest `CARD_ITEMS` items (XEP-0060, 6.5.7).
fn items_iq(service: &BareJid, node: &str) -> Result<Iq, ClientError> {
    pubsub_iq(
        false,
        service,
        &format!(
            "<items node='{}' max_items='{CARD_ITEMS}'/>",
            xml_escape(node)
        ),
    )
}

/// One page of the items of a node (XEP-0060, 6.5, with XEP-0059). `after` is the `last`
/// of the page before. A service without RSM ignores the set and sends all items.
fn items_page_iq(service: &BareJid, node: &str, after: Option<&str>) -> Result<Iq, ClientError> {
    let after = after
        .map(|a| format!("<after>{}</after>", xml_escape(a)))
        .unwrap_or_default();
    pubsub_iq(
        false,
        service,
        &format!(
            "<items node='{}'/><set xmlns='{}'><max>{ITEMS_PAGE}</max>{after}</set>",
            xml_escape(node),
            ns::RSM
        ),
    )
}

/// Send `iq` and route its answer through `then`, or answer `reply` with the error.
fn go<T>(
    ctx: &mut Ctx<'_>,
    iq: Result<Iq, ClientError>,
    reply: Reply<T>,
    then: impl FnOnce(Reply<T>) -> Pending,
) {
    match iq {
        Ok(iq) => {
            ctx.request(iq, FeaturePending::Spaces(then(reply)));
        }
        Err(e) => {
            let _ = reply.send(Err(e));
        }
    }
}

/// Ask for the disco#info of a node.
fn request_info(ctx: &mut Ctx<'_>, service: &BareJid, node: &str, after: After) {
    let iq = info_iq(service, node);
    ctx.request(
        iq,
        FeaturePending::Spaces(Pending::NodeInfo {
            service: service.clone(),
            node: node.to_owned(),
            after,
        }),
    );
}

fn request_items(
    ctx: &mut Ctx<'_>,
    service: &BareJid,
    node: &str,
    join: Option<Reply<JoinOutcome>>,
) {
    request_items_page(ctx, service, node, join, Vec::new(), None);
}

/// Ask for a page of the items. `items` are the pages that came before.
fn request_items_page(
    ctx: &mut Ctx<'_>,
    service: &BareJid,
    node: &str,
    join: Option<Reply<JoinOutcome>>,
    items: Vec<(String, Option<Element>)>,
    after: Option<String>,
) {
    let iq = items_page_iq(service, node, after.as_deref());
    let pending = Pending::Items {
        service: service.clone(),
        node: node.to_owned(),
        join,
        items,
        after,
    };
    match iq {
        Ok(iq) => {
            ctx.request(iq, FeaturePending::Spaces(pending));
        }
        Err(e) => {
            log::warn!("items request: {e}");
            if let Pending::Items {
                join: Some(reply), ..
            } = pending
            {
                let _ = reply.send(Ok(JoinOutcome::Joined));
            }
        }
    }
}

// --- Commands ---

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Browse { reply } => match service_of(ctx) {
            Ok(service) => start_browse(ctx, service, reply),
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },
        Command::Card {
            service,
            node,
            reply,
        } => {
            let iq = info_iq(&service, &node);
            ctx.request(
                iq,
                FeaturePending::Spaces(Pending::CardInfo {
                    service,
                    node,
                    reply,
                }),
            );
        }
        Command::Join {
            service,
            node,
            reply,
        } => {
            let iq = subscribe_iq(ctx, &service, &node);
            go(ctx, iq, reply, |reply| Pending::Subscribe {
                service,
                node,
                reply,
            });
        }
        Command::Leave {
            service,
            node,
            reply,
        } => {
            let subid = stored_subid(ctx, &service, &node);
            log::debug!("unsubscribe from {node} of {service}: subid {subid:?}");
            let iq = unsubscribe_iq(ctx, &service, &node, subid.as_deref());
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::Leave { service, node },
                reply,
            });
        }
        Command::PendingJoins { reply } => {
            let _ = reply.send(pending_joins(ctx.store, ctx.account_id));
        }
        Command::JoinRequests {
            service,
            node,
            reply,
        } => {
            // XEP-0060, 8.7: the service sends each pending request form again, to this
            // session. The service sends the forms before the command answer, so the
            // store has them when the owner query goes out. A form that went to another
            // session of the account, or to no session, comes back this way.
            let iq = adhoc::execute_iq(
                Jid::from(service.clone()),
                GET_PENDING,
                &[("pubsub#node".to_owned(), node.clone())],
            );
            ctx.request(
                iq,
                FeaturePending::Spaces(Pending::ResendForms {
                    service,
                    node,
                    reply,
                }),
            );
        }
        Command::AnswerJoin {
            service,
            node,
            jid,
            state,
            reply,
        } => {
            // XEP-0060, 8.6: the owner answers with the authorization form in a message.
            // ejabberd does not list pending subscribers in the owner subscriptions
            // query, and it acts on this form.
            let key = (service.to_string(), node.clone(), jid.to_string());
            let subid = db::request_subid(
                ctx.store.conn(),
                ctx.account_id,
                service.as_str(),
                &node,
                jid.as_str(),
            )
            .ok()
            .flatten()
            .or_else(|| ctx.state.spaces.join_subids.remove(&key));
            ctx.send(authorization_answer(
                &service,
                &node,
                jid.as_str(),
                state == "subscribed",
                subid.as_deref(),
            ));
            if state == "subscribed" {
                grant_rooms(ctx, &service, &node, &jid);
            }
            // A message has no answer. Keep the request, hidden, until the service sends
            // the subscription event (`on_subscription_event`), or lists it as pending
            // again (`Pending::JoinRequests`).
            if let Err(e) = db::answer_request(
                ctx.store.conn(),
                ctx.account_id,
                service.as_str(),
                &node,
                jid.as_str(),
            ) {
                ctx.store_error("answer a join request", e);
            }
            let _ = reply.send(Ok(()));
        }
        Command::Create {
            name,
            description,
            access,
            reply,
        } => match service_of(ctx).and_then(|service| {
            if access == SpaceAccess::Authorize && !advertises(ctx, FEATURE_ACCESS_AUTHORIZE) {
                Err(ClientError::Unsupported(
                    "the pubsub service has no access-authorize".into(),
                ))
            } else {
                Ok(service)
            }
        }) {
            Ok(service) => {
                let node = format!("space-{}", &new_id().replace('-', "")[..12]);
                let iq = pubsub_iq(
                    true,
                    &service,
                    &format!(
                        "<create node='{}'/><configure>{}</configure>",
                        xml_escape(&node),
                        node_config(&name, description.as_deref(), access)
                    ),
                );
                go(ctx, iq, reply, |reply| Pending::Create {
                    service,
                    node,
                    name,
                    description,
                    access,
                    reply,
                });
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },
        Command::AddRoom {
            service,
            node,
            room,
            name,
            reply,
        } => {
            let name_attr = if name.is_empty() {
                String::new()
            } else {
                format!(" name='{}'", xml_escape(&name))
            };
            let conference = format!(
                "<conference xmlns='{}' autojoin='false'{name_attr}/>",
                ns::BOOKMARKS2
            );
            let iq = pubsub_iq(
                true,
                &service,
                &format!(
                    "<publish node='{}'><item id='{}'>{conference}</item></publish>",
                    xml_escape(&node),
                    xml_escape(room.as_str())
                ),
            );
            let payload: Element = conference.parse().expect("the conference XML is valid");
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::AddRoom {
                    service,
                    node,
                    room,
                    payload,
                },
                reply,
            });
        }
        Command::RemoveRoom {
            service,
            node,
            room,
            reply,
        } => {
            let iq = pubsub_iq(
                true,
                &service,
                &format!(
                    "<retract node='{}' notify='true'><item id='{}'/></retract>",
                    xml_escape(&node),
                    xml_escape(room.as_str())
                ),
            );
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::RemoveRoom {
                    service,
                    node,
                    room,
                },
                reply,
            });
        }
        Command::AddMember {
            service,
            node,
            member,
            reply,
        } => {
            let iq = owner_iq(
                true,
                &service,
                &format!(
                    "<affiliations node='{}'><affiliation jid='{}' affiliation='member'/></affiliations>",
                    xml_escape(&node),
                    xml_escape(member.as_str())
                ),
            );
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::AddMember {
                    service,
                    node,
                    member,
                },
                reply,
            });
        }
        Command::Delete {
            service,
            node,
            reply,
        } => {
            let iq = owner_iq(
                true,
                &service,
                &format!("<delete node='{}'/>", xml_escape(&node)),
            );
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::DeleteSpace { service, node },
                reply,
            });
        }
        Command::Members {
            service,
            node,
            reply,
        } => {
            // XEP-0060, 8.9.1: the owner asks for the affiliations of the node.
            let iq = owner_iq(
                false,
                &service,
                &format!("<affiliations node='{}'/>", xml_escape(&node)),
            );
            go(ctx, iq, reply, |reply| Pending::Members { reply });
        }
        Command::Unaffiliate {
            service,
            node,
            member,
            ban,
            reply,
        } => {
            // XEP-0060, 8.9.2: `none` takes the affiliation away, `outcast` bans.
            let affiliation = if ban { "outcast" } else { "none" };
            let iq = owner_iq(
                true,
                &service,
                &format!(
                    "<affiliations node='{}'><affiliation jid='{}' affiliation='{affiliation}'/></affiliations>",
                    xml_escape(&node),
                    xml_escape(member.as_str())
                ),
            );
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::Unaffiliate {
                    service,
                    node,
                    member,
                },
                reply,
            });
        }
        Command::Config {
            service,
            node,
            reply,
        } => {
            let iq = owner_iq(
                false,
                &service,
                &format!("<configure node='{}'/>", xml_escape(&node)),
            );
            go(ctx, iq, reply, |reply| Pending::Config { reply });
        }
        Command::Configure {
            service,
            node,
            name,
            description,
            reply,
        } => {
            let iq = configure_iq(&service, &node, name.as_deref(), description.as_deref());
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::Configure {
                    service,
                    node,
                    name,
                    description,
                },
                reply,
            });
        }
        Command::SetImage {
            service,
            node,
            banner,
            mime,
            data,
            width,
            height,
            reply,
        } => start_image(
            ctx,
            service,
            node,
            banner,
            mime,
            data,
            (width, height),
            reply,
        ),
    }
}

/// The owner request that changes the name and the description of a space node. The
/// form has only the changed fields (XEP-0060, 8.2.4): the service keeps the others.
fn configure_iq(
    service: &BareJid,
    node: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<Iq, ClientError> {
    let mut fields = Vec::new();
    if let Some(name) = name {
        fields.push(Field::new("pubsub#title", FieldType::TextSingle).with_value(name));
    }
    if let Some(description) = description {
        fields
            .push(Field::new("pubsub#description", FieldType::TextSingle).with_value(description));
    }
    if fields.is_empty() {
        return Err(ClientError::Invalid("nothing to change".into()));
    }
    let form = DataForm::new(DataFormType::Submit, ns::PUBSUB_CONFIGURE, fields);
    owner_iq(
        true,
        service,
        &format!(
            "<configure node='{}'>{}</configure>",
            xml_escape(node),
            String::from(&Element::from(form))
        ),
    )
}

/// The subscription id that we stored for a space, if the service gave one.
fn stored_subid(ctx: &Ctx<'_>, service: &BareJid, node: &str) -> Option<String> {
    db::subid(ctx.store.conn(), ctx.account_id, service.as_str(), node)
        .ok()
        .flatten()
}

/// The node configuration form of a new space (XEP-0503, "Space Node Configuration").
fn node_config(name: &str, description: Option<&str>, access: SpaceAccess) -> String {
    let access = access.as_str();
    let text = |var: &str, value: &str| Field::new(var, FieldType::TextSingle).with_value(value);
    let boolean = |var: &str, value: &str| Field::new(var, FieldType::Boolean).with_value(value);
    let mut fields = vec![text("pubsub#type", NS_SPACES), text("pubsub#title", name)];
    if let Some(description) = description {
        fields.push(text("pubsub#description", description));
    }
    // Only the owner publishes to a space node. XEP-0503 says not to use `open`. Services
    // default to `publishers`, but a service may differ, so we ask for it.
    fields.push(Field::new("pubsub#publish_model", FieldType::ListSingle).with_value("publishers"));
    fields.extend([
        Field::new("pubsub#access_model", FieldType::ListSingle).with_value(access),
        boolean("pubsub#persist_items", "1"),
        boolean("pubsub#purge_offline", "0"),
        boolean("pubsub#notify_retract", "1"),
        boolean("pubsub#notify_sub", "1"),
        boolean("pubsub#notify_config", "1"),
        boolean("pubsub#notify_delete", "1"),
        text("pubsub#max_items", MAX_ITEMS),
    ]);
    let form = DataForm::new(DataFormType::Submit, ns::PUBSUB_CONFIGURE, fields);
    String::from(&Element::from(form))
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    fn no<T>(reply: Reply<T>) {
        let _ = reply.send(Err(ClientError::NotConnected));
    }
    match command {
        Command::Browse { reply } => no(reply),
        Command::Card { reply, .. } => no(reply),
        Command::Join { reply, .. } => no(reply),
        Command::Leave { reply, .. } => no(reply),
        Command::Create { reply, .. } => no(reply),
        Command::PendingJoins { reply } => no(reply),
        Command::JoinRequests { reply, .. } => no(reply),
        Command::AnswerJoin { reply, .. } => no(reply),
        Command::AddRoom { reply, .. } => no(reply),
        Command::RemoveRoom { reply, .. } => no(reply),
        Command::AddMember { reply, .. } => no(reply),
        Command::Delete { reply, .. } => no(reply),
        Command::Members { reply, .. } => no(reply),
        Command::Unaffiliate { reply, .. } => no(reply),
        Command::Config { reply, .. } => no(reply),
        Command::Configure { reply, .. } => no(reply),
        Command::SetImage { reply, .. } => no(reply),
    }
}

// --- Answers ---

fn server_error(error: StanzaError) -> ClientError {
    let text = error.texts.values().next().cloned().unwrap_or_default();
    let message = format!("{:?} {text}", error.defined_condition);
    ClientError::Server(message.trim().to_owned())
}

/// True if the server says that the thing does not exist (`item-not-found`). `server_error`
/// keeps the name of the condition at the start of the message.
fn is_not_found(error: &ClientError) -> bool {
    matches!(error, ClientError::Server(m) if m.starts_with("ItemNotFound"))
}

/// A refresh of a followed space failed. The node is gone: drop the space and tell the
/// user. Any other error keeps the space. A lost connection is no news.
fn on_refresh_error(ctx: &mut Ctx<'_>, service: &BareJid, node: &str, error: &ClientError) {
    if matches!(error, ClientError::NotConnected) {
        return;
    }
    if is_not_found(error) {
        let s = service.to_string();
        remove_space(ctx, &s, node);
        ctx.emit(ClientEvent::Notice(format!(
            "The space {node} no longer exists. It was removed from your list"
        )));
    } else {
        ctx.emit(ClientEvent::Notice(format!(
            "Could not refresh the space {node}: {error}"
        )));
    }
}

fn outcome(response: IqResponse) -> Result<Option<Element>, ClientError> {
    match response {
        IqResponse::Result(payload) => Ok(payload),
        IqResponse::Error(e) => Err(server_error(e)),
        IqResponse::Lost => Err(ClientError::NotConnected),
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let result = outcome(response);
    match pending {
        Pending::Ignore => {}
        Pending::Subscriptions { service } => match result {
            Ok(Some(payload)) => on_subscriptions(ctx, &service, &payload),
            other => log::warn!("subscriptions of {service}: {other:?}"),
        },
        Pending::NodeInfo {
            service,
            node,
            after,
        } => on_node_info(ctx, &service, &node, after, result),
        Pending::Items {
            service,
            node,
            join,
            mut items,
            after,
        } => {
            let lost = matches!(result, Err(ClientError::NotConnected));
            match &result {
                Ok(Some(payload)) => {
                    let page = parse_items(payload);
                    let got = page.len();
                    items.extend(page);
                    // Another page follows if the service gave a `last` that is new, and
                    // the count says that there is more. A service without RSM sends all
                    // items and no `set`.
                    let set = payload
                        .get_child("set", ns::RSM)
                        .and_then(|set| SetResult::try_from(set.clone()).ok());
                    let next = set
                        .as_ref()
                        .and_then(|set| set.last.clone())
                        .filter(|last| after.as_ref() != Some(last))
                        .filter(|_| got > 0 && items.len() < MAX_SYNC_ITEMS)
                        .filter(|_| {
                            set.as_ref()
                                .and_then(|set| set.count)
                                .is_none_or(|count| items.len() < count)
                        });
                    if next.is_some() {
                        return request_items_page(ctx, &service, &node, join, items, next);
                    }
                    items.truncate(MAX_SYNC_ITEMS);
                    sync_items(ctx, &service, &node, items);
                }
                other => {
                    log::warn!("items of {service} {node}: {other:?}");
                    if join.is_none()
                        && let Err(e) = &result
                    {
                        on_refresh_error(ctx, &service, &node, e);
                    }
                }
            }
            if let Some(reply) = join {
                let _ = reply.send(if lost {
                    Err(ClientError::NotConnected)
                } else {
                    Ok(JoinOutcome::Joined)
                });
            }
        }
        Pending::RoomConfig { room, uri } => on_room_config(ctx, &room, &uri, result),
        Pending::BrowseItems { browse } => on_browse_page(ctx, browse, result),
        Pending::CardInfo {
            service,
            node,
            reply,
        } => on_card_info(ctx, &service, &node, reply, result),
        Pending::CardItems { card, reply } => {
            let mut card = card;
            match result {
                Err(ClientError::NotConnected) => {
                    let _ = reply.send(Err(ClientError::NotConnected));
                    return;
                }
                Ok(Some(payload)) => card.channels = Some(count_rooms(&payload)),
                other => log::debug!("items of {} {}: {other:?}", card.service, card.node),
            }
            let _ = reply.send(Ok(card));
        }
        Pending::AvatarData {
            service,
            node,
            hash,
        } => on_avatar_data(ctx, &service, &node, &hash, result),
        Pending::BrowseInfo { browse, node } => on_browse_info(ctx, browse, &node, result),
        Pending::Subscribe {
            service,
            node,
            reply,
        } => match result {
            Ok(Some(payload)) => {
                let subscription = payload.get_child("subscription", ns::PUBSUB);
                let state = subscription
                    .and_then(|s| s.attr("subscription"))
                    .unwrap_or("");
                let subid = subscription
                    .and_then(|s| s.attr("subid"))
                    .map(str::to_owned);
                log::debug!("subscribe to {node} of {service}: {state}, subid {subid:?}");
                match state {
                    "subscribed" => {
                        request_info(ctx, &service, &node, After::Join(reply, subid));
                    }
                    "pending" => {
                        // Keep the request. The service tells us when the owner answers.
                        let s = service.to_string();
                        if let Err(e) =
                            db::upsert_pending(ctx.store.conn(), ctx.account_id, &s, &node)
                                .and_then(|_| {
                                    db::set_subid(
                                        ctx.store.conn(),
                                        ctx.account_id,
                                        &s,
                                        &node,
                                        subid.as_deref(),
                                    )
                                })
                        {
                            ctx.store_error("store a pending join", e);
                        }
                        request_info(ctx, &service, &node, After::Pending);
                        let _ = reply.send(Ok(JoinOutcome::Pending));
                    }
                    other => {
                        let _ = reply.send(Err(ClientError::Server(format!(
                            "subscription state {other:?}"
                        ))));
                    }
                }
            }
            Ok(None) => {
                let _ = reply.send(Err(ClientError::Server("empty answer".into())));
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },
        Pending::Create {
            service,
            node,
            name,
            description,
            access,
            reply,
        } => match result {
            Ok(_) => {
                let iq = subscribe_iq(ctx, &service, &node);
                go(ctx, iq, reply, |reply| Pending::CreateSubscribe {
                    service,
                    node,
                    name,
                    description,
                    access,
                    reply,
                });
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },
        Pending::CreateSubscribe {
            service,
            node,
            name,
            description,
            access,
            reply,
        } => match result {
            Ok(payload) => {
                let subid = payload
                    .as_ref()
                    .and_then(|p| p.get_child("subscription", ns::PUBSUB))
                    .and_then(|s| s.attr("subid"))
                    .map(str::to_owned);
                // ejabberd makes the owner's own subscription to an authorize node wait
                // for approval too. We own the node, so approve it.
                if payload.as_ref().is_some_and(is_pending_subscription) {
                    ctx.send(authorization_answer(
                        &service,
                        &node,
                        ctx.account.as_str(),
                        true,
                        subid.as_deref(),
                    ));
                }
                let access = access.as_str();
                let s = service.to_string();
                let stored = db::upsert_space(
                    ctx.store.conn(),
                    ctx.account_id,
                    &s,
                    &node,
                    Some(&name),
                    description.as_deref(),
                    Some(access),
                )
                .and_then(|_| {
                    db::set_subid(
                        ctx.store.conn(),
                        ctx.account_id,
                        &s,
                        &node,
                        subid.as_deref(),
                    )
                });
                if let Err(e) = stored {
                    ctx.store_error("store a new space", e);
                }
                changed(ctx, &s, &node);
                let _ = reply.send(Ok((s, node)));
            }
            Err(e) => {
                // The node exists but we do not follow it. The caller can delete it.
                let _ = reply.send(Err(e));
            }
        },
        Pending::ResendForms {
            service,
            node,
            reply,
        } => {
            // A service without the command still has the owner query and the forms.
            if let Err(e) = result {
                log::debug!("get-pending for {node} at {service}: {e}");
            }
            let iq = owner_iq(
                false,
                &service,
                &format!("<subscriptions node='{}'/>", xml_escape(&node)),
            );
            let service = service.to_string();
            go(ctx, iq, reply, |reply| Pending::JoinRequests {
                service,
                node,
                reply,
            });
        }
        Pending::JoinRequests {
            service,
            node,
            reply,
        } => {
            // The owner query, plus the requests that the service sent as forms. A request
            // that we answered, and that the service still lists as pending, is open again.
            let listed = match &result {
                Ok(payload) => payload
                    .as_ref()
                    .map(|p| parse_join_requests(p, &node))
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            for request in &listed {
                if let Err(e) = db::reopen_request(
                    ctx.store.conn(),
                    ctx.account_id,
                    &service,
                    &node,
                    &request.jid,
                    request.subid.as_deref(),
                ) {
                    ctx.store_error("store a join request", e);
                }
                if let Some(subid) = &request.subid {
                    let key = (service.clone(), node.clone(), request.jid.clone());
                    ctx.state.spaces.join_subids.insert(key, subid.clone());
                }
            }
            let stored = db::requests(ctx.store.conn(), ctx.account_id, &service, &node)
                .unwrap_or_else(|e| {
                    ctx.store_error("read the join requests", e);
                    Vec::new()
                });
            let _ = reply.send(result.map(|_| {
                let mut list = listed;
                for request in stored {
                    if !list.iter().any(|r| r.jid == request.jid) {
                        list.push(request);
                    }
                }
                // The owner is no request. An older version stored the form that the
                // service sends for the owner's own subscription.
                list.retain(|r| r.jid != ctx.account.as_str());
                list
            }));
        }
        Pending::NodeMembers {
            service,
            node,
            room,
            subscriptions,
            mut seen,
        } => {
            match &result {
                Ok(Some(payload)) => {
                    log::debug!(
                        "members of {node} ({subscriptions}): {}",
                        String::from(payload)
                    );
                    for jid in node_members(payload, &node, subscriptions) {
                        if jid != *ctx.account && seen.insert(jid.clone()) {
                            super::muc::grant_membership(ctx, &room, &jid);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => log::info!("cannot list the members of {service} {node}: {e}"),
            }
            if !subscriptions && !matches!(result, Err(ClientError::NotConnected)) {
                request_node_members(ctx, service, node, room, true, seen);
            }
        }
        Pending::Members { reply } => {
            let _ = reply
                .send(result.map(|payload| payload.map(|p| parse_members(&p)).unwrap_or_default()));
        }
        Pending::Config { reply } => {
            let _ = reply
                .send(result.map(|payload| payload.map(|p| parse_config(&p)).unwrap_or_default()));
        }
        Pending::Done { action, reply } => match result {
            Ok(_) => {
                apply_action(ctx, action);
                let _ = reply.send(Ok(()));
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },
    }
}

/// Make `member` a member of each room of a space. A members-only room lets in only its
/// members, so a person that the owner accepted could not enter any room without this.
/// It works where we own the rooms. Elsewhere the rooms refuse, and we ignore that.
fn grant_rooms(ctx: &mut Ctx<'_>, service: &BareJid, node: &str, member: &Jid) {
    let member = member.to_bare();
    let rooms = db::room_jids(ctx.store.conn(), ctx.account_id, service.as_str(), node)
        .unwrap_or_else(|e| {
            ctx.store_error("read the rooms of a space", e);
            Vec::new()
        });
    for room in rooms.iter().filter_map(|r| BareJid::new(r).ok()) {
        super::muc::grant_membership(ctx, &room, &member);
    }
}

/// Ask the owner view of a space node for its members. A room that the owner adds later
/// must accept the people who already joined, so the client grants each of them. The
/// affiliations and the subscriptions can differ by access model, so we read both.
fn request_node_members(
    ctx: &mut Ctx<'_>,
    service: BareJid,
    node: String,
    room: BareJid,
    subscriptions: bool,
    seen: HashSet<BareJid>,
) {
    let element = if subscriptions {
        "subscriptions"
    } else {
        "affiliations"
    };
    let Ok(iq) = owner_iq(
        false,
        &service,
        &format!("<{element} node='{}'/>", xml_escape(&node)),
    ) else {
        return;
    };
    ctx.request(
        iq,
        FeaturePending::Spaces(Pending::NodeMembers {
            service,
            node,
            room,
            subscriptions,
            seen,
        }),
    );
}

/// The JIDs in an owner answer that count as members of the node. Outcasts, people with
/// no affiliation, and pending or unconfigured subscriptions do not count.
fn node_members(payload: &Element, node: &str, subscriptions: bool) -> Vec<BareJid> {
    let (name, item, attr, good): (_, _, _, &[&str]) = if subscriptions {
        (
            "subscriptions",
            "subscription",
            "subscription",
            &["subscribed"],
        )
    } else {
        (
            "affiliations",
            "affiliation",
            "affiliation",
            &["member", "publisher", "publish-only"],
        )
    };
    let Some(list) = payload
        .get_child(name, ns::PUBSUB_OWNER)
        .or_else(|| payload.get_child(name, ns::PUBSUB))
    else {
        return Vec::new();
    };
    if list.attr("node").is_some_and(|n| n != node) {
        return Vec::new();
    }
    list.children()
        .filter(|c| c.name() == item && c.attr(attr).is_some_and(|a| good.contains(&a)))
        .filter(|c| c.attr("node").is_none_or(|n| n == node))
        .filter_map(|c| BareJid::new(c.attr("jid")?).ok())
        .collect()
}

fn apply_action(ctx: &mut Ctx<'_>, action: Action) {
    match action {
        Action::Leave { service, node } | Action::DeleteSpace { service, node } => {
            remove_space(ctx, &service.to_string(), &node);
        }
        Action::AddRoom {
            service,
            node,
            room,
            payload,
        } => {
            let s = service.to_string();
            store_item(ctx, &s, &node, room.as_str(), Some(&payload), None);
            changed(ctx, &s, &node);
            let uri = node_uri(&service, &node);
            request_node_members(ctx, service, node, room.clone(), false, HashSet::new());
            request_room_config(ctx, room, uri);
        }
        Action::RemoveRoom {
            service,
            node,
            room,
        } => {
            let s = service.to_string();
            remove_item(ctx, &s, &node, room.as_str());
            changed(ctx, &s, &node);
        }
        Action::AddMember {
            service,
            node,
            member,
        } => grant_rooms(ctx, &service, &node, &Jid::from(member)),
        Action::Unaffiliate {
            service,
            node,
            member,
        } => {
            // End the subscription too: on some services it outlives the affiliation. A
            // refusal or an error changes nothing for the owner, so the answer is ignored.
            let inner = format!(
                "<subscriptions node='{}'><subscription jid='{}' subscription='none'/></subscriptions>",
                xml_escape(&node),
                xml_escape(member.as_str())
            );
            if let Ok(iq) = owner_iq(true, &service, &inner) {
                ctx.request(iq, FeaturePending::Spaces(Pending::Ignore));
            }
            let s = service.to_string();
            if let Err(e) =
                db::remove_request(ctx.store.conn(), ctx.account_id, &s, &node, member.as_str())
            {
                ctx.store_error("remove a join request", e);
            }
        }
        Action::Configure {
            service,
            node,
            name,
            description,
        } => {
            let s = service.to_string();
            if let Err(e) = db::update_meta(
                ctx.store.conn(),
                ctx.account_id,
                &s,
                &node,
                name.as_deref(),
                description.as_deref(),
            ) {
                ctx.store_error("store a space", e);
            }
            changed(ctx, &s, &node);
        }
        Action::Image {
            service,
            node,
            item,
            banner,
            data,
        } => {
            let s = service.to_string();
            if banner {
                store_item(ctx, &s, &node, BANNER_ITEM, Some(&item), None);
            } else {
                // The service will send the change as an event. Store it now, with the
                // image that we have, so the avatar shows at once.
                store_own_avatar(ctx, &s, &node, &item, &data);
            }
            changed(ctx, &s, &node);
        }
    }
}

const NS_MUC_OWNER: &str = "http://jabber.org/protocol/muc#owner";
const NS_DATA: &str = "jabber:x:data";
/// The room config field of XEP-0503 that names the space of a room.
const FIELD_ROOM_PUBSUB: &str = "muc#roomconfig_pubsub";

/// An attribute name. Each caller passes a literal that is a valid XML name.
fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// The XMPP URI of a space node: `xmpp:SERVICE?;node=NODE` (XEP-0503).
fn node_uri(service: &BareJid, node: &str) -> String {
    let mut encoded = String::new();
    for b in node.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("xmpp:{service}?;node={encoded}")
}

/// Ask for the config form of a room that joined a space. Only the owner of the room
/// gets it. `on_room_config` does the rest, and every failure ends quietly.
fn request_room_config(ctx: &mut Ctx<'_>, room: BareJid, uri: String) {
    let iq = Iq::Get {
        from: None,
        to: Some(Jid::from(room.clone())),
        id: String::new(),
        payload: Element::builder("query", NS_MUC_OWNER).build(),
    };
    ctx.request(
        iq,
        FeaturePending::Spaces(Pending::RoomConfig { room, uri }),
    );
}

/// The submit of the pubsub field, or `None` if the form has no such field, or it has
/// the value already.
fn room_pubsub_submit(form: &Element, uri: &str) -> Option<Element> {
    let x = form.get_child("x", NS_DATA)?;
    let field = x
        .children()
        .find(|f| f.is("field", NS_DATA) && f.attr("var") == Some(FIELD_ROOM_PUBSUB))?;
    let current = field.get_child("value", NS_DATA).map(|v| v.text());
    if current.as_deref() == Some(uri) {
        return None;
    }
    let field = |var: &str, value: &str| {
        Element::builder("field", NS_DATA)
            .attr(nc("var"), var)
            .append(Element::builder("value", NS_DATA).append(value))
    };
    let x = Element::builder("x", NS_DATA)
        .attr(nc("type"), "submit")
        .append(field(
            "FORM_TYPE",
            "http://jabber.org/protocol/muc#roomconfig",
        ))
        .append(field(FIELD_ROOM_PUBSUB, uri))
        .build();
    Some(Element::builder("query", NS_MUC_OWNER).append(x).build())
}

fn on_room_config(
    ctx: &mut Ctx<'_>,
    room: &BareJid,
    uri: &str,
    result: Result<Option<Element>, ClientError>,
) {
    let form = match result {
        Ok(Some(form)) => form,
        other => {
            log::debug!("room config of {room}: {other:?}");
            return;
        }
    };
    let Some(payload) = room_pubsub_submit(&form, uri) else {
        log::debug!("room {room} has no pubsub config field, or has the space already");
        return;
    };
    let iq = Iq::Set {
        from: None,
        to: Some(Jid::from(room.clone())),
        id: String::new(),
        payload,
    };
    // A refusal changes nothing for the space. The answer has no use.
    ctx.request(iq, FeaturePending::Spaces(Pending::Ignore));
}

/// The user subscriptions of the service: keep the spaces that we follow.
fn on_subscriptions(ctx: &mut Ctx<'_>, service: &BareJid, payload: &Element) {
    let Some(list) = payload.get_child("subscriptions", ns::PUBSUB) else {
        return;
    };
    let mut nodes = Vec::new();
    let mut subids = Vec::new();
    let mut denied = Vec::new();
    for sub in list.children().filter(|c| c.is("subscription", ns::PUBSUB)) {
        let ours = match sub.attr("jid").map(BareJid::new) {
            Some(Ok(jid)) => jid == *ctx.account,
            Some(Err(_)) => false,
            None => true,
        };
        match (ours, sub.attr("subscription"), sub.attr("node")) {
            (true, Some("subscribed"), Some(node)) => {
                nodes.push(node.to_owned());
                if let Some(subid) = sub.attr("subid") {
                    subids.push((node.to_owned(), subid.to_owned()));
                }
            }
            (true, Some("none"), Some(node)) => denied.push(node.to_owned()),
            _ => {}
        }
    }
    let s = service.to_string();
    // A space that we do not follow any more: forget it.
    match db::followed_nodes(ctx.store.conn(), ctx.account_id, &s) {
        Ok(known) => {
            for node in known.iter().filter(|n| !nodes.contains(n)) {
                remove_space(ctx, &s, node);
            }
        }
        Err(e) => ctx.store_error("read the spaces", e),
    }
    // A pending join that the owner denied while we were away. A pending join that the list
    // omits stays: some services do not list them. `leave_space` cancels it.
    for node in denied {
        if db::is_pending(ctx.store.conn(), ctx.account_id, &s, &node).unwrap_or(false) {
            remove_space(ctx, &s, &node);
            ctx.emit(ClientEvent::Notice(format!(
                "Your request to join {node} was denied"
            )));
        }
    }
    // Keep the subscription ids, for the unsubscribe. A node that is no row is no space.
    for (node, subid) in subids {
        if let Err(e) = db::set_subid(ctx.store.conn(), ctx.account_id, &s, &node, Some(&subid)) {
            ctx.store_error("store a subscription id", e);
        }
        ctx.state
            .spaces
            .list_subids
            .insert((s.clone(), node), subid);
    }
    // A node that an earlier start found to be no space (for example a Movim comments
    // node) costs no disco#info now. Forget the nodes that we do not subscribe to any more.
    let skip = db::non_space_nodes(ctx.store.conn(), ctx.account_id, &s).unwrap_or_else(|e| {
        ctx.store_error("read the nodes that are no space", e);
        Vec::new()
    });
    for node in skip.iter().filter(|n| !nodes.contains(n)) {
        if let Err(e) = db::forget_non_space(ctx.store.conn(), ctx.account_id, &s, node) {
            ctx.store_error("forget a node that is no space", e);
        }
    }
    // A pending join that the owner approved becomes a space: `upsert_space` follows it.
    for node in nodes.iter().filter(|n| !skip.contains(n)) {
        request_info(ctx, service, node, After::Items);
    }
}

/// The subscriptions of a space node for its owner. Keeps the pending ones.
/// The answer of an owner to a join request (XEP-0060, 8.6): the authorization form,
/// submitted in a message to the service.
/// With the subscription id of the request, if we know it, the form names the subscription
/// (`pubsub#subid`).
fn authorization_answer(
    service: &BareJid,
    node: &str,
    jid: &str,
    allow: bool,
    subid: Option<&str>,
) -> Message {
    let mut fields = vec![
        Field::text_single("pubsub#node", node),
        Field::new("pubsub#subscriber_jid", FieldType::JidSingle).with_value(jid),
        Field::new("pubsub#allow", FieldType::Boolean).with_value(if allow {
            "true"
        } else {
            "false"
        }),
    ];
    if let Some(subid) = subid {
        fields.push(Field::text_single("pubsub#subid", subid));
    }
    let form = DataForm::new(DataFormType::Submit, FORM_SUBSCRIBE_AUTHORIZATION, fields);
    let mut message = Message::new(Some(Jid::from(service.clone())));
    message.payloads.push(form.into());
    message
}

/// True if a subscribe answer says `pending`.
fn is_pending_subscription(payload: &Element) -> bool {
    payload
        .get_child("subscription", ns::PUBSUB)
        .is_some_and(|s| s.attr("subscription") == Some("pending"))
}

fn parse_join_requests(payload: &Element, node: &str) -> Vec<JoinRequest> {
    let Some(list) = payload
        .get_child("subscriptions", ns::PUBSUB_OWNER)
        .or_else(|| payload.get_child("subscriptions", ns::PUBSUB))
    else {
        return Vec::new();
    };
    if list.attr("node").is_some_and(|n| n != node) {
        return Vec::new();
    }
    list.children()
        .filter(|c| c.name() == "subscription" && c.attr("subscription") == Some("pending"))
        .filter_map(|c| {
            Some(JoinRequest {
                jid: c.attr("jid")?.to_owned(),
                subid: c.attr("subid").map(str::to_owned),
            })
        })
        .collect()
}

/// The affiliations of an owner answer (XEP-0060, 8.9.1). The owner comes first.
fn parse_members(payload: &Element) -> Vec<SpaceMember> {
    let Some(list) = payload
        .get_child("affiliations", ns::PUBSUB_OWNER)
        .or_else(|| payload.get_child("affiliations", ns::PUBSUB))
    else {
        return Vec::new();
    };
    let mut members: Vec<SpaceMember> = list
        .children()
        .filter(|c| c.name() == "affiliation")
        .filter_map(|c| {
            Some(SpaceMember {
                jid: c.attr("jid")?.to_owned(),
                affiliation: c.attr("affiliation")?.to_owned(),
            })
        })
        .collect();
    let rank = |m: &SpaceMember| match m.affiliation.as_str() {
        "owner" => 0,
        "publisher" | "publish-only" => 1,
        "member" => 2,
        _ => 3,
    };
    members.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.jid.cmp(&b.jid)));
    members
}

/// The fields of the node configuration form of an owner answer (XEP-0060, 8.2.1).
fn parse_config(payload: &Element) -> Vec<SpaceConfigField> {
    let Some(form) = payload
        .get_child("configure", ns::PUBSUB_OWNER)
        .and_then(|c| c.get_child("x", ns::DATA_FORMS))
        .and_then(|x| DataForm::try_from(x.clone()).ok())
    else {
        return Vec::new();
    };
    form.fields
        .iter()
        .filter_map(|f| {
            Some(SpaceConfigField {
                var: f.var.clone().filter(|v| v != "FORM_TYPE")?,
                value: f.values.join(", "),
            })
        })
        .collect()
}

/// True if the pubsub service of the spaces advertises `feature`.
fn advertises(ctx: &Ctx<'_>, feature: &str) -> bool {
    spaces_service(&ctx.state.disco).is_some_and(|(_, info)| info.features.contains(feature))
}

/// A message from the service with an authorization form (XEP-0060, 8.6): a user asks to
/// join a space that we own. Emits a notice. Returns true if the message is such a form.
pub(crate) fn on_authorization(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(from) = message.from.as_ref().map(Jid::to_bare) else {
        return false;
    };
    let form = message
        .payloads
        .iter()
        .filter_map(|p| DataForm::try_from(p.clone()).ok())
        .find(|f| f.form_type() == Some(FORM_SUBSCRIBE_AUTHORIZATION));
    let Some(form) = form else {
        return false;
    };
    let value = |var: &str| {
        form.fields
            .iter()
            .find(|f| f.var.as_deref() == Some(var))
            .and_then(|f| f.values.first().cloned())
    };
    let (Some(node), Some(jid)) = (value("pubsub#node"), value("pubsub#subscriber_jid")) else {
        log::warn!("a join request from {from} has no node or subscriber");
        return true;
    };
    // ejabberd also sends a form for the owner's own subscription when it creates an
    // `authorize` node, and then subscribes the owner. That is no join request.
    if jid == ctx.account.as_str() {
        return true;
    }
    // An offline request arrives right after login, before service discovery names the
    // spaces service. So a space in the store also counts.
    let known = ctx.state.spaces.service.as_ref() == Some(&from)
        || db::space_name(ctx.store.conn(), ctx.account_id, from.as_str(), &node)
            .ok()
            .flatten()
            .is_some();
    if !known {
        log::warn!("dropped a join request from {from}: not our spaces service");
        return true;
    }
    let subid = value("pubsub#subid");
    if let Err(e) = db::add_request(
        ctx.store.conn(),
        ctx.account_id,
        from.as_str(),
        &node,
        &jid,
        subid.as_deref(),
    ) {
        ctx.store_error("store a join request", e);
    }
    let name = db::space_name(ctx.store.conn(), ctx.account_id, from.as_str(), &node)
        .ok()
        .flatten()
        .unwrap_or_else(|| node.clone());
    ctx.emit(ClientEvent::Notice(format!(
        "{jid} asks to join the space {name}"
    )));
    true
}

/// The joins that wait for approval.
pub(crate) fn pending_joins(
    store: &Store,
    account_id: i64,
) -> Result<Vec<PendingJoin>, ClientError> {
    db::pending(store.conn(), account_id).map_err(|e| ClientError::Invalid(format!("store: {e}")))
}

/// A subscription notification about us (XEP-0060, 8.x). It answers a join that waited for
/// the owner. Any other subscription event is dropped.
fn on_subscription_event(
    ctx: &mut Ctx<'_>,
    service: &Jid,
    node: &str,
    jid: Option<Jid>,
    state: Option<Subscription>,
    subid: Option<String>,
) {
    if service.resource().is_some() {
        return;
    }
    let bare = service.to_bare();
    let s = bare.to_string();
    if let Some(other) = jid
        .as_ref()
        .map(Jid::to_bare)
        .filter(|j| *j != *ctx.account)
    {
        // A person that asked to join our space: the service answered the request that
        // we answered (XEP-0060, 8.6), so the stored request is done.
        if matches!(state, Some(Subscription::Subscribed | Subscription::None))
            && let Err(e) =
                db::remove_request(ctx.store.conn(), ctx.account_id, &s, node, other.as_str())
        {
            ctx.store_error("remove a join request", e);
        }
        return;
    }
    if !db::is_pending(ctx.store.conn(), ctx.account_id, &s, node).unwrap_or(false) {
        log::debug!("dropped a subscription event for {node} of {service}: no pending join");
        return;
    }
    let name = db::space_name(ctx.store.conn(), ctx.account_id, &s, node)
        .ok()
        .flatten()
        .unwrap_or_else(|| node.to_owned());
    match state {
        Some(Subscription::Subscribed) => {
            if let Err(e) =
                db::set_subid(ctx.store.conn(), ctx.account_id, &s, node, subid.as_deref())
            {
                ctx.store_error("store a subscription id", e);
            }
            request_info(ctx, &bare, node, After::Items);
            ctx.emit(ClientEvent::Notice(format!(
                "Your request to join {name} was approved"
            )));
        }
        Some(Subscription::None) => {
            remove_space(ctx, &s, node);
            ctx.emit(ClientEvent::Notice(format!(
                "Your request to join {name} was denied"
            )));
        }
        _ => {}
    }
}

fn on_node_info(
    ctx: &mut Ctx<'_>,
    service: &BareJid,
    node: &str,
    after: After,
    result: Result<Option<Element>, ClientError>,
) {
    let lost = matches!(result, Err(ClientError::NotConnected));
    let info = result
        .as_ref()
        .ok()
        .and_then(|p| p.clone())
        .and_then(|p| DiscoInfoResult::try_from(p).ok());
    let s = service.to_string();
    if matches!(after, After::Pending) {
        // Only the metadata: the space is not ours yet.
        if let Some(info) = info {
            let meta = meta_of(&info);
            if let Err(e) = db::update_pending_meta(
                ctx.store.conn(),
                ctx.account_id,
                &s,
                node,
                meta.title.as_deref(),
                meta.description.as_deref(),
                meta.access_model.as_deref(),
            ) {
                ctx.store_error("store a pending join", e);
            }
            changed(ctx, &s, node);
        }
        return;
    }
    match (info, &after) {
        (Some(info), _) => {
            let meta = meta_of(&info);
            if meta.type_.as_deref() != Some(NS_SPACES) {
                log::info!("node {node} of {service} is not a space");
                // Remember it, if the answer had the meta-data form: no disco#info for
                // this node at the next start.
                let has_meta = info
                    .extensions
                    .iter()
                    .any(|f| f.form_type() == Some(NS_META));
                if has_meta
                    && matches!(after, After::Items)
                    && let Err(e) = db::add_non_space(ctx.store.conn(), ctx.account_id, &s, node)
                {
                    ctx.store_error("store a node that is no space", e);
                }
                if let After::Join(reply, subid) = after {
                    let _ = reply.send(Err(ClientError::Invalid("the node is not a space".into())));
                    if let Ok(iq) = unsubscribe_iq(ctx, service, node, subid.as_deref()) {
                        ctx.request(iq, FeaturePending::Spaces(Pending::Ignore));
                    }
                }
                return;
            }
            if let Err(e) = db::upsert_space(
                ctx.store.conn(),
                ctx.account_id,
                &s,
                node,
                meta.title.as_deref(),
                meta.description.as_deref(),
                meta.access_model.as_deref(),
            ) {
                ctx.store_error("store a space", e);
            }
            if let Err(e) = db::forget_non_space(ctx.store.conn(), ctx.account_id, &s, node) {
                ctx.store_error("forget a node that is no space", e);
            }
            if let Some(subid) = ctx
                .state
                .spaces
                .list_subids
                .remove(&(s.clone(), node.to_owned()))
                && let Err(e) =
                    db::set_subid(ctx.store.conn(), ctx.account_id, &s, node, Some(&subid))
            {
                ctx.store_error("store a subscription id", e);
            }
            if let After::Join(_, Some(subid)) = &after
                && let Err(e) =
                    db::set_subid(ctx.store.conn(), ctx.account_id, &s, node, Some(subid))
            {
                ctx.store_error("store a subscription id", e);
            }
        }
        (None, After::Join(..)) if !lost => {
            // We are subscribed already. Keep the space without its metadata.
            log::warn!("no disco#info for {node} of {service}: {result:?}");
            let subid = match &after {
                After::Join(_, subid) => subid.as_deref(),
                _ => None,
            };
            let stored =
                db::upsert_space(ctx.store.conn(), ctx.account_id, &s, node, None, None, None)
                    .and_then(|_| db::set_subid(ctx.store.conn(), ctx.account_id, &s, node, subid));
            if let Err(e) = stored {
                ctx.store_error("store a space", e);
            }
        }
        (None, _) => {
            log::warn!("no disco#info for {node} of {service}: {result:?}");
            match (after, &result) {
                (After::Join(reply, _), _) => {
                    let _ = reply.send(Err(ClientError::NotConnected));
                }
                (After::Items, Err(e)) => on_refresh_error(ctx, service, node, e),
                _ => {}
            }
            return;
        }
    }
    changed(ctx, &s, node);
    match after {
        After::Nothing | After::Pending => {}
        After::Items => request_items(ctx, service, node, None),
        After::Join(reply, _) => request_items(ctx, service, node, Some(reply)),
    }
}

/// The disco#info answer for `space_info`. A node that is no space is an error.
fn on_card_info(
    ctx: &mut Ctx<'_>,
    service: &BareJid,
    node: &str,
    reply: Reply<SpaceCard>,
    result: Result<Option<Element>, ClientError>,
) {
    let payload = match result {
        Ok(Some(payload)) => payload,
        Ok(None) => {
            let _ = reply.send(Err(ClientError::Server("empty answer".into())));
            return;
        }
        Err(e) => {
            let _ = reply.send(Err(e));
            return;
        }
    };
    let Ok(info) = DiscoInfoResult::try_from(payload) else {
        let _ = reply.send(Err(ClientError::Server("bad answer".into())));
        return;
    };
    let meta = meta_of(&info);
    if meta.type_.as_deref() != Some(NS_SPACES) {
        let _ = reply.send(Err(ClientError::Invalid("the node is not a space".into())));
        return;
    }
    let found = space_info(service, node, meta);
    let card = SpaceCard {
        service: found.service,
        node: found.node,
        name: found.name,
        description: found.description,
        access_model: found.access_model,
        channels: None,
    };
    match items_iq(service, node) {
        Ok(iq) => {
            ctx.request(
                iq,
                FeaturePending::Spaces(Pending::CardItems { card, reply }),
            );
        }
        Err(_) => {
            let _ = reply.send(Ok(card));
        }
    }
}

/// The number of room items in an `<items/>` result.
fn count_rooms(payload: &Element) -> usize {
    parse_items(payload)
        .iter()
        .filter(|(_, p)| {
            p.as_ref()
                .is_some_and(|p| p.is("conference", ns::BOOKMARKS2))
        })
        .count()
}

#[derive(Default)]
struct Meta {
    type_: Option<String>,
    title: Option<String>,
    description: Option<String>,
    access_model: Option<String>,
}

/// The pubsub#meta-data form of a node disco#info.
fn meta_of(info: &DiscoInfoResult) -> Meta {
    match info
        .extensions
        .iter()
        .find(|f| f.form_type() == Some(NS_META))
    {
        Some(form) => meta_of_form(form),
        None => Meta::default(),
    }
}

/// The values of a pubsub#meta-data form.
fn meta_of_form(form: &DataForm) -> Meta {
    let mut meta = Meta::default();
    let value = |var: &str| {
        form.fields
            .iter()
            .find(|f| f.var.as_deref() == Some(var))
            .and_then(|f| f.values.first().cloned())
    };
    meta.type_ = value("pubsub#type");
    meta.title = value("pubsub#title");
    meta.description = value("pubsub#description");
    meta.access_model = value("pubsub#access_model");
    meta
}

/// The extensions that the service advertises, from the disco state.
fn browse_mode(ctx: &Ctx<'_>, service: &BareJid) -> Mode {
    let Some((_, info)) = ctx
        .state
        .disco
        .services
        .iter()
        .find(|(jid, _)| jid.to_bare() == *service)
    else {
        return Mode::default();
    };
    let has = |feature: &str| info.features.contains(feature);
    Mode {
        rsm: has(FEATURE_RSM),
        filter: has(NS_TYPE_FILTER),
        ext: has(NS_EXT_DISCO) || has(NS_EXT_DISCO_SHORT),
    }
}

/// The disco#items query of one browse page.
fn browse_query(mode: Mode, after: Option<&str>) -> Element {
    let rsm = mode.rsm.then(|| SetQuery {
        max: Some(PAGE_SIZE),
        after: after.map(str::to_owned),
        before: None,
        index: None,
    });
    let mut query = Element::from(DiscoItemsQuery { node: None, rsm });
    if mode.filter {
        let form = DataForm::new(
            DataFormType::Submit,
            NS_TYPE_FILTER,
            vec![Field::new("included-types", FieldType::ListMulti).with_value(NS_SPACES)],
        );
        let filter = Element::builder("filter", NS_TYPE_FILTER)
            .append(Element::from(form))
            .build();
        query.append_child(filter);
    }
    if mode.ext {
        let form = DataForm::new(
            DataFormType::Submit,
            NS_EXT_DISCO,
            vec![
                Field::new("type", FieldType::ListMulti).with_value("nodes"),
                Field::new("full_metadata", FieldType::Boolean).with_value("true"),
            ],
        );
        query.append_child(Element::from(form));
    }
    query
}

fn start_browse(ctx: &mut Ctx<'_>, service: BareJid, reply: Reply<Vec<SpaceInfo>>) {
    let id = ctx.state.spaces.next_browse;
    ctx.state.spaces.next_browse += 1;
    let mode = browse_mode(ctx, &service);
    ctx.state.spaces.browses.insert(
        id,
        Browse {
            service,
            mode,
            pages: 0,
            after: None,
            known: HashSet::new(),
            queue: VecDeque::new(),
            in_flight: 0,
            found: Vec::new(),
            reply,
        },
    );
    request_page(ctx, id);
}

/// Ask for the next page of the nodes of a browse.
fn request_page(ctx: &mut Ctx<'_>, id: u64) {
    let Some(browse) = ctx.state.spaces.browses.get(&id) else {
        return;
    };
    let payload = browse_query(browse.mode, browse.after.as_deref());
    let iq = Iq::Get {
        from: None,
        to: Some(Jid::from(browse.service.clone())),
        id: String::new(),
        payload,
    };
    ctx.request(
        iq,
        FeaturePending::Spaces(Pending::BrowseItems { browse: id }),
    );
}

/// End a browse with an error.
fn fail_browse(ctx: &mut Ctx<'_>, id: u64, error: ClientError) {
    if let Some(browse) = ctx.state.spaces.browses.remove(&id) {
        let _ = browse.reply.send(Err(error));
    }
}

/// One node of a disco#items page. `meta` is the XEP-0499 metadata form, if the item has it.
struct PageItem {
    node: String,
    meta: Option<Meta>,
}

struct Page {
    items: Vec<PageItem>,
    /// RSM `last`, and `count`.
    last: Option<String>,
    count: Option<usize>,
}

fn parse_page(payload: &Element) -> Option<Page> {
    if !payload.is("query", ns::DISCO_ITEMS) {
        return None;
    }
    let items = payload
        .children()
        .filter(|c| c.is("item", ns::DISCO_ITEMS))
        .filter_map(|item| {
            let node = item.attr("node")?.to_owned();
            let meta = item
                .children()
                .find(|c| c.is("x", ns::DATA_FORMS))
                .and_then(|x| DataForm::try_from(x.clone()).ok())
                .filter(|form| form.form_type() == Some(NS_META))
                .map(|form| meta_of_form(&form));
            Some(PageItem { node, meta })
        })
        .collect();
    let rsm = payload
        .children()
        .find(|c| c.is("set", ns::RSM))
        .and_then(|set| SetResult::try_from(set.clone()).ok());
    Some(Page {
        items,
        last: rsm.as_ref().and_then(|r| r.last.clone()),
        count: rsm.as_ref().and_then(|r| r.count),
    })
}

fn space_info(service: &BareJid, node: &str, meta: Meta) -> SpaceInfo {
    SpaceInfo {
        service: service.to_string(),
        name: meta.title.unwrap_or_else(|| node.to_owned()),
        node: node.to_owned(),
        description: meta.description,
        access_model: meta.access_model,
    }
}

/// Browse lists the spaces that a user can join: `open` spaces, and `authorize` spaces,
/// where the owner approves each join request. `access_model` tells a UI which one.
fn is_open_space(meta: &Meta) -> bool {
    meta.type_.as_deref() == Some(NS_SPACES)
        && matches!(meta.access_model.as_deref(), Some("open" | "authorize"))
}

fn on_browse_page(ctx: &mut Ctx<'_>, id: u64, result: Result<Option<Element>, ClientError>) {
    let Some(browse) = ctx.state.spaces.browses.get_mut(&id) else {
        return;
    };
    let page = match result {
        Ok(Some(payload)) => {
            parse_page(&payload).ok_or_else(|| ClientError::Server("bad disco#items answer".into()))
        }
        Ok(None) => Err(ClientError::Server("empty answer".into())),
        Err(e) => Err(e),
    };
    let page = match page {
        Ok(page) => page,
        Err(ClientError::NotConnected) => return fail_browse(ctx, id, ClientError::NotConnected),
        Err(e) if browse.pages == 0 && browse.mode.any() => {
            // The service may not support an extension that it advertises. Start again.
            log::warn!(
                "browse with {:?} failed: {e}. Trying a plain request.",
                browse.mode
            );
            browse.mode = Mode::default();
            request_page(ctx, id);
            return;
        }
        Err(e) if browse.pages == 0 => return fail_browse(ctx, id, e),
        Err(e) => {
            // Keep the nodes of the earlier pages.
            log::warn!("browse: a later page failed: {e}");
            return pump_browse(ctx, id);
        }
    };
    browse.pages += 1;
    let before = browse.known.len();
    for item in page.items {
        if browse.known.len() >= MAX_BROWSE_NODES {
            break;
        }
        if !browse.known.insert(item.node.clone()) {
            continue;
        }
        // Trust the metadata only if we asked for it, and only if it names a type.
        match item.meta.filter(|m| browse.mode.ext && m.type_.is_some()) {
            Some(meta) if is_open_space(&meta) => {
                browse
                    .found
                    .push(space_info(&browse.service, &item.node, meta));
            }
            Some(_) => {}
            None => browse.queue.push_back(item.node),
        }
    }
    // Another page follows if this one brought new nodes, and the service has more.
    let added = browse.known.len() - before;
    let more = browse.mode.rsm
        && added > 0
        && browse.known.len() < MAX_BROWSE_NODES
        && page.last.is_some()
        && page.last != browse.after
        && page.count.is_none_or(|count| browse.known.len() < count);
    if more {
        browse.after = page.last;
        request_page(ctx, id);
    } else {
        pump_browse(ctx, id);
    }
}

/// Send the disco#info queries that fit in the window. End the browse if none is left.
fn pump_browse(ctx: &mut Ctx<'_>, id: u64) {
    loop {
        let Some(browse) = ctx.state.spaces.browses.get_mut(&id) else {
            return;
        };
        if browse.in_flight >= BROWSE_WINDOW {
            return;
        }
        let Some(node) = browse.queue.pop_front() else {
            break;
        };
        browse.in_flight += 1;
        let iq = info_iq(&browse.service, &node);
        ctx.request(
            iq,
            FeaturePending::Spaces(Pending::BrowseInfo { browse: id, node }),
        );
    }
    let Some(browse) = ctx.state.spaces.browses.get(&id) else {
        return;
    };
    if browse.in_flight == 0
        && let Some(mut done) = ctx.state.spaces.browses.remove(&id)
    {
        done.found
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.node.cmp(&b.node)));
        let _ = done.reply.send(Ok(done.found));
    }
}

fn on_browse_info(
    ctx: &mut Ctx<'_>,
    id: u64,
    node: &str,
    result: Result<Option<Element>, ClientError>,
) {
    let Some(browse) = ctx.state.spaces.browses.get_mut(&id) else {
        return;
    };
    browse.in_flight = browse.in_flight.saturating_sub(1);
    // The session ended: no answer for the other queries will come.
    if matches!(result, Err(ClientError::NotConnected)) {
        return fail_browse(ctx, id, ClientError::NotConnected);
    }
    if let Ok(Some(payload)) = result
        && let Ok(info) = DiscoInfoResult::try_from(payload)
    {
        let meta = meta_of(&info);
        if is_open_space(&meta) {
            browse.found.push(space_info(&browse.service, node, meta));
        }
    }
    pump_browse(ctx, id);
}

// --- Items ---

/// The items of an `<items/>` result: id and payload.
fn parse_items(payload: &Element) -> Vec<(String, Option<Element>)> {
    let Some(items) = payload.get_child("items", ns::PUBSUB) else {
        return Vec::new();
    };
    items
        .children()
        .filter(|c| c.is("item", ns::PUBSUB))
        .filter_map(|item| {
            let id = item.attr("id")?.to_owned();
            Some((id, item.children().next().cloned()))
        })
        .collect()
}

/// Replace the items of a node with the items that the server sent.
fn sync_items(
    ctx: &mut Ctx<'_>,
    service: &BareJid,
    node: &str,
    items: Vec<(String, Option<Element>)>,
) {
    let s = service.to_string();
    match db::is_followed(ctx.store.conn(), ctx.account_id, &s, node) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => return ctx.store_error("read a space", e),
    }
    for (i, (id, payload)) in items.iter().enumerate() {
        store_item(ctx, &s, node, id, payload.as_ref(), Some(i as i64));
    }
    let ids: Vec<&str> = items.iter().map(|(id, _)| id.as_str()).collect();
    let stored = db::item_ids(ctx.store.conn(), ctx.account_id, &s, node).unwrap_or_default();
    for gone in stored.iter().filter(|id| !ids.contains(&id.as_str())) {
        remove_item(ctx, &s, node, gone);
    }
    if !ids.contains(&AVATAR_ITEM) {
        remove_item(ctx, &s, node, AVATAR_ITEM);
    }
    changed(ctx, &s, node);
}

/// Store one item. A room item has a room JID for the item id. The avatar item goes to
/// the avatars table. Any other item keeps its XML.
fn store_item(
    ctx: &mut Ctx<'_>,
    service: &str,
    node: &str,
    id: &str,
    payload: Option<&Element>,
    position: Option<i64>,
) {
    if id == AVATAR_ITEM {
        store_avatar_item(ctx, service, node, payload);
        return;
    }
    // XEP-0330: an item that names a pubsub node. We show rooms only, so we ignore these
    // items on purpose. An older version kept their XML: drop that row.
    if payload.is_some_and(|p| p.ns() == NS_PUBSUB_SUBSCRIPTION) {
        log::debug!("ignored a XEP-0330 subscription item {id} of {service} {node}");
        if let Err(e) = db::delete_item(ctx.store.conn(), ctx.account_id, service, node, id) {
            ctx.store_error("remove a space item", e);
        }
        return;
    }
    let room = payload
        .filter(|p| p.is("conference", ns::BOOKMARKS2))
        .and_then(|_| BareJid::new(id).ok());
    let name = payload
        .filter(|_| room.is_some())
        .and_then(|p| p.attr("name"));
    let xml = payload.map(String::from);
    let result = db::upsert_item(
        ctx.store.conn(),
        ctx.account_id,
        &db::Item {
            service,
            node,
            id,
            room_jid: room.as_ref().map(|r| r.as_str()),
            name,
            payload: xml.as_deref(),
        },
        position,
    );
    if let Err(e) = result {
        ctx.store_error("store a space item", e);
    }
}

/// The first `<info/>` of an avatar metadata element.
struct AvatarInfo {
    hash: String,
    mime: Option<String>,
    bytes: Option<usize>,
    url: Option<String>,
}

/// `None` if `payload` is no metadata element. `Some(None)` if it has no info: the space
/// has no avatar (XEP-0084, 4.2).
fn avatar_of(payload: &Element) -> Option<Option<AvatarInfo>> {
    if !payload.is("metadata", NS_AVATAR_METADATA) {
        return None;
    }
    let info = payload
        .children()
        .find(|c| c.is("info", NS_AVATAR_METADATA))
        .and_then(|info| {
            Some(AvatarInfo {
                hash: info.attr("id")?.to_owned(),
                mime: info.attr("type").map(str::to_owned),
                bytes: info.attr("bytes").and_then(|b| b.parse().ok()),
                url: info.attr("url").map(str::to_owned),
            })
        });
    Some(info)
}

/// The owner of the avatar of a space.
fn avatar_owner(service: &str, node: &str) -> String {
    format!("{service}/{node}")
}

/// The stored avatar of a space, or `None`. The space list has only its hash.
pub fn load_avatar(
    store: &Store,
    account_id: i64,
    service: &str,
    node: &str,
) -> rusqlite::Result<Option<avatars::Avatar>> {
    avatars::load_key(store, account_id, &avatar_owner(service, node))
}

/// Store the image of a space avatar, after a check of its size and its SHA-1 against
/// the stored hash (lower case hex). Returns false if the space has another avatar now.
/// Use it for an image that the caller downloaded from the URL of the avatar info.
pub fn store_avatar_image(
    store: &Store,
    account_id: i64,
    service: &str,
    node: &str,
    hash: &str,
    data: &[u8],
) -> Result<bool, ClientError> {
    avatars::verify_image(hash, data)?;
    avatars::store_data(store, account_id, &avatar_owner(service, node), hash, data)
        .map_err(|e| ClientError::Invalid(format!("store error: {e}")))
}

/// Handle the avatar item of a space: store the hash, and ask for the image if we lack it.
fn store_avatar_item(ctx: &mut Ctx<'_>, service: &str, node: &str, payload: Option<&Element>) {
    let owner = avatar_owner(service, node);
    let info = match payload.and_then(avatar_of) {
        Some(Some(info)) => info,
        Some(None) => {
            if let Err(e) = avatars::remove(ctx.store, ctx.account_id, &owner) {
                ctx.store_error("remove a space avatar", e);
            }
            return;
        }
        None => return,
    };
    // The hash of a proper info is lower case hex. Make sure that our check finds it so.
    let hash = info.hash.to_ascii_lowercase();
    let stored = match avatars::store_metadata(
        ctx.store,
        ctx.account_id,
        &owner,
        &hash,
        info.mime.as_deref(),
    ) {
        Ok(stored) => stored,
        Err(e) => return ctx.store_error("store a space avatar", e),
    };
    if !stored.needs_data {
        return;
    }
    let sane = hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    if !sane || info.bytes.is_some_and(|b| b > MAX_AVATAR_BYTES) {
        log::warn!("the avatar of {owner} has a bad hash or a big size. Not fetched.");
        return;
    }
    if let Some(url) = info.url {
        if ctx
            .state
            .spaces
            .avatar_fetching
            .insert((owner, hash.clone()))
        {
            ctx.download(DownloadRequest {
                service: service.to_owned(),
                node: node.to_owned(),
                hash,
                url,
                max_bytes: MAX_AVATAR_BYTES,
            });
        }
        return;
    }
    let Ok(service) = BareJid::new(service) else {
        return;
    };
    if !ctx
        .state
        .spaces
        .avatar_fetching
        .insert((owner, hash.clone()))
    {
        return;
    }
    let inner = format!(
        "<items node='{NS_AVATAR_DATA}'><item id='{}'/></items>",
        xml_escape(&hash)
    );
    let node = node.to_owned();
    match pubsub_iq(false, &service, &inner) {
        Ok(iq) => {
            let pending = Pending::AvatarData {
                service,
                node,
                hash,
            };
            ctx.request(iq, FeaturePending::Spaces(pending));
        }
        Err(e) => log::warn!("avatar request: {e}"),
    }
}

/// What an image upload needs after the PUT: the place of the item and the metadata.
#[derive(Debug)]
pub(crate) struct ImageJob {
    service: BareJid,
    node: String,
    banner: bool,
    mime: String,
    /// The SHA-1 of the image, lower case hex (XEP-0084).
    hash: String,
    data: Vec<u8>,
    width: u16,
    height: u16,
}

/// The file extension for the name of an upload. The server may show it in the URL.
fn extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        _ => "img",
    }
}

/// Set the avatar or the banner of a space: upload the image (XEP-0363), then publish
/// the XEP-0084 metadata with the URL (`on_image_uploaded`).
#[allow(clippy::too_many_arguments)]
fn start_image(
    ctx: &mut Ctx<'_>,
    service: BareJid,
    node: String,
    banner: bool,
    mime: String,
    data: Vec<u8>,
    (width, height): (u16, u16),
    reply: Reply<()>,
) {
    let limit = if banner {
        MAX_BANNER_BYTES
    } else {
        MAX_AVATAR_BYTES
    };
    if data.is_empty() || data.len() > limit || !mime.starts_with("image/") {
        let what = if banner { "a banner" } else { "an avatar" };
        let _ = reply.send(Err(ClientError::Invalid(format!(
            "{what} is an image of 1 byte to {} KiB",
            limit / 1024
        ))));
        return;
    }
    let hash = avatars::sha1_hex(&data);
    let kind = if banner { "banner" } else { "avatar" };
    let filename = format!("space-{kind}-{}.{}", &hash[..12], extension(&mime));
    let job = ImageJob {
        service,
        node,
        banner,
        mime: mime.clone(),
        hash,
        data: data.clone(),
        width,
        height,
    };
    super::upload::start_space_upload(ctx, filename, mime, data, job, reply);
}

/// The upload of a space image worked: publish the metadata item. The item id is the
/// avatar item or the banner item. The URL is the GET URL of the upload service.
pub(crate) fn on_image_uploaded(ctx: &mut Ctx<'_>, job: ImageJob, url: String, reply: Reply<()>) {
    let ImageJob {
        service,
        node,
        banner,
        mime,
        hash,
        data,
        width,
        height,
    } = job;
    let size = |name: &str, value: u16| {
        if value > 0 {
            format!(" {name}='{value}'")
        } else {
            String::new()
        }
    };
    let metadata = format!(
        "<metadata xmlns='{NS_AVATAR_METADATA}'><info bytes='{}' id='{hash}' type='{}' url='{}'{}{}/></metadata>",
        data.len(),
        xml_escape(&mime),
        xml_escape(&url),
        size("width", width),
        size("height", height),
    );
    let item_id = if banner { BANNER_ITEM } else { AVATAR_ITEM };
    let iq = pubsub_iq(
        true,
        &service,
        &format!(
            "<publish node='{}'><item id='{item_id}'>{metadata}</item></publish>",
            xml_escape(&node)
        ),
    );
    let item: Element = metadata.parse().expect("the metadata XML is valid");
    go(ctx, iq, reply, |reply| Pending::Done {
        action: Action::Image {
            service,
            node,
            item,
            banner,
            data,
        },
        reply,
    });
}

/// Store the avatar that we just published, with its image. The hash is ours, so there is
/// no fetch.
fn store_own_avatar(ctx: &mut Ctx<'_>, service: &str, node: &str, item: &Element, data: &[u8]) {
    let Some(Some(info)) = avatar_of(item) else {
        return;
    };
    let owner = avatar_owner(service, node);
    let stored = avatars::store_metadata(
        ctx.store,
        ctx.account_id,
        &owner,
        &info.hash,
        info.mime.as_deref(),
    );
    if let Err(e) = stored {
        return ctx.store_error("store a space avatar", e);
    }
    if let Err(e) = store_avatar_image(ctx.store, ctx.account_id, service, node, &info.hash, data) {
        log::warn!("avatar image for {node} of {service}: {e}");
    }
}

/// An HTTP GET for the image of a space avatar. The runtime runs it.
#[derive(Debug)]
pub(crate) struct DownloadRequest {
    pub service: String,
    pub node: String,
    /// The lower case SHA-1 hex of the image.
    pub hash: String,
    pub url: String,
    pub max_bytes: usize,
}

/// The result of a `DownloadRequest`.
#[derive(Debug)]
pub(crate) struct DownloadDone {
    pub request: DownloadRequest,
    pub result: Result<Vec<u8>, String>,
}

/// Run a download. Send the result to `done` as `Internal::DownloadDone`.
#[cfg(feature = "native-session")]
pub(crate) fn start_download(
    request: DownloadRequest,
    done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    crate::runtime::spawn(async move {
        let result = crate::runtime::http_get(&request.url, request.max_bytes).await;
        // An error means that the actor stopped. Nobody waits for the result.
        let _ = done.unbounded_send(super::Internal::DownloadDone(DownloadDone {
            request,
            result,
        }));
    });
}

/// Without the native session there is no HTTP client yet. Fail at once.
#[cfg(not(feature = "native-session"))]
pub(crate) fn start_download(
    request: DownloadRequest,
    done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    let _ = done.unbounded_send(super::Internal::DownloadDone(DownloadDone {
        request,
        result: Err("HTTP download is not available in this build".into()),
    }));
}

/// A download finished. Store the image, or keep the hash only.
pub(crate) fn on_download_done(ctx: &mut Ctx<'_>, done: DownloadDone) {
    let DownloadDone { request, result } = done;
    let (service, node, hash) = (&request.service, &request.node, &request.hash);
    ctx.state
        .spaces
        .avatar_fetching
        .remove(&(avatar_owner(service, node), hash.clone()));
    let data = match result {
        Ok(data) => data,
        Err(e) => {
            log::warn!("avatar download for {node} of {service}: {e}");
            return;
        }
    };
    match store_avatar_image(ctx.store, ctx.account_id, service, node, hash, &data) {
        Ok(true) => changed(ctx, service, node),
        Ok(false) => {}
        Err(e) => log::warn!("avatar image for {node} of {service}: {e}"),
    }
}

fn on_avatar_data(
    ctx: &mut Ctx<'_>,
    service: &BareJid,
    node: &str,
    hash: &str,
    result: Result<Option<Element>, ClientError>,
) {
    let s = service.to_string();
    ctx.state
        .spaces
        .avatar_fetching
        .remove(&(avatar_owner(&s, node), hash.to_owned()));
    let data = result
        .as_ref()
        .ok()
        .and_then(|p| p.as_ref())
        .and_then(|p| p.get_child("items", ns::PUBSUB))
        .and_then(|items| {
            items
                .children()
                .filter(|c| c.is("item", ns::PUBSUB))
                .find_map(|item| item.children().next())
        })
        .and_then(|element| AvatarData::try_from(element.clone()).ok())
        .map(|d| d.data);
    let Some(data) = data else {
        log::debug!("no avatar image for {node} of {service}: {result:?}");
        return;
    };
    match store_avatar_image(ctx.store, ctx.account_id, &s, node, hash, &data) {
        Ok(true) => changed(ctx, &s, node),
        Ok(false) => {}
        Err(e) => log::warn!("avatar image for {node} of {service}: {e}"),
    }
}

fn remove_item(ctx: &mut Ctx<'_>, service: &str, node: &str, id: &str) {
    let conn = ctx.store.conn();
    let result = db::delete_item(conn, ctx.account_id, service, node, id).and_then(|_| {
        if id == AVATAR_ITEM {
            avatars::remove(ctx.store, ctx.account_id, &avatar_owner(service, node)).map(|_| ())
        } else {
            Ok(())
        }
    });
    if let Err(e) = result {
        ctx.store_error("remove a space item", e);
    }
}

fn remove_space(ctx: &mut Ctx<'_>, service: &str, node: &str) {
    let result = db::delete_space(ctx.store.conn(), ctx.account_id, service, node).and_then(|_| {
        avatars::remove(ctx.store, ctx.account_id, &avatar_owner(service, node)).map(|_| ())
    });
    if let Err(e) = result {
        ctx.store_error("remove a space", e);
    }
    changed(ctx, service, node);
}

/// Mark the views of a space as changed.
fn changed(ctx: &mut Ctx<'_>, service: &str, node: &str) {
    ctx.changed(ViewKey::SpaceList);
    ctx.changed(ViewKey::ChannelList(ChannelScope::Space {
        service: service.to_owned(),
        node: node.to_owned(),
    }));
    // A room that enters or leaves a space also enters or leaves Home.
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
}

// --- Events ---

/// A pubsub event from `service`, for example a change to a space node. Only the service
/// of a space that we follow can change it: any other sender is dropped.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, service: &Jid, payload: Payload) {
    let node = pubsub::node_of(&payload).to_owned();
    if let Payload::Subscription {
        jid,
        subscription,
        subid,
        ..
    } = payload
    {
        let subid = subid.map(|id| id.0);
        return on_subscription_event(ctx, service, &node, jid, subscription, subid);
    }
    let s = service.to_string();
    match db::is_followed(ctx.store.conn(), ctx.account_id, &s, &node) {
        Ok(true) => {}
        Ok(false) => {
            log::warn!(
                "dropped a pubsub event from {service} for node {node}: not a space that we follow"
            );
            return;
        }
        Err(e) => return ctx.store_error("read a space", e),
    }
    let bare = service.to_bare();
    match payload {
        Payload::Items {
            published,
            retracted,
            ..
        } => {
            for item in published {
                if let Some(id) = item.id {
                    store_item(ctx, &s, &node, &id.0, item.payload.as_ref(), None);
                }
            }
            for id in retracted {
                remove_item(ctx, &s, &node, &id.0);
            }
            changed(ctx, &s, &node);
        }
        Payload::Purge { .. } => {
            let ids = db::item_ids(ctx.store.conn(), ctx.account_id, &s, &node).unwrap_or_default();
            for id in ids.iter().map(String::as_str).chain([AVATAR_ITEM]) {
                remove_item(ctx, &s, &node, id);
            }
            changed(ctx, &s, &node);
        }
        Payload::Delete { .. } => remove_space(ctx, &s, &node),
        Payload::Configuration { .. } => request_info(ctx, &bare, &node, After::Nothing),
        Payload::Subscription { .. } => {}
    }
}

#[cfg(test)]
mod tests;

// --- Store ---

pub(super) mod db {
    use super::*;

    pub struct Item<'a> {
        pub service: &'a str,
        pub node: &'a str,
        pub id: &'a str,
        pub room_jid: Option<&'a str>,
        pub name: Option<&'a str>,
        pub payload: Option<&'a str>,
    }

    pub fn upsert_space(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        name: Option<&str>,
        description: Option<&str>,
        access_model: Option<&str>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO spaces (account_id, service, node, name, description, access_model,
                                 subscribed, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1,
                     (SELECT COALESCE(MAX(position), -1) + 1 FROM spaces WHERE account_id = ?1))
             ON CONFLICT (account_id, service, node) DO UPDATE SET
                 name = excluded.name, description = excluded.description,
                 access_model = excluded.access_model, subscribed = 1",
            params![account_id, service, node, name, description, access_model],
        )?;
        Ok(())
    }

    /// Store a join that waits for the owner: `subscribed` is 2. A followed space stays.
    pub fn upsert_pending(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO spaces (account_id, service, node, subscribed, position)
             VALUES (?1, ?2, ?3, 2,
                     (SELECT COALESCE(MAX(position), -1) + 1 FROM spaces WHERE account_id = ?1))
             ON CONFLICT (account_id, service, node) DO NOTHING",
            params![account_id, service, node],
        )?;
        Ok(())
    }

    pub fn update_pending_meta(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        name: Option<&str>,
        description: Option<&str>,
        access_model: Option<&str>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "UPDATE spaces SET name = ?4, description = ?5, access_model = ?6
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND subscribed = 2",
            params![account_id, service, node, name, description, access_model],
        )?;
        Ok(())
    }

    pub fn is_pending(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM spaces
                            WHERE account_id = ?1 AND service = ?2 AND node = ?3
                              AND subscribed = 2)",
            params![account_id, service, node],
            |row| row.get(0),
        )
    }

    /// Store a join request. A request that we answered before is open again: the service
    /// asks again. A known subscription id stays if the new one is empty.
    pub fn add_request(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        jid: &str,
        subid: Option<&str>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO space_join_requests (account_id, service, node, jid, subid)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (account_id, service, node, jid) DO UPDATE SET
                 answered = 0, subid = COALESCE(excluded.subid, subid)",
            params![account_id, service, node, jid, subid],
        )?;
        Ok(())
    }

    /// A request that we answered, and that the service lists as pending again, is open
    /// again. A new subscription id replaces the old one. No row: nothing changes.
    pub fn reopen_request(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        jid: &str,
        subid: Option<&str>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "UPDATE space_join_requests SET answered = 0, subid = COALESCE(?5, subid)
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND jid = ?4",
            params![account_id, service, node, jid, subid],
        )?;
        Ok(())
    }

    /// The subscription id of a join request, if the service gave one.
    pub fn request_subid(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        jid: &str,
    ) -> rusqlite::Result<Option<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT subid FROM space_join_requests
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND jid = ?4",
        )?;
        let mut rows = stmt.query_map(params![account_id, service, node, jid], |row| row.get(0))?;
        Ok(rows.next().transpose()?.flatten())
    }

    /// Mark a request as answered. It stays until the subscription event arrives.
    pub fn answer_request(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        jid: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "UPDATE space_join_requests SET answered = 1
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND jid = ?4",
            params![account_id, service, node, jid],
        )?;
        Ok(())
    }

    /// Keep the subscription id of a space that we follow, or that waits for approval.
    /// Nothing changes if the space has no row. `None` keeps the id that we have.
    pub fn set_subid(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        subid: Option<&str>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "UPDATE spaces SET subid = COALESCE(?4, subid)
             WHERE account_id = ?1 AND service = ?2 AND node = ?3",
            params![account_id, service, node, subid],
        )?;
        Ok(())
    }

    pub fn subid(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<Option<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT subid FROM spaces WHERE account_id = ?1 AND service = ?2 AND node = ?3",
        )?;
        let mut rows = stmt.query_map(params![account_id, service, node], |row| row.get(0))?;
        Ok(rows.next().transpose()?.flatten())
    }

    /// Change the name or the description of a space that we follow. An empty description
    /// clears it. `None` keeps a value.
    pub fn update_meta(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> rusqlite::Result<()> {
        let description = description.map(|d| (!d.is_empty()).then_some(d));
        conn.execute(
            "UPDATE spaces SET name = COALESCE(?4, name),
                               description = CASE WHEN ?5 THEN ?6 ELSE description END
             WHERE account_id = ?1 AND service = ?2 AND node = ?3",
            params![
                account_id,
                service,
                node,
                name,
                description.is_some(),
                description.flatten()
            ],
        )?;
        Ok(())
    }

    /// The nodes of a service that an earlier start found to be no space.
    pub fn non_space_nodes(
        conn: &Connection,
        account_id: i64,
        service: &str,
    ) -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT node FROM non_space_nodes WHERE account_id = ?1 AND service = ?2",
        )?;
        stmt.query_map(params![account_id, service], |row| row.get(0))?
            .collect()
    }

    pub fn add_non_space(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT OR IGNORE INTO non_space_nodes (account_id, service, node)
             VALUES (?1, ?2, ?3)",
            params![account_id, service, node],
        )?;
        Ok(())
    }

    pub fn forget_non_space(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM non_space_nodes
             WHERE account_id = ?1 AND service = ?2 AND node = ?3",
            params![account_id, service, node],
        )?;
        Ok(())
    }

    pub fn remove_request(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        jid: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM space_join_requests
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND jid = ?4",
            params![account_id, service, node, jid],
        )?;
        Ok(())
    }

    /// The requests that wait for us. A request that we answered is not one of them.
    pub fn requests(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<Vec<JoinRequest>> {
        let mut stmt = conn.prepare_cached(
            "SELECT jid, subid FROM space_join_requests
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND answered = 0
             ORDER BY jid",
        )?;
        stmt.query_map(params![account_id, service, node], |row| {
            Ok(JoinRequest {
                jid: row.get(0)?,
                subid: row.get(1)?,
            })
        })?
        .collect()
    }

    pub fn pending(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<PendingJoin>> {
        let mut stmt = conn.prepare_cached(
            "SELECT service, node, COALESCE(name, node) FROM spaces
             WHERE account_id = ?1 AND subscribed = 2 ORDER BY position, node",
        )?;
        stmt.query_map(params![account_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect()
    }

    pub fn space_name(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<Option<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT name FROM spaces WHERE account_id = ?1 AND service = ?2 AND node = ?3",
        )?;
        let mut rows = stmt.query_map(params![account_id, service, node], |row| row.get(0))?;
        Ok(rows.next().transpose()?.flatten())
    }

    pub fn is_followed(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM spaces
                            WHERE account_id = ?1 AND service = ?2 AND node = ?3
                              AND subscribed = 1)",
            params![account_id, service, node],
            |row| row.get(0),
        )
    }

    /// The distinct pubsub services of the stored spaces.
    pub fn services(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT service FROM spaces WHERE account_id = ?1 ORDER BY service",
        )?;
        stmt.query_map(params![account_id], |row| row.get(0))?
            .collect()
    }

    pub fn followed_nodes(
        conn: &Connection,
        account_id: i64,
        service: &str,
    ) -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT node FROM spaces WHERE account_id = ?1 AND service = ?2 AND subscribed = 1",
        )?;
        stmt.query_map(params![account_id, service], |row| row.get(0))?
            .collect()
    }

    pub fn delete_space(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM space_items WHERE account_id = ?1 AND service = ?2 AND node = ?3",
            params![account_id, service, node],
        )?;
        conn.execute(
            "DELETE FROM spaces WHERE account_id = ?1 AND service = ?2 AND node = ?3",
            params![account_id, service, node],
        )?;
        Ok(())
    }

    /// Store an item. With no position, a new item goes last and an old one keeps its place.
    pub fn upsert_item(
        conn: &Connection,
        account_id: i64,
        item: &Item<'_>,
        position: Option<i64>,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO space_items (account_id, service, node, item_id, room_jid, name,
                                      position, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                     COALESCE(?7, (SELECT COALESCE(MAX(position), -1) + 1 FROM space_items
                                   WHERE account_id = ?1 AND service = ?2 AND node = ?3)),
                     ?8)
             ON CONFLICT (account_id, service, node, item_id) DO UPDATE SET
                 room_jid = excluded.room_jid, name = excluded.name,
                 payload = excluded.payload,
                 position = COALESCE(?7, space_items.position)",
            params![
                account_id,
                item.service,
                item.node,
                item.id,
                item.room_jid,
                item.name,
                position,
                item.payload
            ],
        )?;
        Ok(())
    }

    /// The JIDs of the rooms in a space.
    pub fn room_jids(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT room_jid FROM space_items
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND room_jid IS NOT NULL",
        )?;
        stmt.query_map(params![account_id, service, node], |row| row.get(0))?
            .collect()
    }

    pub fn item_ids(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
    ) -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare_cached(
            "SELECT item_id FROM space_items WHERE account_id = ?1 AND service = ?2 AND node = ?3",
        )?;
        stmt.query_map(params![account_id, service, node], |row| row.get(0))?
            .collect()
    }

    pub fn delete_item(
        conn: &Connection,
        account_id: i64,
        service: &str,
        node: &str,
        id: &str,
    ) -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM space_items
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 AND item_id = ?4",
            params![account_id, service, node, id],
        )?;
        Ok(())
    }
}
