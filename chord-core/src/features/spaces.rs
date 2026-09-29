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
//!   the disco#info (name, description, access model) and for the items.
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
//!   `urn:xmpp:avatar:data` node of the service. An image at a URL needs an HTTP download,
//!   which the features cannot do. `store_avatar_image` takes such an image from the caller.
//! - Private spaces use the `whitelist` access model. The owner makes a member with
//!   `add_space_member`, then the member calls `join_space`. Join requests (`authorize`)
//!   only work where a server offers them. Prosody 13 does not.

use std::collections::{HashMap, HashSet, VecDeque};

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{Connection, params};
use xmpp_parsers::avatar::Data as AvatarData;
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::disco::{DiscoInfoQuery, DiscoInfoResult, DiscoItemsQuery};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::ns;
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::rsm::{SetQuery, SetResult};
use xmpp_parsers::stanza_error::StanzaError;

use super::avatars::{self, MAX_AVATAR_BYTES};
use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, new_id, pubsub};
use crate::actor::{ClientError, ClientHandle};
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

/// A space that `browse_spaces` found.
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceInfo {
    /// Pubsub service JID.
    pub service: String,
    pub node: String,
    pub name: String,
    pub description: Option<String>,
    pub access_model: Option<String>,
}

/// The result of `join_space`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    /// We are subscribed. The space is in the space list.
    Joined,
    /// The service holds a join request for the owner to approve.
    Pending,
}

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
    Join(Reply<JoinOutcome>),
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
    AddMember,
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
    Items {
        service: BareJid,
        node: String,
        join: Option<Reply<JoinOutcome>>,
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
        private: bool,
        reply: Reply<(String, String)>,
    },
    CreateSubscribe {
        service: BareJid,
        node: String,
        name: String,
        private: bool,
        reply: Reply<(String, String)>,
    },
    Done {
        action: Action,
        reply: Reply<()>,
    },
    /// The image of a space avatar, from the data node of the service.
    AvatarData {
        service: BareJid,
        node: String,
        hash: String,
    },
    /// The answer does not matter.
    Ignore,
}

/// A command from the public API.
pub(crate) enum Command {
    Browse {
        reply: Reply<Vec<SpaceInfo>>,
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
        private: bool,
        reply: Reply<(String, String)>,
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
}

impl ClientHandle {
    /// The public spaces of the pubsub service of our server.
    pub async fn browse_spaces(&self) -> Result<Vec<SpaceInfo>, ClientError> {
        self.space_call(|reply| Command::Browse { reply }).await
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
        let name = name.to_owned();
        self.space_call(|reply| Command::Create {
            name,
            private,
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
    let Some((jid, info)) = ctx.state.disco.find_identity("pubsub", "service") else {
        log::info!("the server has no pubsub service. Spaces are off.");
        return;
    };
    let service = jid.to_bare();
    for feature in REQUIRED_FEATURES {
        if !info
            .features
            .contains(&format!("{FEATURE_PREFIX}{feature}"))
        {
            log::warn!("pubsub service {service} does not advertise {feature} (XEP-0503)");
        }
    }
    ctx.state.spaces.started = true;
    ctx.state.spaces.service = Some(service.clone());
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

fn service_of(ctx: &Ctx<'_>) -> Result<BareJid, ClientError> {
    if let Some(service) = &ctx.state.spaces.service {
        return Ok(service.clone());
    }
    ctx.state
        .disco
        .find_identity("pubsub", "service")
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

fn unsubscribe_iq(ctx: &Ctx<'_>, service: &BareJid, node: &str) -> Result<Iq, ClientError> {
    pubsub_iq(
        true,
        service,
        &format!(
            "<unsubscribe node='{}' jid='{}'/>",
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

fn items_iq(service: &BareJid, node: &str) -> Result<Iq, ClientError> {
    pubsub_iq(
        false,
        service,
        &format!("<items node='{}'/>", xml_escape(node)),
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
    let pending = Pending::Items {
        service: service.clone(),
        node: node.to_owned(),
        join,
    };
    match items_iq(service, node) {
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
            let iq = unsubscribe_iq(ctx, &service, &node);
            go(ctx, iq, reply, |reply| Pending::Done {
                action: Action::Leave { service, node },
                reply,
            });
        }
        Command::Create {
            name,
            private,
            reply,
        } => match service_of(ctx) {
            Ok(service) => {
                let node = format!("space-{}", &new_id().replace('-', "")[..12]);
                let iq = pubsub_iq(
                    true,
                    &service,
                    &format!(
                        "<create node='{}'/><configure>{}</configure>",
                        xml_escape(&node),
                        node_config(&name, private)
                    ),
                );
                go(ctx, iq, reply, |reply| Pending::Create {
                    service,
                    node,
                    name,
                    private,
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
                action: Action::AddMember,
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
    }
}

/// The node configuration form of a new space (XEP-0503, "Space Node Configuration").
fn node_config(name: &str, private: bool) -> String {
    let access = if private { "whitelist" } else { "open" };
    let text = |var: &str, value: &str| Field::new(var, FieldType::TextSingle).with_value(value);
    let boolean = |var: &str, value: &str| Field::new(var, FieldType::Boolean).with_value(value);
    let fields = vec![
        text("pubsub#type", NS_SPACES),
        text("pubsub#title", name),
        Field::new("pubsub#access_model", FieldType::ListSingle).with_value(access),
        boolean("pubsub#persist_items", "1"),
        boolean("pubsub#purge_offline", "0"),
        boolean("pubsub#notify_retract", "1"),
        boolean("pubsub#notify_sub", "1"),
        boolean("pubsub#notify_config", "1"),
        boolean("pubsub#notify_delete", "1"),
        text("pubsub#max_items", MAX_ITEMS),
    ];
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
        Command::Join { reply, .. } => no(reply),
        Command::Leave { reply, .. } => no(reply),
        Command::Create { reply, .. } => no(reply),
        Command::AddRoom { reply, .. } => no(reply),
        Command::RemoveRoom { reply, .. } => no(reply),
        Command::AddMember { reply, .. } => no(reply),
        Command::Delete { reply, .. } => no(reply),
    }
}

// --- Answers ---

fn server_error(error: StanzaError) -> ClientError {
    let text = error.texts.values().next().cloned().unwrap_or_default();
    let message = format!("{:?} {text}", error.defined_condition);
    ClientError::Server(message.trim().to_owned())
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
        } => {
            let lost = matches!(result, Err(ClientError::NotConnected));
            match &result {
                Ok(Some(payload)) => {
                    let items = parse_items(payload);
                    sync_items(ctx, &service, &node, items);
                }
                other => log::warn!("items of {service} {node}: {other:?}"),
            }
            if let Some(reply) = join {
                let _ = reply.send(if lost {
                    Err(ClientError::NotConnected)
                } else {
                    Ok(JoinOutcome::Joined)
                });
            }
        }
        Pending::BrowseItems { browse } => on_browse_page(ctx, browse, result),
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
                let state = payload
                    .get_child("subscription", ns::PUBSUB)
                    .and_then(|s| s.attr("subscription"))
                    .unwrap_or("");
                match state {
                    "subscribed" => request_info(ctx, &service, &node, After::Join(reply)),
                    "pending" => {
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
            private,
            reply,
        } => match result {
            Ok(_) => {
                let iq = subscribe_iq(ctx, &service, &node);
                go(ctx, iq, reply, |reply| Pending::CreateSubscribe {
                    service,
                    node,
                    name,
                    private,
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
            private,
            reply,
        } => match result {
            Ok(_) => {
                let access = if private { "whitelist" } else { "open" };
                let s = service.to_string();
                if let Err(e) = db::upsert_space(
                    ctx.store.conn(),
                    ctx.account_id,
                    &s,
                    &node,
                    Some(&name),
                    None,
                    Some(access),
                ) {
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
        Action::AddMember => {}
    }
}

/// The user subscriptions of the service: keep the spaces that we follow.
fn on_subscriptions(ctx: &mut Ctx<'_>, service: &BareJid, payload: &Element) {
    let Some(list) = payload.get_child("subscriptions", ns::PUBSUB) else {
        return;
    };
    let mut nodes = Vec::new();
    for sub in list.children().filter(|c| c.is("subscription", ns::PUBSUB)) {
        let ours = match sub.attr("jid").map(BareJid::new) {
            Some(Ok(jid)) => jid == *ctx.account,
            Some(Err(_)) => false,
            None => true,
        };
        if let (true, Some("subscribed"), Some(node)) =
            (ours, sub.attr("subscription"), sub.attr("node"))
        {
            nodes.push(node.to_owned());
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
    for node in nodes {
        request_info(ctx, service, &node, After::Items);
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
    match (info, &after) {
        (Some(info), _) => {
            let meta = meta_of(&info);
            if meta.type_.as_deref() != Some(NS_SPACES) {
                log::info!("node {node} of {service} is not a space");
                if let After::Join(reply) = after {
                    let _ = reply.send(Err(ClientError::Invalid("the node is not a space".into())));
                    if let Ok(iq) = unsubscribe_iq(ctx, service, node) {
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
        }
        (None, After::Join(_)) if !lost => {
            // We are subscribed already. Keep the space without its metadata.
            log::warn!("no disco#info for {node} of {service}: {result:?}");
            if let Err(e) =
                db::upsert_space(ctx.store.conn(), ctx.account_id, &s, node, None, None, None)
            {
                ctx.store_error("store a space", e);
            }
        }
        (None, _) => {
            log::warn!("no disco#info for {node} of {service}: {result:?}");
            if let After::Join(reply) = after {
                let _ = reply.send(Err(ClientError::NotConnected));
            }
            return;
        }
    }
    changed(ctx, &s, node);
    match after {
        After::Nothing => {}
        After::Items => request_items(ctx, service, node, None),
        After::Join(reply) => request_items(ctx, service, node, Some(reply)),
    }
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

/// Browse lists open spaces only.
fn is_open_space(meta: &Meta) -> bool {
    meta.type_.as_deref() == Some(NS_SPACES) && meta.access_model.as_deref() == Some("open")
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
    if info.url.is_some() {
        log::debug!("the avatar of {owner} is at a URL. Not fetched.");
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

mod db {
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
