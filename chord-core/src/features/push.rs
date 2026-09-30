//! Push notifications (XEP-0357): enable and disable an app server.
//!
//! The server keeps one registration for each account and resource, and XEP-0357 has no
//! query to list them. So Chord does not trust its own table: after each new session and
//! service discovery, `on_services_ready` sends `enable` again for every stored row.
//! Enable is idempotent: the same service and node replace the old registration. The
//! store keeps the service, the node, and the publish options (the app server secret, for
//! example), because a new `enable` without the options would replace the registration
//! and drop the secret.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
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
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
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
        /// The publish options, to store with the registration.
        options: Vec<(String, String)>,
        reply: Reply<()>,
    },
    /// The enable that Chord sends again for a stored registration. Only an error needs
    /// an action.
    Reenable { service: String, node: String },
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
            options,
            reply,
        } => {
            let outcome = outcome.and_then(|()| store_registration(ctx, &service, &node, &options));
            let _ = reply.send(outcome);
        }
        Pending::Reenable { service, node } => {
            // Keep the row: a failure may be short, and the user did not ask to stop.
            if let Err(e) = outcome {
                log::warn!("push: enable again for {service} node {node} failed: {e}");
            } else {
                log::info!("push: enabled {service} node {node} again");
            }
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
    let options = form.unwrap_or_default();
    ctx.request(
        enable_iq(&service, &node, &options),
        FeaturePending::Push(Pending::Enable {
            service: service.to_string(),
            node,
            options,
            reply,
        }),
    );
    Ok(())
}

/// The enable IQ. Every publish option is a `text-single` field, which is what the
/// standard `secret` field needs (XEP-0357, 5).
fn enable_iq(service: &BareJid, node: &str, options: &[(String, String)]) -> Iq {
    let form = (!options.is_empty()).then(|| {
        let fields = options
            .iter()
            .map(|(var, value)| Field::new(var, FieldType::TextSingle).with_value(value))
            .collect();
        DataForm::new(DataFormType::Submit, NS_PUBLISH_OPTIONS, fields)
    });
    // xmpp-parsers 0.23, push.rs lines 15-27: `jid` is a bare JID, `node` and `form` are optional.
    Iq::from_set(
        "",
        Enable {
            jid: service.clone(),
            node: Some(node.to_owned()),
            form,
        },
    )
}

/// Service discovery finished: enable every stored registration again. The server may
/// have dropped one, for example after an account reset or a server restore.
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    let stored = match list(ctx.store, ctx.account_id) {
        Ok(stored) => stored,
        Err(e) => {
            log::error!("push: read the registrations: {e}");
            return;
        }
    };
    if stored.is_empty() {
        return;
    }
    if !ctx.state.disco.server_has(NS_PUSH) {
        log::info!("push: the server has no push support, so no enable again");
        return;
    }
    for registration in stored {
        let Ok(service) = BareJid::new(&registration.service) else {
            continue;
        };
        let options = stored_options(ctx, &registration.service, &registration.node);
        ctx.request(
            enable_iq(&service, &registration.node, &options),
            FeaturePending::Push(Pending::Reenable {
                service: registration.service,
                node: registration.node,
            }),
        );
    }
}

fn stored_options(ctx: &mut Ctx<'_>, service: &str, node: &str) -> Vec<(String, String)> {
    let read = || -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = ctx.store.conn().prepare(
            "SELECT var, value FROM push_options
             WHERE account_id = ?1 AND service = ?2 AND node = ?3 ORDER BY var",
        )?;
        let rows = stmt.query_map(rusqlite::params![ctx.account_id, service, node], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        rows.collect()
    };
    read().unwrap_or_else(|e| {
        log::error!("push: read the options: {e}");
        Vec::new()
    })
}

fn store_registration(
    ctx: &mut Ctx<'_>,
    service: &str,
    node: &str,
    options: &[(String, String)],
) -> Result<(), ClientError> {
    let conn = ctx.store.conn();
    let write = || -> rusqlite::Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO push_registrations (account_id, service, node)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![ctx.account_id, service, node],
        )?;
        // The new enable replaces the old registration, options included.
        conn.execute(
            "DELETE FROM push_options WHERE account_id = ?1 AND service = ?2 AND node = ?3",
            rusqlite::params![ctx.account_id, service, node],
        )?;
        for (var, value) in options {
            conn.execute(
                "INSERT INTO push_options (account_id, service, node, var, value)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (account_id, service, node, var) DO UPDATE SET value = excluded.value",
                rusqlite::params![ctx.account_id, service, node, var, value],
            )?;
        }
        Ok(())
    };
    write().map_err(|e| ClientError::Invalid(format!("store: {e}")))
}

fn delete_registrations(
    ctx: &mut Ctx<'_>,
    service: &str,
    node: Option<&str>,
) -> Result<(), ClientError> {
    // A NULL node matches every node of the service.
    let conn = ctx.store.conn();
    let delete = |table: &str| {
        conn.execute(
            &format!(
                "DELETE FROM {table}
                 WHERE account_id = ?1 AND service = ?2 AND (?3 IS NULL OR node = ?3)"
            ),
            rusqlite::params![ctx.account_id, service, node],
        )
    };
    delete("push_options")
        .and_then(|_| delete("push_registrations"))
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
        h.with_ctx(|ctx| store_registration(ctx, service, node, &[]).unwrap());
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
    fn a_new_session_enables_the_stored_registrations_again_with_their_secret() {
        let mut h = harness(true);
        // A first enable with a secret stores the options with the row.
        let form = vec![("secret".to_owned(), "s3".to_owned())];
        let _answer = enable(&mut h, Some(form));
        respond(&mut h, IqResponse::Result(None));
        h.sent_iqs();

        // A new session: discovery finishes, and Chord enables the row again.
        h.with_ctx(on_services_ready);
        let payload = payload_of(&mut h);
        assert!(payload.is("enable", NS_PUSH));
        assert_eq!(payload.attr("jid"), Some("push.example"));
        assert_eq!(payload.attr("node"), Some("n1"));
        let x = payload
            .get_child("x", "jabber:x:data")
            .expect("the options");
        let secret = x.children().nth(1).unwrap();
        assert_eq!(secret.attr("var"), Some("secret"));
        assert_eq!(secret.children().next().unwrap().text(), "s3");

        // An error keeps the row: the next login tries again.
        let error = StanzaError::new(ErrorType::Cancel, DefinedCondition::ItemNotFound, "en", "");
        respond(&mut h, IqResponse::Error(error));
        assert_eq!(stored(&h), vec![("push.example".into(), "n1".into())]);
    }

    #[test]
    fn no_enable_again_without_rows_or_without_server_support() {
        let mut h = harness(true);
        h.with_ctx(on_services_ready);
        assert!(h.sent_iqs().is_empty(), "no rows, no IQ");

        store_direct(&mut h, "push.example", "n1");
        let mut h2 = harness(false);
        store_direct(&mut h2, "push.example", "n1");
        h2.with_ctx(on_services_ready);
        assert!(h2.sent_iqs().is_empty(), "the server has no push");
    }

    #[test]
    fn disable_removes_the_options_too() {
        let mut h = harness(true);
        let form = vec![("secret".to_owned(), "s3".to_owned())];
        let _answer = enable(&mut h, Some(form));
        respond(&mut h, IqResponse::Result(None));
        let _answer = disable(&mut h, None);
        respond(&mut h, IqResponse::Result(None));
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT count(*) FROM push_options", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
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
