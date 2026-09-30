//! In-band registration before login (XEP-0077). Like `native.rs`, this module imports
//! `tokio_xmpp`: it opens a stream that is not logged in.
//!
//! `registration_form` connects, asks the server for its registration fields and closes the
//! stream. `register` connects again and submits the answer. Two short connections keep the
//! API simple for a UI that asks the user in between. A CAPTCHA challenge (XEP-0158) does
//! not tie to the stream: the server finds it by the `challenge` field of the form.
//!
//! tokio-xmpp 6.0.0 has no client for this, so the code uses its stream types directly,
//! the same as `login` in `native.rs`.

use core::fmt;
use core::time::Duration;

use futures_util::{SinkExt, StreamExt};
use jid::Jid;
use tokio_xmpp::connect::{DnsConfig, ServerConnector, StartTlsServerConnector};
use tokio_xmpp::xmlstream::{
    FallibleStreamElement, RecvFeaturesError, Timeouts, XmppStreamElement,
};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::ns;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stanza_error::StanzaError;

use super::ServerAddr;
use super::native::is_certificate_error;
use crate::features::register::{
    NS_REGISTER, RegistrationForm, RegistrationSubmission, parse_form, submission_query,
};

/// The default time limit for one call.
pub const DEFAULT_REGISTER_TIMEOUT: Duration = Duration::from_secs(20);

/// Why a registration call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// DNS, TCP, TLS, or the stream failed.
    Unreachable(String),
    /// The server certificate did not pass verification.
    TlsInvalid(String),
    Timeout,
    /// The server answered with an error. It is `not-allowed` or `service-unavailable`
    /// when registration is closed, `conflict` when the name is taken, `not-acceptable`
    /// for a wrong answer, and so on. The text names the condition.
    Refused(String),
    /// The answer is not what the protocol says.
    Protocol(String),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(msg) => write!(f, "server unreachable: {msg}"),
            Self::TlsInvalid(msg) => write!(f, "server certificate invalid: {msg}"),
            Self::Timeout => f.write_str("registration timed out"),
            Self::Refused(msg) => write!(f, "the server refused: {msg}"),
            Self::Protocol(msg) => write!(f, "unexpected answer: {msg}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Ask the server of `domain` for its registration fields. Nothing changes on the server.
pub async fn registration_form(
    domain: &str,
    server: ServerAddr,
    timeout: Duration,
) -> Result<RegistrationForm, RegisterError> {
    let iq = Iq::Get {
        from: None,
        to: None,
        id: REQUEST_ID.to_owned(),
        payload: Element::builder("query", NS_REGISTER).build(),
    };
    let query = run(domain, server, iq, timeout).await?;
    let query = query
        .filter(|q| q.is("query", NS_REGISTER))
        .ok_or_else(|| RegisterError::Protocol("the answer has no registration query".into()))?;
    Ok(parse_form(&query))
}

/// Create the account. `submission` holds the legacy fields (`username`, `password` ...) or
/// the filled data form. Returns when the server says that the account exists.
pub async fn register(
    domain: &str,
    server: ServerAddr,
    submission: &RegistrationSubmission,
    timeout: Duration,
) -> Result<(), RegisterError> {
    let iq = Iq::Set {
        from: None,
        to: None,
        id: REQUEST_ID.to_owned(),
        payload: submission_query(submission),
    };
    run(domain, server, iq, timeout).await.map(drop)
}

const REQUEST_ID: &str = "chord-register";

async fn run(
    domain: &str,
    server: ServerAddr,
    iq: Iq,
    timeout: Duration,
) -> Result<Option<Element>, RegisterError> {
    let jid = Jid::new(domain)
        .map_err(|e| RegisterError::Protocol(format!("bad server name {domain}: {e}")))?;
    let work = async {
        match server {
            ServerAddr::Srv => {
                let connector =
                    StartTlsServerConnector::from(DnsConfig::srv_default_client(domain));
                exchange(connector, &jid, iq).await
            }
            ServerAddr::StartTls { host, port } => {
                let connector = StartTlsServerConnector::from(DnsConfig::no_srv(&host, port));
                exchange(connector, &jid, iq).await
            }
            #[cfg(feature = "dev-insecure")]
            ServerAddr::InsecureTcp { host, port } => {
                let connector =
                    tokio_xmpp::connect::TcpServerConnector::from(DnsConfig::no_srv(&host, port));
                exchange(connector, &jid, iq).await
            }
        }
    };
    tokio::time::timeout(timeout, work)
        .await
        .map_err(|_| RegisterError::Timeout)?
}

fn map_error(error: tokio_xmpp::Error) -> RegisterError {
    match error {
        tokio_xmpp::Error::Io(ref e) if is_certificate_error(e) => {
            RegisterError::TlsInvalid(error.to_string())
        }
        other => RegisterError::Unreachable(other.to_string()),
    }
}

/// Open a stream, send one IQ and read its answer. Returns the payload of the result.
async fn exchange<C: ServerConnector>(
    server: C,
    jid: &Jid,
    iq: Iq,
) -> Result<Option<Element>, RegisterError> {
    let (stream, _binding) = server
        .connect(jid, ns::JABBER_CLIENT, Timeouts::default())
        .await
        .map_err(map_error)?;
    let (features, stream) = stream
        .recv_features::<FallibleStreamElement>()
        .await
        .map_err(|e| match e {
            RecvFeaturesError::Io(e) => map_error(e.into()),
            RecvFeaturesError::StreamError(e) => RegisterError::Protocol(e.to_string()),
        })?;
    let mut stream = stream.box_stream();
    let request = XmppStreamElement::Stanza(Stanza::Iq(iq));
    stream
        .send(&request)
        .await
        .map_err(|e| RegisterError::Unreachable(e.to_string()))?;
    let outcome = loop {
        let item = stream
            .next()
            .await
            .ok_or_else(|| RegisterError::Unreachable("the server closed the stream".into()))?
            .map_err(|e| RegisterError::Protocol(format!("{e:?}")))?;
        match item {
            FallibleStreamElement::Ok(XmppStreamElement::Stanza(Stanza::Iq(answer)))
                if answer.id() == REQUEST_ID =>
            {
                break match answer {
                    Iq::Result { payload, .. } => Ok(payload),
                    Iq::Error { error, .. } => {
                        let mut text = describe(&error);
                        if !features.in_band_registration {
                            text.push_str(" (the server does not offer registration)");
                        }
                        Err(RegisterError::Refused(text))
                    }
                    _ => Err(RegisterError::Protocol("the server sent a request".into())),
                };
            }
            FallibleStreamElement::Ok(XmppStreamElement::StreamError(e)) => {
                break Err(RegisterError::Protocol(e.to_string()));
            }
            // Anything else on a stream that is not logged in does not matter.
            _ => {}
        }
    };
    // The server may close first. The result does not depend on it.
    let _ = stream.shutdown().await;
    outcome
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
    use std::io;
    use std::sync::{Arc, Mutex};

    use sasl::common::ChannelBinding;
    use tokio::io::{BufReader, DuplexStream};
    use tokio_xmpp::xmlstream::{PendingFeaturesRecv, StreamHeader, accept_stream, initiate_stream};
    use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType};
    use xmpp_parsers::stream_features::StreamFeatures;

    use super::*;

    /// What the fake server answers.
    #[derive(Clone, Debug)]
    enum Answer {
        Result(Option<Element>),
        Error(DefinedCondition),
    }

    /// A server in memory. It reads one IQ, keeps it, and answers.
    #[derive(Clone, Debug)]
    struct Fake {
        answer: Answer,
        seen: Arc<Mutex<Vec<Iq>>>,
    }

    impl ServerConnector for Fake {
        type Stream = BufReader<DuplexStream>;

        async fn connect(
            &self,
            _jid: &Jid,
            ns: &'static str,
            timeouts: Timeouts,
        ) -> Result<(PendingFeaturesRecv<Self::Stream>, ChannelBinding), tokio_xmpp::Error> {
            let (client, server) = tokio::io::duplex(8192);
            let (answer, seen) = (self.answer.clone(), Arc::clone(&self.seen));
            tokio::spawn(async move {
                let io = BufReader::new(server);
                let accepted = accept_stream(io, ns::JABBER_CLIENT, Timeouts::default())
                    .await
                    .unwrap();
                let header = StreamHeader {
                    from: Some("example.org".into()),
                    to: None,
                    id: Some("s1".into()),
                };
                let pending = accepted.send_header(header).await.unwrap();
                let features = StreamFeatures {
                    in_band_registration: true,
                    ..Default::default()
                };
                let mut stream = pending
                    .send_features::<FallibleStreamElement>(&features)
                    .await
                    .unwrap();
                let Some(Ok(FallibleStreamElement::Ok(XmppStreamElement::Stanza(Stanza::Iq(
                    request,
                ))))) = stream.next().await
                else {
                    panic!("the client sent no IQ");
                };
                let id = request.id().to_owned();
                seen.lock().unwrap().push(request);
                let reply = match answer {
                    Answer::Result(payload) => Iq::Result {
                        from: None,
                        to: None,
                        id,
                        payload,
                    },
                    Answer::Error(condition) => Iq::Error {
                        from: None,
                        to: None,
                        id,
                        error: StanzaError::new(ErrorType::Cancel, condition, "en", "closed"),
                        payload: None,
                    },
                };
                stream
                    .send(&XmppStreamElement::Stanza(Stanza::Iq(reply)))
                    .await
                    .unwrap();
                // Wait for the client to close, so that it reads the answer first.
                while stream.next().await.is_some() {}
            });
            let header = StreamHeader {
                from: None,
                to: Some("example.org".into()),
                id: None,
            };
            let pending = initiate_stream(BufReader::new(client), ns, header, timeouts)
                .await
                .map_err(|e: io::Error| tokio_xmpp::Error::Io(e))?;
            Ok((pending, ChannelBinding::Unsupported))
        }
    }

    fn fake(answer: Answer) -> (Fake, Arc<Mutex<Vec<Iq>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let server = Fake {
            answer,
            seen: Arc::clone(&seen),
        };
        (server, seen)
    }

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn domain() -> Jid {
        Jid::new("example.org").unwrap()
    }

    fn form_request() -> Iq {
        Iq::Get {
            from: None,
            to: None,
            id: REQUEST_ID.to_owned(),
            payload: Element::builder("query", NS_REGISTER).build(),
        }
    }

    #[tokio::test]
    async fn the_form_request_is_a_get_and_the_answer_parses() {
        let query = el("<query xmlns='jabber:iq:register'><instructions>Pick a name.</instructions>\
            <username/><password/></query>");
        let (server, seen) = fake(Answer::Result(Some(query)));
        let payload = exchange(server, &domain(), form_request())
            .await
            .unwrap()
            .unwrap();
        let form = parse_form(&payload);
        assert_eq!(form.fields, vec!["username", "password"]);
        assert_eq!(form.instructions.as_deref(), Some("Pick a name."));
        let seen = seen.lock().unwrap();
        assert!(
            matches!(&seen[0], Iq::Get { to: None, payload, .. } if payload.is("query", NS_REGISTER))
        );
    }

    #[tokio::test]
    async fn the_submission_is_a_set_with_the_fields() {
        let (server, seen) = fake(Answer::Result(None));
        let submission = RegistrationSubmission::Fields(vec![
            ("username".into(), "carol".into()),
            ("password".into(), "s3cret".into()),
        ]);
        let iq = Iq::Set {
            from: None,
            to: None,
            id: REQUEST_ID.to_owned(),
            payload: submission_query(&submission),
        };
        assert_eq!(exchange(server, &domain(), iq).await.unwrap(), None);
        let seen = seen.lock().unwrap();
        let Iq::Set { payload, .. } = &seen[0] else {
            panic!("not a set");
        };
        let name = payload.get_child("username", NS_REGISTER).unwrap();
        assert_eq!(name.text(), "carol");
    }

    #[tokio::test]
    async fn a_closed_registration_is_a_refusal() {
        let (server, _) = fake(Answer::Error(DefinedCondition::NotAllowed));
        match exchange(server, &domain(), form_request()).await {
            Err(RegisterError::Refused(text)) => assert!(text.contains("NotAllowed"), "{text}"),
            other => panic!("unexpected {other:?}"),
        }
    }
}
