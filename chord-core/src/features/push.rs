//! Push notifications (XEP-0357): enable and disable an app server.
//!
//! The server keeps the registration for the account. So `on_connected` does nothing and
//! Chord never enables a service again on its own. The store keeps the service and the
//! node only. The publish-options form (with the app server secret) goes to the server
//! and is never stored.

use futures_channel::oneshot;
use jid::Jid;
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::push::{Disable, Enable};
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};

const NS_PUSH: &str = "urn:xmpp:push:0";
const NS_PUBLISH_OPTIONS: &str = "http://jabber.org/protocol/pubsub#publish-options";

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// A push service that this account enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRegistration {
    /// The bare JID of the push service.
    pub service: String,
    /// The node that the app server gave for this account.
    pub node: String,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    Enable {
        service: String,
        node: String,
        reply: Reply<()>,
    },
    Disable {
        service: String,
        node: Option<String>,
        reply: Reply<()>,
    },
}

/// A command from the public API.
pub(crate) enum Command {
    Enable {
        service: Jid,
        node: String,
        form: Option<Vec<(String, String)>>,
        reply: Reply<()>,
    },
    Disable {
        service: Jid,
        node: Option<String>,
        reply: Reply<()>,
    },
    List {
        reply: Reply<Vec<PushRegistration>>,
    },
}

impl ClientHandle {
    /// Enable push notifications through `service` and `node` (XEP-0357).
    ///
    /// `form` holds the publish options for the app server, for example its secret. The
    /// core sends them to the server and does not store them. The command waits for
    /// service discovery. It fails with `Unsupported` when neither the server nor the
    /// account advertises `urn:xmpp:push:0`.
    pub async fn enable_push(
        &self,
        service: Jid,
        node: String,
        form: Option<Vec<(String, String)>>,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Push(Command::Enable {
            service,
            node,
            form,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Disable push notifications for `service`. With `node` set, only that node stops.
    /// With `None`, every node of the service stops.
    pub async fn disable_push(
        &self,
        service: Jid,
        node: Option<String>,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Push(Command::Disable {
            service,
            node,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The push services that this account enabled, from the store.
    pub async fn push_registrations(&self) -> Result<Vec<PushRegistration>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Push(Command::List { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let outcome = match response {
        IqResponse::Result(_) => Ok(()),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    match pending {
        Pending::Enable {
            service,
            node,
            reply,
        } => {
            let outcome = outcome.and_then(|()| store_registration(ctx, &service, &node));
            let _ = reply.send(outcome);
        }
        Pending::Disable {
            service,
            node,
            reply,
        } => {
            let outcome =
                outcome.and_then(|()| delete_registrations(ctx, &service, node.as_deref()));
            let _ = reply.send(outcome);
        }
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Enable {
            service,
            node,
            form,
            reply,
        } => {
            if let Err((e, reply)) = start_enable(ctx, service, node, form, reply) {
                let _ = reply.send(Err(e));
            }
        }
        Command::Disable {
            service,
            node,
            reply,
        } => {
            let service = service.to_bare();
            let payload = Disable {
                jid: service.clone(),
                node: node.clone(),
            };
            ctx.request(
                Iq::from_set("", payload),
                FeaturePending::Push(Pending::Disable {
                    service: service.to_string(),
                    node,
                    reply,
                }),
            );
        }
        Command::List { reply } => {
            let _ = reply.send(list(ctx.store, ctx.account_id));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
///
/// `List` needs the store, so `features::on_command_offline` must answer it with
/// `push::list` before it calls this function.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Enable { reply, .. } | Command::Disable { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::List { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// The stored registrations of an account. For `features::on_command_offline` too.
pub(crate) fn list(
    store: &crate::store::Store,
    account_id: i64,
) -> Result<Vec<PushRegistration>, ClientError> {
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    let mut stmt = store
        .conn()
        .prepare(
            "SELECT service, node FROM push_registrations WHERE account_id = ?1
             ORDER BY service, node",
        )
        .map_err(store_error)?;
    let rows = stmt
        .query_map([account_id], |row| {
            Ok(PushRegistration {
                service: row.get(0)?,
                node: row.get(1)?,
            })
        })
        .map_err(store_error)?;
    rows.collect::<Result<_, _>>().map_err(store_error)
}

type StartError = (ClientError, Reply<()>);

fn start_enable(
    ctx: &mut Ctx<'_>,
    service: Jid,
    node: String,
    form: Option<Vec<(String, String)>>,
    reply: Reply<()>,
) -> Result<(), StartError> {
    if node.is_empty() {
        return Err((ClientError::Invalid("the push node is empty".into()), reply));
    }
    // Prosody advertises push on the account JID (mod_cloud_notify, account-disco-info).
    if !ctx.state.disco.server_has(NS_PUSH) {
        let e = ClientError::Unsupported("the server has no push support".into());
        return Err((e, reply));
    }
    let service = service.to_bare();
    let form = form.map(|fields| {
        let fields = fields
            .into_iter()
            .map(|(var, value)| Field::new(&var, FieldType::TextSingle).with_value(&value))
            .collect();
        DataForm::new(DataFormType::Submit, NS_PUBLISH_OPTIONS, fields)
    });
    // xmpp-parsers 0.23, push.rs lines 15-27: `jid` is a bare JID, `node` and `form` are optional.
    let payload = Enable {
        jid: service.clone(),
        node: Some(node.clone()),
        form,
    };
    ctx.request(
        Iq::from_set("", payload),
        FeaturePending::Push(Pending::Enable {
            service: service.to_string(),
            node,
            reply,
        }),
    );
    Ok(())
}

fn store_registration(ctx: &mut Ctx<'_>, service: &str, node: &str) -> Result<(), ClientError> {
    ctx.store
        .conn()
        .execute(
            "INSERT OR REPLACE INTO push_registrations (account_id, service, node)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![ctx.account_id, service, node],
        )
        .map(|_| ())
        .map_err(|e| ClientError::Invalid(format!("store: {e}")))
}

fn delete_registrations(
    ctx: &mut Ctx<'_>,
    service: &str,
    node: Option<&str>,
) -> Result<(), ClientError> {
    // A NULL node matches every node of the service.
    ctx.store
        .conn()
        .execute(
            "DELETE FROM push_registrations
             WHERE account_id = ?1 AND service = ?2 AND (?3 IS NULL OR node = ?3)",
            rusqlite::params![ctx.account_id, service, node],
        )
        .map(|_| ())
        .map_err(|e| ClientError::Invalid(format!("store: {e}")))
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
    use crate::features::{FeatureCommand, on_command};

    type Answer = oneshot::Receiver<Result<(), ClientError>>;

    fn jid(s: &str) -> Jid {
        Jid::new(s).unwrap()
    }

    fn harness(supported: bool) -> Harness {
        let mut h = Harness::new();
        let mut info = crate::features::disco::info(None);
        if supported {
            info.features.insert(NS_PUSH.into());
        }
        h.state.disco.server = Some(info);
        h.state.disco.complete = true;
        h
    }

    fn enable(h: &mut Harness, form: Option<Vec<(String, String)>>) -> Answer {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Push(Command::Enable {
                    service: jid("push.example"),
                    node: "n1".into(),
                    form,
                    reply,
                }),
            )
        });
        answer
    }

    fn disable(h: &mut Harness, node: Option<&str>) -> Answer {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Push(Command::Disable {
                    service: jid("push.example"),
                    node: node.map(str::to_owned),
                    reply,
                }),
            )
        });
        answer
    }

    fn respond(h: &mut Harness, response: IqResponse) {
        h.respond(|p| matches!(p, FeaturePending::Push(_)), response);
    }

    fn stored(h: &Harness) -> Vec<(String, String)> {
        list(&h.store, h.account_id)
            .unwrap()
            .into_iter()
            .map(|r| (r.service, r.node))
            .collect()
    }

    fn payload_of(h: &mut Harness) -> xmpp_parsers::minidom::Element {
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        payload.clone()
    }

    fn store_direct(h: &mut Harness, service: &str, node: &str) {
        h.with_ctx(|ctx| store_registration(ctx, service, node).unwrap());
    }

    #[test]
    fn enable_without_form() {
        let mut h = harness(true);
        let _answer = enable(&mut h, None);
        let payload = payload_of(&mut h);
        assert!(payload.is("enable", NS_PUSH));
        assert_eq!(payload.attr("jid"), Some("push.example"));
        assert_eq!(payload.attr("node"), Some("n1"));
        assert_eq!(payload.children().count(), 0);
    }

    #[test]
    fn enable_with_form() {
        let mut h = harness(true);
        let form = vec![("secret".to_owned(), "s3".to_owned())];
        let _answer = enable(&mut h, Some(form));
        let payload = payload_of(&mut h);
        let x = payload.get_child("x", "jabber:x:data").expect("a form");
        assert_eq!(x.attr("type"), Some("submit"));
        let fields: Vec<_> = x.children().collect();
        assert_eq!(fields[0].attr("var"), Some("FORM_TYPE"));
        assert_eq!(
            fields[0].children().next().unwrap().text(),
            NS_PUBLISH_OPTIONS
        );
        assert_eq!(fields[1].attr("var"), Some("secret"));
        assert_eq!(fields[1].children().next().unwrap().text(), "s3");
    }

    #[test]
    fn result_stores_the_row_once() {
        let mut h = harness(true);
        let form = vec![("secret".to_owned(), "s3".to_owned())];
        let mut answer = enable(&mut h, Some(form));
        assert!(stored(&h).is_empty());
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        let _answer = enable(&mut h, None);
        respond(&mut h, IqResponse::Result(None));
        assert_eq!(stored(&h), vec![("push.example".into(), "n1".into())]);
    }

    #[test]
    fn error_maps_to_server_and_stores_nothing() {
        let mut h = harness(true);
        let mut answer = enable(&mut h, None);
        let error = StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::ServiceUnavailable,
            "en",
            "no push",
        );
        respond(&mut h, IqResponse::Error(error));
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Server(text)) => assert!(text.contains("ServiceUnavailable")),
            other => panic!("unexpected {other:?}"),
        }
        assert!(stored(&h).is_empty());
    }

    #[test]
    fn unsupported_server_sends_nothing() {
        let mut h = harness(false);
        let mut answer = enable(&mut h, None);
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Unsupported(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn disable_deletes_one_node_or_all() {
        let mut h = harness(true);
        for node in ["n1", "n2"] {
            store_direct(&mut h, "push.example", node);
        }
        store_direct(&mut h, "other.example", "n1");

        let mut answer = disable(&mut h, Some("n1"));
        let payload = payload_of(&mut h);
        assert!(payload.is("disable", NS_PUSH));
        assert_eq!(payload.attr("node"), Some("n1"));
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        assert_eq!(stored(&h).len(), 2);

        let _answer = disable(&mut h, None);
        let payload = payload_of(&mut h);
        assert_eq!(payload.attr("node"), None);
        respond(&mut h, IqResponse::Result(None));
        assert_eq!(stored(&h), vec![("other.example".into(), "n1".into())]);
    }

    #[test]
    fn list_command_reads_the_store() {
        let mut h = harness(true);
        store_direct(&mut h, "push.example", "n1");
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, FeatureCommand::Push(Command::List { reply })));
        let rows = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(rows.len(), 1);
    }
}
