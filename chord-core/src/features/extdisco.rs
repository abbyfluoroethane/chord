//! External service discovery (XEP-0215): STUN and TURN servers for calls.
//!
//! The server advertises `urn:xmpp:extdisco:2`. When discovery finishes, this feature
//! asks the server for its services. A TURN service can be restricted: the server hands
//! out a username, a password, and an expiry time. The core keeps the list and asks
//! again before the first credentials expire. The list stays in memory only. Credentials
//! are short-lived secrets, so they never go to the store or to the log.
//!
//! The vendored `xmpp_parsers::extdisco::Service` has private fields, so this module
//! reads the elements itself.

use futures_channel::oneshot;
use jid::Jid;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, disco};
use crate::actor::{ClientError, ClientHandle};

pub const NS_EXTDISCO: &str = "urn:xmpp:extdisco:2";

/// Ask again this long before the first credentials expire. Ticks come every 15 s, so the
/// margin must be larger than that.
const REFRESH_MARGIN_MS: i64 = 60_000;

type Reply = oneshot::Sender<Result<Vec<IceServer>, ClientError>>;

/// The kind of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum IceKind {
    Stun,
    Turn,
    /// The XEP lets a server advertise other types (for example `stuns` or `turns`).
    /// A client that cannot use one skips it.
    Other,
}

/// A STUN or TURN server, as a WebRTC stack wants it.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct IceServer {
    pub kind: IceKind,
    /// The name of the type in the stanza, for example `turn`.
    pub type_name: String,
    /// A domain name or an IP address.
    pub host: String,
    pub port: Option<u16>,
    /// `udp` or `tcp`, when the server says.
    pub transport: Option<String>,
    /// The server needs credentials for this service.
    pub restricted: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    /// When the credentials expire, in Unix ms. `None` for a service with no expiry.
    pub expires_ms: Option<i64>,
}

impl IceServer {
    /// A URI for a WebRTC `RTCIceServer.urls` entry, for example `turn:host:3478?transport=udp`
    /// (RFC 7064 and RFC 7065). `None` for a type that has no URI scheme.
    pub fn uri(&self) -> Option<String> {
        let scheme = match self.type_name.as_str() {
            "stun" | "stuns" | "turn" | "turns" => self.type_name.as_str(),
            _ => return None,
        };
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let mut uri = format!("{scheme}:{host}");
        if let Some(port) = self.port {
            uri.push_str(&format!(":{port}"));
        }
        // RFC 7064 allows no transport parameter on a STUN URI.
        if matches!(self.kind, IceKind::Turn)
            && let Some(transport) = &self.transport
        {
            uri.push_str(&format!("?transport={transport}"));
        }
        Some(uri)
    }

    fn same_service(&self, other: &Self) -> bool {
        self.type_name == other.type_name
            && self.host == other.host
            && self.port == other.port
            && self.transport == other.transport
    }
}

// The password is a secret, so it stays out of debug output and logs.
impl core::fmt::Debug for IceServer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IceServer")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("transport", &self.transport)
            .field("restricted", &self.restricted)
            .field("username", &self.username.as_ref().map(|_| "<set>"))
            .field("password", &self.password.as_ref().map(|_| "<set>"))
            .field("expires_ms", &self.expires_ms)
            .finish()
    }
}

/// The action of a service element in a push (XEP-0215, section 4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Add,
    Remove,
    Modify,
}

/// Read one `<service/>` element. Returns `None` for an element with no `host` or `type`.
fn parse_service(el: &Element) -> Option<(Action, IceServer)> {
    if !el.is("service", NS_EXTDISCO) {
        return None;
    }
    let host = el.attr("host")?.to_owned();
    let type_name = el.attr("type")?.to_owned();
    let kind = match type_name.as_str() {
        "stun" => IceKind::Stun,
        "turn" => IceKind::Turn,
        _ => IceKind::Other,
    };
    let action = match el.attr("action") {
        // The example of the XEP says `delete`, and the schema says `remove`.
        Some("remove" | "delete") => Action::Remove,
        Some("modify") => Action::Modify,
        _ => Action::Add,
    };
    let restricted = matches!(el.attr("restricted"), Some("true" | "1"));
    let expires_ms = el
        .attr("expires")
        .and_then(|s| s.parse::<xmpp_parsers::date::DateTime>().ok())
        .map(|d| d.0.timestamp_millis());
    Some((
        action,
        IceServer {
            kind,
            type_name,
            host,
            port: el.attr("port").and_then(|p| p.parse().ok()),
            transport: el.attr("transport").map(str::to_owned),
            restricted,
            username: el.attr("username").map(str::to_owned),
            password: el.attr("password").map(str::to_owned),
            expires_ms,
        },
    ))
}

/// The services in a `<services/>` or `<credentials/>` element.
fn parse_services(el: &Element) -> Vec<(Action, IceServer)> {
    el.children().filter_map(parse_service).collect()
}

#[derive(Debug, Default)]
pub(crate) struct State {
    /// The server supports XEP-0215. Set when discovery finishes.
    supported: bool,
    services: Vec<IceServer>,
    /// The list has an answer from the server for this session.
    loaded: bool,
    /// A `services` or `credentials` query waits for its answer.
    in_flight: bool,
    /// The last error. After a failure the core asks again only when a caller asks.
    last_error: Option<String>,
    /// Callers that wait for the list.
    waiters: Vec<Reply>,
}

#[derive(Debug)]
pub(crate) enum Pending {
    Services,
    Credentials,
}

/// A command from the public API.
pub(crate) enum Command {
    List { reply: Reply },
}

impl ClientHandle {
    /// The STUN and TURN servers of our server (XEP-0215), with the credentials of the
    /// restricted ones. The core keeps the list and asks again before the credentials
    /// expire, so the answer has credentials that are valid now. The command waits for
    /// service discovery. It fails with `Unsupported` when the server does not advertise
    /// `urn:xmpp:extdisco:2`, and with `Server` when the server refuses.
    pub async fn ice_servers(&self) -> Result<Vec<IceServer>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Extdisco(Command::List { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// Current Unix time in ms, from SQLite (the core avoids `SystemTime` for wasm).
fn now_ms(ctx: &Ctx<'_>) -> i64 {
    ctx.store
        .conn()
        .query_row(
            "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
}

/// The list is complete and no credentials expire soon.
fn is_fresh(ctx: &Ctx<'_>) -> bool {
    let state = &ctx.state.extdisco;
    if !state.loaded {
        return false;
    }
    let now = now_ms(ctx);
    state
        .services
        .iter()
        .filter_map(|s| s.expires_ms)
        .all(|expires| expires - REFRESH_MARGIN_MS > now)
}

/// Discovery finished. Load the list if the server supports XEP-0215.
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    ctx.state.extdisco.supported = ctx.state.disco.server_has(NS_EXTDISCO);
    if ctx.state.extdisco.supported {
        fetch(ctx);
    }
}

fn server_jid(ctx: &Ctx<'_>) -> Jid {
    disco::domain_of(ctx.account)
}

fn fetch(ctx: &mut Ctx<'_>) {
    if ctx.state.extdisco.in_flight {
        return;
    }
    ctx.state.extdisco.in_flight = true;
    let payload = Element::builder("services", NS_EXTDISCO).build();
    ctx.request(
        get(server_jid(ctx), payload),
        FeaturePending::Extdisco(Pending::Services),
    );
}

/// Ask for the credentials of the restricted services that came with none (XEP-0215,
/// section 4.2). One query lists them.
fn fetch_credentials(ctx: &mut Ctx<'_>, missing: &[IceServer]) {
    let mut credentials = Element::builder("credentials", NS_EXTDISCO);
    for s in missing {
        let mut service = Element::builder("service", NS_EXTDISCO)
            .attr(nc("host"), s.host.as_str())
            .attr(nc("type"), s.type_name.as_str());
        if let Some(port) = s.port {
            service = service.attr(nc("port"), port.to_string());
        }
        if let Some(transport) = &s.transport {
            service = service.attr(nc("transport"), transport.as_str());
        }
        credentials = credentials.append(service.build());
    }
    ctx.state.extdisco.in_flight = true;
    ctx.request(
        get(server_jid(ctx), credentials.build()),
        FeaturePending::Extdisco(Pending::Credentials),
    );
}

fn get(to: Jid, payload: Element) -> Iq {
    Iq::Get {
        from: None,
        to: Some(to),
        id: String::new(),
        payload,
    }
}

/// An attribute name. Each caller passes a literal that is a valid XML name.
fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    let Command::List { reply } = command;
    if !ctx.state.disco.complete {
        // `features::on_command` defers this command until discovery is complete.
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
    if !ctx.state.extdisco.supported {
        let e = ClientError::Unsupported("the server has no external service discovery".into());
        let _ = reply.send(Err(e));
        return;
    }
    if is_fresh(ctx) {
        let _ = reply.send(Ok(ctx.state.extdisco.services.clone()));
        return;
    }
    ctx.state.extdisco.waiters.push(reply);
    fetch(ctx);
}

/// A command while no session is up.
pub(crate) fn offline(command: Command) {
    let Command::List { reply } = command;
    let _ = reply.send(Err(ClientError::NotConnected));
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    ctx.state.extdisco.in_flight = false;
    let outcome = match response {
        IqResponse::Result(payload) => Ok(payload),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    let payload = match outcome {
        Ok(payload) => payload,
        Err(e) => {
            log::warn!("external service discovery failed: {e}");
            ctx.state.extdisco.last_error = Some(e.to_string());
            answer_waiters(ctx, Err(e));
            return;
        }
    };
    ctx.state.extdisco.last_error = None;
    let received: Vec<IceServer> = payload
        .as_ref()
        .map(parse_services)
        .unwrap_or_default()
        .into_iter()
        .filter(|(action, _)| *action != Action::Remove)
        .map(|(_, s)| s)
        .collect();
    match pending {
        Pending::Services => ctx.state.extdisco.services = received,
        Pending::Credentials => merge_credentials(&mut ctx.state.extdisco.services, received),
    }
    ctx.state.extdisco.loaded = true;
    // A restricted service with no credentials needs a second query. Ask once: if the
    // answer still has none, the server offers none and the service stays without.
    if matches!(pending, Pending::Services) {
        let missing: Vec<IceServer> = ctx
            .state
            .extdisco
            .services
            .iter()
            .filter(|s| s.restricted && s.password.is_none())
            .cloned()
            .collect();
        if !missing.is_empty() {
            fetch_credentials(ctx, &missing);
            return;
        }
    }
    let list = ctx.state.extdisco.services.clone();
    answer_waiters(ctx, Ok(list));
}

/// Put the credentials of `received` into the matching services.
fn merge_credentials(services: &mut [IceServer], received: Vec<IceServer>) {
    for new in received {
        if let Some(old) = services.iter_mut().find(|s| s.same_service(&new)) {
            old.username = new.username;
            old.password = new.password;
            old.expires_ms = new.expires_ms;
        }
    }
}

fn answer_waiters(ctx: &mut Ctx<'_>, result: Result<Vec<IceServer>, ClientError>) {
    for reply in std::mem::take(&mut ctx.state.extdisco.waiters) {
        let _ = reply.send(result.clone());
    }
}

/// A `<services/>` push from the server (XEP-0215, section 4.4). It is an IQ set. It adds,
/// changes, or removes services, and we answer with a result. Returns true if `iq` is one.
/// Only our server may send it.
pub(crate) fn on_iq(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Set { payload, from, .. } = iq else {
        return false;
    };
    if !payload.is("services", NS_EXTDISCO) {
        return false;
    }
    let from_server = from.as_ref().is_none_or(|from| *from == server_jid(ctx));
    if !from_server || !ctx.state.extdisco.supported {
        log::warn!("dropped an extdisco push from {from:?}");
        let error = StanzaError::new(
            xmpp_parsers::stanza_error::ErrorType::Auth,
            xmpp_parsers::stanza_error::DefinedCondition::Forbidden,
            "en",
            "only the server may push services",
        );
        ctx.send(super::error_reply(iq, error));
        return true;
    }
    for (action, service) in parse_services(payload) {
        let list = &mut ctx.state.extdisco.services;
        list.retain(|s| !s.same_service(&service));
        if action != Action::Remove {
            list.push(service);
        }
    }
    ctx.state.extdisco.loaded = true;
    ctx.send(super::result_reply(iq, None));
    true
}

/// A session tick. Ask again when credentials expire soon and nobody asked yet.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    let state = &ctx.state.extdisco;
    // After a failure the list stays as it is until the next `ice_servers` call.
    if !state.supported || state.in_flight || state.last_error.is_some() || !state.loaded {
        return;
    }
    if !is_fresh(ctx) {
        fetch(ctx);
    }
}

fn describe(error: &StanzaError) -> String {
    let text = error.texts.values().next().map(String::as_str);
    match text {
        Some(text) => format!("{:?}: {text}", error.defined_condition),
        None => format!("{:?}", error.defined_condition),
    }
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType};

    use super::*;
    use crate::features::testing::Harness;
    use crate::features::{on_command, on_tick as features_tick};

    /// The answer of XEP-0215, example 2 (with credentials and an expiry added to TURN).
    const SERVICES: &str = "<services xmlns='urn:xmpp:extdisco:2'>\
        <service host='stun.shakespeare.lit' port='9998' transport='udp' type='stun'/>\
        <service host='turn.shakespeare.lit' port='9999' transport='udp' type='turn' \
         restricted='1' username='user' password='pass' expires='2099-01-01T00:00:00Z'/>\
        </services>";

    fn harness(supported: bool) -> Harness {
        let mut h = Harness::new();
        let mut info = disco::info(None);
        if supported {
            info.features.insert(NS_EXTDISCO.into());
        }
        h.state.disco.server = Some(info);
        h.state.disco.complete = true;
        h.with_ctx(on_services_ready);
        h
    }

    fn respond(h: &mut Harness, response: IqResponse) {
        h.respond(|p| matches!(p, FeaturePending::Extdisco(_)), response);
    }

    fn ask(h: &mut Harness) -> oneshot::Receiver<Result<Vec<IceServer>, ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, FeatureCommand::Extdisco(Command::List { reply })));
        answer
    }

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    #[test]
    fn parses_the_example_of_the_xep() {
        let parsed = parse_services(&el(SERVICES));
        assert_eq!(parsed.len(), 2);
        let stun = &parsed[0].1;
        assert_eq!(stun.kind, IceKind::Stun);
        assert_eq!(stun.host, "stun.shakespeare.lit");
        assert_eq!(stun.port, Some(9998));
        assert_eq!(stun.transport.as_deref(), Some("udp"));
        assert!(!stun.restricted);
        assert_eq!(stun.uri().unwrap(), "stun:stun.shakespeare.lit:9998");
        let turn = &parsed[1].1;
        assert_eq!(turn.kind, IceKind::Turn);
        assert!(turn.restricted);
        assert_eq!(turn.username.as_deref(), Some("user"));
        assert_eq!(turn.password.as_deref(), Some("pass"));
        assert_eq!(turn.expires_ms, Some(4_070_908_800_000));
        assert_eq!(
            turn.uri().unwrap(),
            "turn:turn.shakespeare.lit:9999?transport=udp"
        );
    }

    #[test]
    fn debug_output_hides_the_password() {
        let turn = &parse_services(&el(SERVICES))[1].1;
        let text = format!("{turn:?}");
        assert!(!text.contains("\"pass\""));
        assert!(!text.contains("\"user\""));
    }

    #[test]
    fn skips_a_service_with_no_host_and_reads_other_types() {
        let xml = "<services xmlns='urn:xmpp:extdisco:2'>\
            <service type='stun'/>\
            <service host='::1' port='5349' type='turns' transport='tcp'/></services>";
        let parsed = parse_services(&el(xml));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].1.kind, IceKind::Other);
        assert_eq!(parsed[0].1.uri().unwrap(), "turns:[::1]:5349");
    }

    #[test]
    fn asks_the_server_when_discovery_finishes() {
        let mut h = harness(true);
        let iqs = h.sent_iqs();
        assert_eq!(iqs.len(), 1);
        let Iq::Get { to, payload, .. } = &iqs[0] else {
            panic!("not a get");
        };
        assert_eq!(to.as_ref().unwrap().to_string(), "chord.localhost");
        assert!(payload.is("services", NS_EXTDISCO));
    }

    #[test]
    fn sends_nothing_for_a_server_without_support() {
        let mut h = harness(false);
        assert!(h.sent_iqs().is_empty());
        let mut answer = ask(&mut h);
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Unsupported(_))
        ));
    }

    #[test]
    fn a_caller_waits_for_the_answer() {
        let mut h = harness(true);
        let mut answer = ask(&mut h);
        assert!(answer.try_recv().unwrap().is_none());
        respond(&mut h, IqResponse::Result(Some(el(SERVICES))));
        let list = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(list.len(), 2);
        // The list is fresh now: the next call answers at once, with no new query.
        h.take_sent();
        let mut again = ask(&mut h);
        assert_eq!(again.try_recv().unwrap().unwrap().unwrap().len(), 2);
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn an_error_reaches_the_caller() {
        let mut h = harness(true);
        let mut answer = ask(&mut h);
        let error = StanzaError::new(
            ErrorType::Auth,
            DefinedCondition::Forbidden,
            "en",
            "not for you",
        );
        respond(&mut h, IqResponse::Error(error));
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Server(text)) => assert!(text.contains("Forbidden")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_restricted_service_with_no_credentials_gets_a_second_query() {
        let mut h = harness(true);
        let mut answer = ask(&mut h);
        let xml = "<services xmlns='urn:xmpp:extdisco:2'>\
            <service host='turn.example' port='3478' transport='udp' type='turn' restricted='true'/>\
            </services>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        assert!(answer.try_recv().unwrap().is_none());
        let iqs = h.sent_iqs();
        let Iq::Get { payload, .. } = iqs.last().unwrap() else {
            panic!("not a get");
        };
        assert!(payload.is("credentials", NS_EXTDISCO));
        let creds = "<credentials xmlns='urn:xmpp:extdisco:2'>\
            <service host='turn.example' port='3478' transport='udp' type='turn' \
             username='u' password='p' expires='2099-01-01T00:00:00Z'/></credentials>";
        respond(&mut h, IqResponse::Result(Some(el(creds))));
        let list = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(list[0].password.as_deref(), Some("p"));
        assert!(list[0].restricted);
    }

    #[test]
    fn credentials_near_expiry_are_asked_for_again_on_a_tick() {
        let mut h = harness(true);
        let xml = "<services xmlns='urn:xmpp:extdisco:2'>\
            <service host='turn.example' type='turn' restricted='1' username='u' password='p' \
             expires='2000-01-01T00:00:00Z'/></services>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        h.take_sent();
        h.with_ctx(features_tick);
        assert_eq!(h.sent_iqs().len(), 1);
        // A query is out, so a second tick asks nothing more.
        h.with_ctx(features_tick);
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_push_from_the_server_changes_the_list_and_others_are_refused() {
        let mut h = harness(true);
        respond(&mut h, IqResponse::Result(Some(el(SERVICES))));
        h.take_sent();
        let push = |from: &str, body: &str| Iq::Set {
            from: Some(Jid::new(from).unwrap()),
            to: None,
            id: "p1".into(),
            payload: el(body),
        };
        let remove = "<services xmlns='urn:xmpp:extdisco:2'>\
            <service action='delete' host='stun.shakespeare.lit' port='9998' transport='udp' type='stun'/>\
            </services>";
        // A stranger cannot change the list. It gets an error.
        assert!(h.with_ctx(|ctx| on_iq(ctx, &push("evil.example", remove))));
        assert_eq!(h.state.extdisco.services.len(), 2);
        assert!(matches!(h.sent_iqs()[0], Iq::Error { .. }));
        // The server can. It gets a result.
        assert!(h.with_ctx(|ctx| on_iq(ctx, &push("chord.localhost", remove))));
        assert_eq!(h.state.extdisco.services.len(), 1);
        assert!(matches!(h.sent_iqs()[0], Iq::Result { .. }));
    }
}
