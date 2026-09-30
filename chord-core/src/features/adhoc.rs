//! Ad-hoc commands (XEP-0050), the one-step form only.
//!
//! The client sends `execute` with the fields, and the service answers `completed` with a
//! result form. A push app server uses this for registration: the client sends its FCM or
//! UnifiedPush address and gets back the node and the secret for `enable_push`
//! (`docs/android-push.md`). A command with more steps fails with `Unsupported`.

use futures_channel::oneshot;
use jid::Jid;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};

const NS_COMMANDS: &str = "http://jabber.org/protocol/commands";
const NS_DATA: &str = "jabber:x:data";

type Reply = oneshot::Sender<Result<Vec<(String, String)>, ClientError>>;

#[derive(Debug)]
pub(crate) enum Pending {
    Execute { reply: Reply },
}

pub(crate) enum Command {
    Execute {
        to: Jid,
        node: String,
        fields: Vec<(String, String)>,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Run an ad-hoc command that has one step (XEP-0050). `fields` go in the submitted form.
    /// Returns the fields of the result form as (name, first value) pairs.
    ///
    /// Fails with `Server` when the service answers with an error, and with `Unsupported`
    /// when the command wants more steps or does not finish.
    pub async fn execute_command(
        &self,
        to: Jid,
        node: String,
        fields: Vec<(String, String)>,
    ) -> Result<Vec<(String, String)>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Adhoc(Command::Execute {
            to,
            node,
            fields,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// The IQ that starts the command.
fn execute_iq(to: Jid, node: &str, fields: &[(String, String)]) -> Iq {
    let mut form = Element::builder("x", NS_DATA).attr(nc("type"), "submit");
    for (var, value) in fields {
        form = form.append(
            Element::builder("field", NS_DATA)
                .attr(nc("var"), var.as_str())
                .append(
                    Element::builder("value", NS_DATA)
                        .append(value.as_str())
                        .build(),
                )
                .build(),
        );
    }
    let command = Element::builder("command", NS_COMMANDS)
        .attr(nc("node"), node)
        .attr(nc("action"), "execute")
        .append(form.build())
        .build();
    Iq::Set {
        from: None,
        to: Some(to),
        id: String::new(),
        payload: command,
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    let Command::Execute {
        to,
        node,
        fields,
        reply,
    } = command;
    if node.is_empty() {
        let _ = reply.send(Err(ClientError::Invalid(
            "the command node is empty".into(),
        )));
        return;
    }
    ctx.request(
        execute_iq(to, &node, &fields),
        FeaturePending::Adhoc(Pending::Execute { reply }),
    );
}

/// A command while no session is up.
pub(crate) fn offline(command: Command) {
    let Command::Execute { reply, .. } = command;
    let _ = reply.send(Err(ClientError::NotConnected));
}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let Pending::Execute { reply } = pending;
    let outcome = match response {
        IqResponse::Result(payload) => parse_result(payload),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    let _ = reply.send(outcome);
}

/// The fields of the result form of a completed command.
fn parse_result(payload: Option<Element>) -> Result<Vec<(String, String)>, ClientError> {
    let command = payload
        .filter(|p| p.is("command", NS_COMMANDS))
        .ok_or_else(|| ClientError::Server("the answer has no command element".into()))?;
    match command.attr("status") {
        Some("completed") => {}
        Some(other) => {
            let e = format!("the command is {other}, and Chord runs one-step commands only");
            return Err(ClientError::Unsupported(e));
        }
        None => return Err(ClientError::Server("the answer has no status".into())),
    }
    let mut out = Vec::new();
    if let Some(form) = command.get_child("x", NS_DATA) {
        for field in form.children().filter(|c| c.is("field", NS_DATA)) {
            let Some(var) = field.attr("var") else {
                continue;
            };
            let value = field
                .get_child("value", NS_DATA)
                .map(Element::text)
                .unwrap_or_default();
            out.push((var.to_owned(), value));
        }
    }
    Ok(out)
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
    use crate::features::on_command as features_command;
    use crate::features::testing::Harness;

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn run(
        h: &mut Harness,
        fields: Vec<(String, String)>,
    ) -> oneshot::Receiver<Result<Vec<(String, String)>, ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::Adhoc(Command::Execute {
                    to: Jid::new("push.example.org").unwrap(),
                    node: "register-push-fcm".into(),
                    fields,
                    reply,
                }),
            )
        });
        answer
    }

    fn respond(h: &mut Harness, response: IqResponse) {
        h.respond(|p| matches!(p, FeaturePending::Adhoc(_)), response);
    }

    #[test]
    fn execute_sends_the_form() {
        let mut h = Harness::new();
        let _answer = run(
            &mut h,
            vec![
                ("token".into(), "T".into()),
                ("android-id".into(), "A".into()),
            ],
        );
        let iqs = h.sent_iqs();
        let Iq::Set { to, payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert_eq!(to.as_ref().unwrap().to_string(), "push.example.org");
        assert_eq!(payload.attr("node"), Some("register-push-fcm"));
        assert_eq!(payload.attr("action"), Some("execute"));
        let form = payload.get_child("x", NS_DATA).unwrap();
        assert_eq!(form.attr("type"), Some("submit"));
        let fields: Vec<_> = form.children().collect();
        assert_eq!(fields[0].attr("var"), Some("token"));
        assert_eq!(fields[0].children().next().unwrap().text(), "T");
        assert_eq!(fields[1].attr("var"), Some("android-id"));
    }

    #[test]
    fn a_completed_command_returns_the_result_fields() {
        let mut h = Harness::new();
        let mut answer = run(&mut h, vec![]);
        let xml = "<command xmlns='http://jabber.org/protocol/commands' node='register-push-fcm' \
            sessionid='s1' status='completed'><x xmlns='jabber:x:data' type='result'>\
            <field var='jid'><value>p2.example.org</value></field>\
            <field var='node'><value>n-123</value></field>\
            <field var='secret'><value>s-456</value></field></x></command>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        let fields = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(
            fields,
            vec![
                ("jid".to_owned(), "p2.example.org".to_owned()),
                ("node".to_owned(), "n-123".to_owned()),
                ("secret".to_owned(), "s-456".to_owned()),
            ]
        );
    }

    #[test]
    fn a_command_with_more_steps_is_unsupported() {
        let mut h = Harness::new();
        let mut answer = run(&mut h, vec![]);
        let xml = "<command xmlns='http://jabber.org/protocol/commands' node='x' \
            sessionid='s1' status='executing'/>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Unsupported(_))
        ));
    }

    #[test]
    fn a_service_error_reaches_the_caller() {
        let mut h = Harness::new();
        let mut answer = run(&mut h, vec![]);
        let error = StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::ItemNotFound,
            "en",
            "no such command",
        );
        respond(&mut h, IqResponse::Error(error));
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Server(text)) => assert!(text.contains("ItemNotFound")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn an_empty_node_is_refused_before_sending() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::Adhoc(Command::Execute {
                    to: Jid::new("push.example.org").unwrap(),
                    node: String::new(),
                    fields: vec![],
                    reply,
                }),
            )
        });
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Invalid(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }
}
