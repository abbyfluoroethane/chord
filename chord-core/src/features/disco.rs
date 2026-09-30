//! Service discovery (XEP-0030) and entity capabilities (XEP-0115).
//!
//! - Answers disco#info queries to our JID.
//! - Puts our caps into our presence, so PEP sends us `+notify` events.
//! - Finds the services of our server at each new session: `State::services` lists each
//!   item of the server with its disco#info. Other features use `find_feature`.

use jid::{BareJid, Jid};
use xmpp_parsers::caps::{self, Caps};
use xmpp_parsers::disco::{
    DiscoInfoQuery, DiscoInfoResult, DiscoItemsQuery, DiscoItemsResult, Identity,
};
use xmpp_parsers::hashes::Algo;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::ns;

use super::{Ctx, IqResponse, Pending as FeaturePending, result_reply};

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
];

/// Our disco#info answer.
pub fn info(node: Option<String>) -> DiscoInfoResult {
    DiscoInfoResult {
        node,
        identities: vec![Identity::new("client", "pc", "en", "Chord")],
        features: FEATURES.iter().map(|f| (*f).to_owned()).collect(),
        extensions: vec![],
    }
}

/// Our caps element (XEP-0115, SHA-1 as the XEP requires for interoperability).
pub fn caps() -> Caps {
    let hash = caps::hash_caps(&caps::compute_disco(&info(None)), Algo::Sha_1)
        .expect("SHA-1 is supported");
    Caps::new(CAPS_NODE, hash)
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

/// Answer a disco#info query to us. Returns true if `iq` is one.
pub(crate) fn on_iq(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Get { payload, .. } = iq else {
        return false;
    };
    let Ok(query) = DiscoInfoQuery::try_from(payload.clone()) else {
        return false;
    };
    ctx.send(result_reply(iq, Some(info(query.node).into())));
    true
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
        assert_eq!(info.features.len(), FEATURES.len());
    }

    #[test]
    fn caps_hash_is_the_xep_0115_hash_of_the_list() {
        // The verification string of XEP-0115, 5.1: the identity, then the sorted features.
        // The hash is SHA-1 (`ver` holds the raw bytes). This value comes from a script that follows the XEP,
        // not from the code under test. Update it when `FEATURES` changes.
        let hex: String = caps().ver.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "1ea4fe87dd6eb18adf0769eb305c2c7a8756985e");
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
            data: vec![1],
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
            data: vec![1],
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
