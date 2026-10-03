//! Service discovery (XEP-0030) and entity capabilities (XEP-0115).
//!
//! - Answers disco#info queries to our JID: with no node, or with our caps node. Any other
//!   node gets item-not-found (XEP-0030, 3.2). A disco#items query gets an empty list.
//! - Puts our caps into our presence, so PEP sends us `+notify` events.
//! - Finds the services of our server at each new session: `State::services` lists each
//!   item of the server with its disco#info. Other features use `find_feature`.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use xmpp_parsers::caps::{self, Caps};
use xmpp_parsers::disco::{
    DiscoInfoQuery, DiscoInfoResult, DiscoItemsQuery, DiscoItemsResult, Identity,
};
use xmpp_parsers::hashes::Algo;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::ns;

use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use super::{
    Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, error_reply, result_reply,
};
use crate::actor::{ClientError, ClientHandle};

/// The caps node: a URI for the Chord client.
pub const CAPS_NODE: &str = "https://github.com/abbyfluoroethane/chord";

/// The protocol features that Chord supports. `+notify` asks PEP for events.
///
/// Add a namespace only when Chord sends it and handles it on receipt. Other clients use
/// this list to decide which buttons to show. One entry is left out on purpose:
/// - `urn:xmpp:mam:2` is for the entity that hosts an archive (XEP-0313, "Determining
///   support"). Chord queries the archive of the server and hosts none.
///
/// CSI (XEP-0352) is a stream feature that the server offers, so it is not here.
pub const FEATURES: &[&str] = &[
    ns::DISCO_INFO,
    ns::CAPS,
    ns::PING,
    "urn:xmpp:carbons:2",
    "urn:xmpp:sid:0",
    "http://jabber.org/protocol/muc",
    "urn:xmpp:bookmarks:1+notify",
    "urn:xmpp:avatar:metadata+notify",
    // Pinned messages, in a private PEP node (pins.rs).
    "urn:chord:pins:0+notify",
    // Tunes (XEP-0118): Chord shows what a contact plays. It publishes none.
    "http://jabber.org/protocol/tune+notify",
    // Message Displayed Synchronization (XEP-0490): we publish and read the node.
    "urn:xmpp:mds:displayed:0+notify",
    "http://jabber.org/protocol/chatstates",
    // Markers (XEP-0333): every message is markable, and `mark_read` sends displayed.
    "urn:xmpp:chat-markers:0",
    // Receipts (XEP-0184): a 1:1 message carries a request, and we answer the request of a
    // contact who sees our presence (markers.rs).
    "urn:xmpp:receipts",
    // Corrections (XEP-0308), retractions (XEP-0424), reactions (XEP-0444), replies
    // (XEP-0461) and their fallback (XEP-0428): sent and applied.
    "urn:xmpp:message-correct:0",
    "urn:xmpp:message-retract:1",
    "urn:xmpp:reactions:0",
    "urn:xmpp:reply:0",
    "urn:xmpp:fallback:0",
    // Out-of-band data (XEP-0066) for uploads, and processing hints (XEP-0334).
    "jabber:x:oob",
    "urn:xmpp:hints",
    // Direct invites (XEP-0249): muc.rs reads the invite and shows it.
    "jabber:x:conference",
    // Stateless file sharing (XEP-0447): sent with an upload, read from a message.
    "urn:xmpp:sfs:0",
];

/// The features that belong to the entity info that Chord shares: the software version
/// (XEP-0092) and the time (XEP-0202). The user can turn them off (`set_share_info`). Then
/// Chord answers neither query and does not list them.
pub const INFO_FEATURES: &[&str] = &["jabber:iq:version", "urn:xmpp:time"];

/// Our disco#info answer, with the entity info features.
pub fn info(node: Option<String>) -> DiscoInfoResult {
    info_with(node, true)
}

/// Our disco#info answer. `share_info` tells if the answer lists `INFO_FEATURES`.
pub fn info_with(node: Option<String>, share_info: bool) -> DiscoInfoResult {
    let extra = if share_info { INFO_FEATURES } else { &[] };
    DiscoInfoResult {
        node,
        identities: vec![Identity::new("client", "pc", "en", "Chord")],
        features: FEATURES
            .iter()
            .chain(extra)
            .map(|f| (*f).to_owned())
            .collect(),
        extensions: vec![],
    }
}

/// Our caps element (XEP-0115, SHA-1 as the XEP requires for interoperability).
pub fn caps() -> Caps {
    caps_with(true)
}

/// Our caps element. `share_info` is the same flag as in `info_with`.
pub fn caps_with(share_info: bool) -> Caps {
    let info = info_with(None, share_info);
    let hash =
        caps::hash_caps(&caps::compute_disco(&info), Algo::Sha_1).expect("SHA-1 is supported");
    Caps::new(CAPS_NODE, hash)
}

/// The disco node of our caps: the caps node, then `#`, then the verification string.
fn caps_node(share_info: bool) -> String {
    let element = xmpp_parsers::minidom::Element::from(caps_with(share_info));
    format!("{CAPS_NODE}#{}", element.attr("ver").unwrap_or_default())
}

/// What the server offers.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// disco#info of the account domain.
    pub server: Option<DiscoInfoResult>,
    /// disco#info of the account bare JID. A server advertises some account features
    /// here, for example XEP-0357 push.
    pub account: Option<DiscoInfoResult>,
    /// The items of the account domain, each with its disco#info.
    pub services: Vec<(Jid, DiscoInfoResult)>,
    /// True when all queries have an answer.
    pub complete: bool,
    /// The user turned off the version and time answers. It stays across sessions.
    pub hide_info: bool,
    /// Service queries that wait for an answer.
    outstanding: usize,
    /// disco#info queries of the domain and the account that wait for an answer.
    info_left: usize,
    /// True when the item list of the domain has an answer.
    items_done: bool,
}

impl State {
    /// The first service that advertises `feature`, for example `urn:xmpp:http:upload:0`.
    pub fn find_feature(&self, feature: &str) -> Option<&(Jid, DiscoInfoResult)> {
        self.services
            .iter()
            .find(|(_, info)| info.features.contains(feature))
    }

    /// Whether the server or the account advertises `feature`.
    pub fn server_has(&self, feature: &str) -> bool {
        [&self.server, &self.account]
            .into_iter()
            .flatten()
            .any(|info| info.features.contains(feature))
    }
}

/// A command from the public API.
pub(crate) enum Command {
    Features { reply: oneshot::Sender<Vec<String>> },
}

impl ClientHandle {
    /// The features that the server and the account advertise in disco#info, sorted, with no
    /// duplicates. The list is empty while the client is offline or before the answers come.
    pub async fn server_features(&self) -> Result<Vec<String>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Disco(Command::Features { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    let Command::Features { reply } = command;
    let _ = reply.send(server_feature_list(&ctx.state.disco));
}

pub(crate) fn offline(command: Command) {
    let Command::Features { reply } = command;
    let _ = reply.send(Vec::new());
}

/// The sorted union of the features of the server and of the account.
fn server_feature_list(state: &State) -> Vec<String> {
    let mut list: Vec<String> = [&state.server, &state.account]
        .into_iter()
        .flatten()
        .flat_map(|info| info.features.iter().cloned())
        .collect();
    list.sort_unstable();
    list.dedup();
    list
}

#[derive(Debug)]
pub(crate) enum Pending {
    ServerInfo,
    AccountInfo,
    ServerItems,
    ServiceInfo(Jid),
}

pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    let domain = domain_of(ctx.account);
    let info = Iq::from_get("", DiscoInfoQuery { node: None }).with_to(domain.clone());
    ctx.request(info, FeaturePending::Disco(Pending::ServerInfo));
    let account =
        Iq::from_get("", DiscoInfoQuery { node: None }).with_to(Jid::from(ctx.account.clone()));
    ctx.request(account, FeaturePending::Disco(Pending::AccountInfo));
    ctx.state.disco.info_left = 2;
    let items = Iq::from_get(
        "",
        DiscoItemsQuery {
            node: None,
            rsm: None,
        },
    )
    .with_to(domain);
    ctx.request(items, FeaturePending::Disco(Pending::ServerItems));
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let IqResponse::Result(Some(payload)) = response else {
        match pending {
            Pending::ServiceInfo(_) => finish_one(ctx),
            // With no item list, discovery has no services.
            Pending::ServerItems => {
                ctx.state.disco.items_done = true;
                try_complete(ctx);
            }
            Pending::ServerInfo | Pending::AccountInfo => finish_info(ctx),
        }
        return;
    };
    match pending {
        Pending::ServerInfo => {
            ctx.state.disco.server = DiscoInfoResult::try_from(payload).ok();
            finish_info(ctx);
        }
        Pending::AccountInfo => {
            ctx.state.disco.account = DiscoInfoResult::try_from(payload).ok();
            finish_info(ctx);
        }
        Pending::ServerItems => {
            ctx.state.disco.items_done = true;
            let Ok(items) = DiscoItemsResult::try_from(payload) else {
                try_complete(ctx);
                return;
            };
            ctx.state.disco.outstanding = items.items.len();
            if items.items.is_empty() {
                try_complete(ctx);
            }
            for item in items.items {
                let query =
                    Iq::from_get("", DiscoInfoQuery { node: None }).with_to(item.jid.clone());
                ctx.request(query, FeaturePending::Disco(Pending::ServiceInfo(item.jid)));
            }
        }
        Pending::ServiceInfo(jid) => {
            if let Ok(info) = DiscoInfoResult::try_from(payload) {
                ctx.state.disco.services.push((jid, info));
            }
            finish_one(ctx);
        }
    }
}

fn finish_one(ctx: &mut Ctx<'_>) {
    let state = &mut ctx.state.disco;
    state.outstanding = state.outstanding.saturating_sub(1);
    try_complete(ctx);
}

fn finish_info(ctx: &mut Ctx<'_>) {
    let state = &mut ctx.state.disco;
    state.info_left = state.info_left.saturating_sub(1);
    try_complete(ctx);
}

/// Mark discovery complete when the domain info, the account info, the item list, and
/// every service query have an answer.
fn try_complete(ctx: &mut Ctx<'_>) {
    let state = &ctx.state.disco;
    if state.items_done && state.info_left == 0 && state.outstanding == 0 {
        mark_complete(ctx);
    }
}

/// Discovery is done: every query has an answer, or the item list failed.
fn mark_complete(ctx: &mut Ctx<'_>) {
    if ctx.state.disco.complete {
        return;
    }
    ctx.state.disco.complete = true;
    // Spaces need the pubsub service, so they start now.
    super::spaces::on_disco_complete(ctx);
    // Commands that waited for the services run now.
    super::on_services_ready(ctx);
}

/// Answer a disco#info or disco#items query to us. Returns true if `iq` is one.
///
/// The node is empty or our caps node (`CAPS_NODE#ver`, XEP-0115, 6.2). Any other node
/// does not exist: item-not-found. We have no items: disco#items gets an empty list.
pub(crate) fn on_iq(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Get { payload, .. } = iq else {
        return false;
    };
    let share = !ctx.state.disco.hide_info;
    if let Ok(query) = DiscoInfoQuery::try_from(payload.clone()) {
        let answer = match known_node(query.node, share) {
            Some(node) => result_reply(iq, Some(info_with(node, share).into())),
            None => not_found(iq),
        };
        ctx.send(answer);
        return true;
    }
    if let Ok(query) = DiscoItemsQuery::try_from(payload.clone()) {
        let answer = match known_node(query.node, share) {
            Some(node) => {
                let items = DiscoItemsResult {
                    node,
                    items: vec![],
                    rsm: None,
                };
                result_reply(iq, Some(items.into()))
            }
            None => not_found(iq),
        };
        ctx.send(answer);
        return true;
    }
    false
}

/// The node of a query: `Some(None)` for no node or an empty one, `Some(Some(node))` for
/// our caps node, and `None` for any other node.
fn known_node(node: Option<String>, share_info: bool) -> Option<Option<String>> {
    match node.filter(|n| !n.is_empty()) {
        None => Some(None),
        Some(node) if node == caps_node(share_info) => Some(Some(node)),
        Some(_) => None,
    }
}

/// The answer for a node that does not exist (XEP-0030, 3.2).
fn not_found(iq: &Iq) -> Iq {
    let error = StanzaError::new(
        ErrorType::Cancel,
        DefinedCondition::ItemNotFound,
        "en",
        "no such node",
    );
    error_reply(iq, error)
}

/// The domain of a bare JID, as a JID.
pub(crate) fn domain_of(jid: &BareJid) -> Jid {
    Jid::from(BareJid::from_parts(None, jid.domain()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;

    #[test]
    fn the_server_feature_list_joins_server_and_account_with_no_duplicates() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Features { reply }));
        assert_eq!(answer.try_recv().unwrap().unwrap(), Vec::<String>::new());

        let with = |list: &[&str]| DiscoInfoResult {
            node: None,
            identities: vec![],
            features: list.iter().map(|f| (*f).to_owned()).collect(),
            extensions: vec![],
        };
        let server = with(&["urn:xmpp:mam:2", "urn:xmpp:push:0"]);
        let account = with(&["urn:xmpp:push:0", "urn:xmpp:carbons:2"]);
        h.state.disco.server = Some(server);
        h.state.disco.account = Some(account);
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Features { reply }));
        assert_eq!(
            answer.try_recv().unwrap().unwrap(),
            ["urn:xmpp:carbons:2", "urn:xmpp:mam:2", "urn:xmpp:push:0"]
        );
    }

    #[test]
    fn the_server_feature_list_is_empty_offline() {
        let (reply, mut answer) = oneshot::channel();
        offline(Command::Features { reply });
        assert!(answer.try_recv().unwrap().unwrap().is_empty());
    }

    #[test]
    fn caps_hash_is_stable_and_matches_info() {
        let a = caps();
        let b = caps();
        assert_eq!(a.ver, b.ver);
        assert_eq!(a.node, CAPS_NODE);
    }

    #[test]
    fn features_hold_what_chord_handles_and_no_more() {
        for f in [
            "urn:xmpp:chat-markers:0",
            "urn:xmpp:message-correct:0",
            "urn:xmpp:message-retract:1",
            "urn:xmpp:reactions:0",
            "urn:xmpp:reply:0",
            "urn:xmpp:fallback:0",
            "jabber:x:oob",
            "urn:xmpp:hints",
            "urn:xmpp:receipts",
            "jabber:x:conference",
            "urn:xmpp:sfs:0",
        ] {
            assert!(FEATURES.contains(&f), "{f}");
        }
        // Chord hosts no archive.
        assert!(!FEATURES.contains(&"urn:xmpp:mam:2"));
        // CSI is a stream feature.
        assert!(!FEATURES.iter().any(|f| f.contains("csi")));
        let mut sorted = FEATURES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), FEATURES.len(), "a namespace twice");
        let info = info(None);
        assert_eq!(info.features.len(), FEATURES.len() + INFO_FEATURES.len());
        assert!(INFO_FEATURES.iter().all(|f| !FEATURES.contains(f)));
        // Sharing off: the list has neither the version nor the time.
        let hidden = info_with(None, false);
        assert_eq!(hidden.features.len(), FEATURES.len());
        assert!(!hidden.features.contains("jabber:iq:version"));
        assert!(!hidden.features.contains("urn:xmpp:time"));
        assert_ne!(caps_with(false).ver, caps().ver);
    }

    #[test]
    fn caps_hash_is_the_xep_0115_hash_of_the_list() {
        // The verification string of XEP-0115, 5.1: the identity, then the sorted features.
        // The hash is SHA-1 (`ver` holds the raw bytes). This value comes from a script that follows the XEP,
        // not from the code under test. Update it when `FEATURES` or `INFO_FEATURES` changes.
        let hex = |caps: Caps| -> String { caps.ver.iter().map(|b| format!("{b:02x}")).collect() };
        // With the version and time features (the default).
        assert_eq!(hex(caps()), "2f1b1eb2201f6961b3f431ee785fc6bdb093e0dd");
        // With sharing off: `FEATURES` alone.
        assert_eq!(
            hex(caps_with(false)),
            "35236609ea3174a720ae8660d541b8401f2c16f6"
        );
    }

    /// Ask `query` of a new session and return its answer.
    fn ask(h: &mut Harness, payload: xmpp_parsers::minidom::Element) -> Iq {
        let mut query = Iq::Get {
            from: Some(Jid::new("bob@chord.localhost/x").unwrap()),
            to: None,
            id: "q1".into(),
            payload,
        };
        *query.id_mut() = "q1".into();
        assert!(h.with_ctx(|ctx| on_iq(ctx, &query)));
        h.sent_iqs().remove(0)
    }

    fn is_item_not_found(iq: &Iq) -> bool {
        matches!(iq, Iq::Error { error, .. } if error.defined_condition == DefinedCondition::ItemNotFound)
    }

    #[test]
    fn an_unknown_node_is_item_not_found_and_our_caps_node_is_known() {
        let mut h = Harness::new();
        let own = caps_node(true);
        assert!(own.starts_with(&format!("{CAPS_NODE}#")));
        // disco#info
        for node in [
            "urn:unknown",
            "http://jabber.org/protocol/commands",
            &format!("{CAPS_NODE}#wrong"),
        ] {
            let answer = ask(
                &mut h,
                DiscoInfoQuery {
                    node: Some(node.into()),
                }
                .into(),
            );
            assert!(is_item_not_found(&answer), "{node}: {answer:?}");
        }
        let answer = ask(
            &mut h,
            DiscoInfoQuery {
                node: Some(own.clone()),
            }
            .into(),
        );
        let Iq::Result {
            payload: Some(p), ..
        } = answer
        else {
            panic!()
        };
        let info = DiscoInfoResult::try_from(p).unwrap();
        assert_eq!(info.node.as_deref(), Some(own.as_str()));
        assert!(info.features.contains("urn:xmpp:ping"));
        // An empty node is no node.
        let answer = ask(
            &mut h,
            DiscoInfoQuery {
                node: Some(String::new()),
            }
            .into(),
        );
        assert!(matches!(answer, Iq::Result { .. }));
    }

    #[test]
    fn disco_items_gets_an_empty_list() {
        let mut h = Harness::new();
        let answer = ask(
            &mut h,
            DiscoItemsQuery {
                node: None,
                rsm: None,
            }
            .into(),
        );
        let Iq::Result {
            payload: Some(p), ..
        } = answer
        else {
            panic!("{answer:?}")
        };
        let items = DiscoItemsResult::try_from(p).unwrap();
        assert!(items.items.is_empty());
        assert_eq!(items.node, None);
        // Our caps node has no items. Any other node does not exist.
        let node = Some(caps_node(true));
        let answer = ask(&mut h, DiscoItemsQuery { node, rsm: None }.into());
        assert!(matches!(answer, Iq::Result { .. }));
        let node = Some("urn:unknown".to_owned());
        let answer = ask(&mut h, DiscoItemsQuery { node, rsm: None }.into());
        assert!(is_item_not_found(&answer));
    }

    #[test]
    fn with_sharing_off_the_caps_node_follows_the_new_hash() {
        let mut h = Harness::new();
        h.state.disco.hide_info = true;
        let hidden = caps_node(false);
        assert_ne!(hidden, caps_node(true));
        let answer = ask(&mut h, DiscoInfoQuery { node: Some(hidden) }.into());
        let Iq::Result {
            payload: Some(p), ..
        } = answer
        else {
            panic!()
        };
        let info = DiscoInfoResult::try_from(p).unwrap();
        assert!(!info.features.contains("urn:xmpp:time"));
        let answer = ask(
            &mut h,
            DiscoInfoQuery {
                node: Some(caps_node(true)),
            }
            .into(),
        );
        assert!(is_item_not_found(&answer));
    }

    #[test]
    fn disco_info_query_gets_our_features() {
        let mut h = Harness::new();
        let query = Iq::from_get("q1", DiscoInfoQuery { node: None })
            .with_to(Jid::new("alice@chord.localhost/chord").unwrap());
        let mut query = query;
        *query.from_mut() = Some(Jid::new("bob@chord.localhost/x").unwrap());
        assert!(h.with_ctx(|ctx| on_iq(ctx, &query)));
        let sent = h.sent_iqs();
        let Iq::Result {
            payload: Some(p),
            id,
            ..
        } = &sent[0]
        else {
            panic!("{sent:?}")
        };
        assert_eq!(id, "q1");
        let info = DiscoInfoResult::try_from(p.clone()).unwrap();
        assert!(info.features.contains("urn:xmpp:bookmarks:1+notify"));
    }

    #[test]
    fn service_commands_wait_for_discovery() {
        use crate::actor::ClientError;
        use crate::features::{FeatureCommand, on_command};
        use futures_channel::oneshot;

        let mut h = Harness::new();
        h.with_ctx(on_connected);
        let (reply, mut answer) = oneshot::channel();
        let command = FeatureCommand::Upload(crate::features::upload::Command::Upload {
            to: Jid::new("bob@chord.localhost").unwrap(),
            filename: "a.txt".into(),
            content_type: "text/plain".into(),
            source: crate::features::upload::Source::Memory(vec![1]),
            size: 1,
            reply,
        });
        h.with_ctx(|ctx| on_command(ctx, command));
        assert_eq!(
            answer.try_recv().unwrap(),
            None,
            "held until discovery finishes"
        );
        assert_eq!(h.state.deferred.len(), 1);

        // The item list fails: discovery ends with no services, and the command runs
        // when the domain info and the account info have an answer too.
        h.respond(
            |p| matches!(p, FeaturePending::Disco(Pending::ServerItems)),
            IqResponse::Lost,
        );
        h.respond(
            |p| matches!(p, FeaturePending::Disco(Pending::ServerInfo)),
            IqResponse::Lost,
        );
        assert!(
            !h.state.disco.complete,
            "the account info has no answer yet"
        );
        h.respond(
            |p| matches!(p, FeaturePending::Disco(Pending::AccountInfo)),
            IqResponse::Lost,
        );
        assert!(h.state.disco.complete);
        assert!(h.state.deferred.is_empty());
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Unsupported(_)))
        ));
    }

    #[test]
    fn a_command_before_connected_survives_the_session_reset() {
        use crate::features::{FeatureCommand, on_command};
        use futures_channel::oneshot;

        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        let command = FeatureCommand::Upload(crate::features::upload::Command::Upload {
            to: Jid::new("bob@chord.localhost").unwrap(),
            filename: "a.txt".into(),
            content_type: "text/plain".into(),
            source: crate::features::upload::Source::Memory(vec![1]),
            size: 1,
            reply,
        });
        // The command arrives after login, before `Connected`.
        h.with_ctx(|ctx| on_command(ctx, command));
        h.with_ctx(|ctx| crate::features::on_connected(ctx, false, &[]));
        assert_eq!(
            h.state.deferred.len(),
            1,
            "the reset keeps the waiting command"
        );
        assert_eq!(answer.try_recv().unwrap(), None);
    }

    #[test]
    fn services_are_discovered_one_by_one() {
        let mut h = Harness::new();
        h.with_ctx(on_connected);
        let items = DiscoItemsResult {
            node: None,
            items: vec![xmpp_parsers::disco::Item {
                jid: Jid::new("upload.chord.localhost").unwrap(),
                node: None,
                name: None,
            }],
            rsm: None,
        };
        h.answer(
            |p| matches!(p, FeaturePending::Disco(Pending::ServerItems)),
            Some(items.into()),
        );
        let mut info = info(None);
        info.features.insert("urn:xmpp:http:upload:0".into());
        h.answer(
            |p| matches!(p, FeaturePending::Disco(Pending::ServiceInfo(_))),
            Some(info.into()),
        );
        assert!(
            !h.state.disco.complete,
            "the info queries have no answer yet"
        );
        h.answer(
            |p| matches!(p, FeaturePending::Disco(Pending::ServerInfo)),
            Some(super::info(None).into()),
        );
        let mut account = super::info(None);
        account.features.insert("urn:xmpp:push:0".into());
        h.answer(
            |p| matches!(p, FeaturePending::Disco(Pending::AccountInfo)),
            Some(account.into()),
        );
        assert!(h.state.disco.complete);
        assert!(h.state.disco.server_has("urn:xmpp:push:0"));
        let (jid, _) = h
            .state
            .disco
            .find_feature("urn:xmpp:http:upload:0")
            .unwrap();
        assert_eq!(jid.as_str(), "upload.chord.localhost");
    }
}
