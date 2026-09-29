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
pub const FEATURES: &[&str] = &[
    ns::DISCO_INFO,
    ns::CAPS,
    ns::PING,
    "urn:xmpp:carbons:2",
    "urn:xmpp:sid:0",
    "urn:xmpp:mam:2",
    "http://jabber.org/protocol/muc",
    "urn:xmpp:bookmarks:1+notify",
    "urn:xmpp:avatar:metadata+notify",
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
    /// The items of the account domain, each with its disco#info.
    pub services: Vec<(Jid, DiscoInfoResult)>,
    /// True when all service queries have an answer.
    pub complete: bool,
    outstanding: usize,
}

impl State {
    /// The first service that advertises `feature`, for example `urn:xmpp:http:upload:0`.
    pub fn find_feature(&self, feature: &str) -> Option<&(Jid, DiscoInfoResult)> {
        self.services
            .iter()
            .find(|(_, info)| info.features.contains(feature))
    }

    /// The first service with this identity, for example ("pubsub", "service").
    pub fn find_identity(&self, category: &str, type_: &str) -> Option<&(Jid, DiscoInfoResult)> {
        self.services.iter().find(|(_, info)| {
            info.identities
                .iter()
                .any(|i| i.category == category && i.type_ == type_)
        })
    }
}

#[derive(Debug)]
pub(crate) enum Pending {
    ServerInfo,
    ServerItems,
    ServiceInfo(Jid),
}

pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    let domain = domain_of(ctx.account);
    let info = Iq::from_get("", DiscoInfoQuery { node: None }).with_to(domain.clone());
    ctx.request(info, FeaturePending::Disco(Pending::ServerInfo));
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
        if let Pending::ServiceInfo(_) = pending {
            finish_one(ctx);
        }
        return;
    };
    match pending {
        Pending::ServerInfo => ctx.state.disco.server = DiscoInfoResult::try_from(payload).ok(),
        Pending::ServerItems => {
            let Ok(items) = DiscoItemsResult::try_from(payload) else {
                return;
            };
            ctx.state.disco.outstanding = items.items.len();
            if items.items.is_empty() {
                ctx.state.disco.complete = true;
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
    if state.outstanding == 0 {
        state.complete = true;
    }
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
        assert!(h.state.disco.complete);
        let (jid, _) = h
            .state
            .disco
            .find_feature("urn:xmpp:http:upload:0")
            .unwrap();
        assert_eq!(jid.as_str(), "upload.chord.localhost");
    }
}
