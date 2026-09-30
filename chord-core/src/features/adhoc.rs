//! Ad-hoc commands (XEP-0050).
//!
//! `execute_command` runs a command that has one step. The client sends `execute` with the
//! fields, and the service answers `completed` with a result form. A push app server uses
//! this for registration: the client sends its FCM or UnifiedPush address and gets back the
//! node and the secret for `enable_push` (`docs/android-push.md`).
//!
//! `list_commands` lists the commands of a service (disco#items on the commands node), and
//! `command_step` runs a command with any number of steps. Each step returns a `CommandStep`
//! with the form to show, the allowed actions (next, prev, complete, cancel) and the
//! session id for the next call.

use futures_channel::oneshot;
use jid::Jid;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};
use crate::forms::Form;

const NS_COMMANDS: &str = "http://jabber.org/protocol/commands";
const NS_DATA: &str = "jabber:x:data";
const NS_DISCO_ITEMS: &str = "http://jabber.org/protocol/disco#items";

type Reply = oneshot::Sender<Result<Vec<(String, String)>, ClientError>>;
type ListReply = oneshot::Sender<Result<Vec<CommandItem>, ClientError>>;
type StepReply = oneshot::Sender<Result<CommandStep, ClientError>>;

/// A command that a service offers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct CommandItem {
    /// The JID of the service that runs the command.
    pub jid: String,
    pub node: String,
    pub name: Option<String>,
}

/// What the client does in one step (XEP-0050, 3.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum CommandAction {
    /// Start the command. This is the first step.
    #[default]
    Execute,
    Next,
    Prev,
    Complete,
    Cancel,
}

impl CommandAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Execute => "execute",
            Self::Next => "next",
            Self::Prev => "prev",
            Self::Complete => "complete",
            Self::Cancel => "cancel",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "execute" => Self::Execute,
            "next" => Self::Next,
            "prev" => Self::Prev,
            "complete" => Self::Complete,
            "cancel" => Self::Cancel,
            _ => return None,
        })
    }
}

/// Where a command is (XEP-0050, 3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum CommandStatus {
    /// The command waits for the next step.
    Executing,
    Completed,
    Canceled,
}

/// A note of a command: a message for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct CommandNote {
    /// "info", "warn" or "error".
    pub kind: String,
    pub text: String,
}

/// The answer of a service to one step.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct CommandStep {
    pub node: String,
    /// Give it back with the next step. A command that ended has none.
    pub session_id: Option<String>,
    pub status: CommandStatus,
    /// The actions that the UI may offer, besides `cancel` which is always allowed.
    pub actions: Vec<CommandAction>,
    /// The action of the primary button. `next` if the service names none.
    pub default_action: Option<CommandAction>,
    pub notes: Vec<CommandNote>,
    /// The form to fill in (status `executing`), or the result form (status `completed`).
    pub form: Option<Form>,
}

#[derive(Debug)]
pub(crate) enum Pending {
    Execute { reply: Reply },
    List { reply: ListReply },
    Step { reply: StepReply },
}

pub(crate) enum Command {
    Execute {
        to: Jid,
        node: String,
        fields: Vec<(String, String)>,
        reply: Reply,
    },
    List {
        to: Jid,
        reply: ListReply,
    },
    Step {
        to: Jid,
        node: String,
        session_id: Option<String>,
        action: CommandAction,
        form: Option<Form>,
        reply: StepReply,
    },
}

impl ClientHandle {
    /// Run an ad-hoc command that has one step (XEP-0050). `fields` go in the submitted form.
    /// Returns the fields of the result form as (name, first value) pairs.
    ///
    /// Fails with `Server` when the service answers with an error, and with `Unsupported`
    /// when the command wants more steps or does not finish. `command_step` runs any command.
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

    /// The commands that `to` offers (XEP-0050, section 3.2). `to` is a server, a room or
    /// any other entity. A service with no commands gives an empty list.
    pub async fn list_commands(&self, to: Jid) -> Result<Vec<CommandItem>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Adhoc(Command::List { to, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Run one step of a command. Start with `CommandAction::Execute` and no session id.
    /// For each later step, pass the `session_id` of the last answer, the action that the
    /// user chose, and the filled form. `Prev` and `Cancel` need no form.
    ///
    /// The command ends when the status is `Completed` or `Canceled`.
    pub async fn command_step(
        &self,
        to: Jid,
        node: String,
        session_id: Option<String>,
        action: CommandAction,
        form: Option<Form>,
    ) -> Result<CommandStep, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Adhoc(Command::Step {
            to,
            node,
            session_id,
            action,
            form,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// The IQ that starts the command.
pub(crate) fn execute_iq(to: Jid, node: &str, fields: &[(String, String)]) -> Iq {
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

/// The IQ of one step. The form goes out for `execute`, `next` and `complete`.
pub(crate) fn step_iq(
    to: Jid,
    node: &str,
    session_id: Option<&str>,
    action: CommandAction,
    form: Option<&Form>,
) -> Iq {
    let mut command = Element::builder("command", NS_COMMANDS)
        .attr(nc("node"), node)
        .attr(nc("action"), action.as_str());
    if let Some(sid) = session_id {
        command = command.attr(nc("sessionid"), sid);
    }
    if let Some(form) = form
        && matches!(
            action,
            CommandAction::Execute | CommandAction::Next | CommandAction::Complete
        )
    {
        command = command.append(form.submission());
    }
    Iq::Set {
        from: None,
        to: Some(to),
        id: String::new(),
        payload: command.build(),
    }
}

fn list_iq(to: Jid) -> Iq {
    Iq::Get {
        from: None,
        to: Some(to),
        id: String::new(),
        payload: Element::builder("query", NS_DISCO_ITEMS)
            .attr(nc("node"), NS_COMMANDS)
            .build(),
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Execute {
            to,
            node,
            fields,
            reply,
        } => {
            if node.is_empty() {
                let _ = reply.send(Err(empty_node()));
                return;
            }
            ctx.request(
                execute_iq(to, &node, &fields),
                FeaturePending::Adhoc(Pending::Execute { reply }),
            );
        }
        Command::List { to, reply } => {
            ctx.request(list_iq(to), FeaturePending::Adhoc(Pending::List { reply }));
        }
        Command::Step {
            to,
            node,
            session_id,
            action,
            form,
            reply,
        } => {
            if node.is_empty() {
                let _ = reply.send(Err(empty_node()));
                return;
            }
            if action != CommandAction::Execute && session_id.is_none() {
                let e = format!("the action {} needs the session id", action.as_str());
                let _ = reply.send(Err(ClientError::Invalid(e)));
                return;
            }
            // A form that misses a required value never reaches the service.
            if let Some(form) = &form
                && matches!(action, CommandAction::Next | CommandAction::Complete)
                && let Some(problem) = form.problems().into_iter().next()
            {
                let _ = reply.send(Err(ClientError::Invalid(problem)));
                return;
            }
            ctx.request(
                step_iq(to, &node, session_id.as_deref(), action, form.as_ref()),
                FeaturePending::Adhoc(Pending::Step { reply }),
            );
        }
    }
}

fn empty_node() -> ClientError {
    ClientError::Invalid("the command node is empty".into())
}

/// A command while no session is up.
pub(crate) fn offline(command: Command) {
    let error = ClientError::NotConnected;
    match command {
        Command::Execute { reply, .. } => drop(reply.send(Err(error))),
        Command::List { reply, .. } => drop(reply.send(Err(error))),
        Command::Step { reply, .. } => drop(reply.send(Err(error))),
    }
}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    // The service error and the lost session are the same for each kind of command.
    let payload = match response {
        IqResponse::Result(payload) => Ok(payload),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    match pending {
        Pending::Execute { reply } => {
            let _ = reply.send(payload.and_then(parse_result));
        }
        Pending::List { reply } => {
            let _ = reply.send(payload.map(parse_items));
        }
        Pending::Step { reply } => {
            let _ = reply.send(payload.and_then(parse_step));
        }
    }
}

/// The items of a disco#items answer on the commands node.
fn parse_items(payload: Option<Element>) -> Vec<CommandItem> {
    let Some(query) = payload else {
        return Vec::new();
    };
    query
        .children()
        .filter(|c| c.is("item", NS_DISCO_ITEMS))
        .filter_map(|item| {
            Some(CommandItem {
                jid: item.attr("jid")?.to_owned(),
                node: item.attr("node")?.to_owned(),
                name: item.attr("name").map(str::to_owned),
            })
        })
        .collect()
}

/// The answer to one step.
fn parse_step(payload: Option<Element>) -> Result<CommandStep, ClientError> {
    let command = payload
        .filter(|p| p.is("command", NS_COMMANDS))
        .ok_or_else(|| ClientError::Server("the answer has no command element".into()))?;
    let status = match command.attr("status") {
        Some("executing") => CommandStatus::Executing,
        Some("completed") => CommandStatus::Completed,
        Some("canceled") => CommandStatus::Canceled,
        Some(other) => {
            return Err(ClientError::Server(format!(
                "unknown command status {other}"
            )));
        }
        None => return Err(ClientError::Server("the answer has no status".into())),
    };
    let mut actions = Vec::new();
    let mut default_action = None;
    if let Some(list) = command.get_child("actions", NS_COMMANDS) {
        actions = list
            .children()
            .filter_map(|a| CommandAction::parse(a.name()))
            .collect();
        // The `execute` attribute names the default. With none, it is `next`.
        default_action = Some(
            list.attr("execute")
                .and_then(CommandAction::parse)
                .unwrap_or(CommandAction::Next),
        );
    }
    let notes = command
        .children()
        .filter(|c| c.is("note", NS_COMMANDS))
        .map(|n| CommandNote {
            kind: n.attr("type").unwrap_or("info").to_owned(),
            text: n.text(),
        })
        .collect();
    Ok(CommandStep {
        node: command.attr("node").unwrap_or_default().to_owned(),
        session_id: command.attr("sessionid").map(str::to_owned),
        status,
        actions,
        default_action,
        notes,
        form: Form::in_element(&command),
    })
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

    fn step(
        h: &mut Harness,
        session_id: Option<&str>,
        action: CommandAction,
        form: Option<Form>,
    ) -> oneshot::Receiver<Result<CommandStep, ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::Adhoc(Command::Step {
                    to: Jid::new("chord.example.org").unwrap(),
                    node: "config".into(),
                    session_id: session_id.map(str::to_owned),
                    action,
                    form,
                    reply,
                }),
            )
        });
        answer
    }

    const FIRST_STEP: &str = "<command xmlns='http://jabber.org/protocol/commands' node='config' \
        sessionid='s9' status='executing'>\
        <actions execute='next'><next/></actions>\
        <note type='info'>Step one</note>\
        <x xmlns='jabber:x:data' type='form'><title>Step 1</title>\
        <field var='name' type='text-single' label='Name'><required/></field></x></command>";

    #[test]
    fn list_commands_asks_disco_items_on_the_commands_node() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::Adhoc(Command::List {
                    to: Jid::new("chord.example.org").unwrap(),
                    reply,
                }),
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Get { to, payload, .. } = &iqs[0] else {
            panic!("not a get");
        };
        assert_eq!(to.as_ref().unwrap().to_string(), "chord.example.org");
        assert!(payload.is("query", NS_DISCO_ITEMS));
        assert_eq!(payload.attr("node"), Some(NS_COMMANDS));
        let xml = "<query xmlns='http://jabber.org/protocol/disco#items' \
            node='http://jabber.org/protocol/commands'>\
            <item jid='chord.example.org' node='http://jabber.org/protocol/admin#get-user-roster' name='Get User Roster'/>\
            <item jid='chord.example.org' node='config'/>\
            <item jid='chord.example.org'/></query>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        let items = answer.try_recv().unwrap().unwrap().unwrap();
        // An item with no node is not a command.
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name.as_deref(), Some("Get User Roster"));
        assert_eq!(items[1].node, "config");
        assert_eq!(items[1].name, None);
    }

    #[test]
    fn the_first_step_returns_the_form_the_actions_and_the_session() {
        let mut h = Harness::new();
        let mut answer = step(&mut h, None, CommandAction::Execute, None);
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert_eq!(payload.attr("action"), Some("execute"));
        assert_eq!(payload.attr("sessionid"), None);
        respond(&mut h, IqResponse::Result(Some(el(FIRST_STEP))));
        let step = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(step.status, CommandStatus::Executing);
        assert_eq!(step.session_id.as_deref(), Some("s9"));
        assert_eq!(step.actions, vec![CommandAction::Next]);
        assert_eq!(step.default_action, Some(CommandAction::Next));
        assert_eq!(step.notes[0].text, "Step one");
        let form = step.form.unwrap();
        assert_eq!(form.title.as_deref(), Some("Step 1"));
        assert!(form.get("name").unwrap().required);
    }

    #[test]
    fn a_later_step_sends_the_session_the_action_and_the_form() {
        let mut h = Harness::new();
        let mut form = Form::from_element(&el(
            "<x xmlns='jabber:x:data' type='form'><field var='name' type='text-single'><required/></field></x>",
        ))
        .unwrap();
        form.set("name", vec!["Lobby".into()]);
        let mut answer = step(&mut h, Some("s9"), CommandAction::Complete, Some(form));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert_eq!(payload.attr("node"), Some("config"));
        assert_eq!(payload.attr("action"), Some("complete"));
        assert_eq!(payload.attr("sessionid"), Some("s9"));
        let x = payload.get_child("x", NS_DATA).unwrap();
        assert_eq!(x.attr("type"), Some("submit"));
        assert_eq!(
            x.get_child("field", NS_DATA).unwrap().attr("var"),
            Some("name")
        );
        let xml = "<command xmlns='http://jabber.org/protocol/commands' node='config' \
            sessionid='s9' status='completed'><note type='info'>Done</note></command>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        let done = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(done.status, CommandStatus::Completed);
        assert!(done.form.is_none());
        assert_eq!(done.notes[0].text, "Done");
    }

    #[test]
    fn prev_and_cancel_send_no_form() {
        let mut h = Harness::new();
        let form = Form::from_element(&el("<x xmlns='jabber:x:data' type='form'/>")).unwrap();
        for action in [CommandAction::Prev, CommandAction::Cancel] {
            drop(step(&mut h, Some("s9"), action, Some(form.clone())));
        }
        let iqs = h.sent_iqs();
        assert_eq!(iqs.len(), 2);
        for iq in &iqs {
            let Iq::Set { payload, .. } = iq else {
                panic!("not a set");
            };
            assert!(payload.get_child("x", NS_DATA).is_none());
        }
    }

    #[test]
    fn a_canceled_command_ends() {
        let mut h = Harness::new();
        let mut answer = step(&mut h, Some("s9"), CommandAction::Cancel, None);
        let xml = "<command xmlns='http://jabber.org/protocol/commands' node='config' \
            sessionid='s9' status='canceled'/>";
        respond(&mut h, IqResponse::Result(Some(el(xml))));
        let done = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(done.status, CommandStatus::Canceled);
        assert!(done.actions.is_empty());
    }

    #[test]
    fn a_step_with_a_missing_required_value_or_session_is_refused() {
        let mut h = Harness::new();
        let form = Form::from_element(&el(
            "<x xmlns='jabber:x:data' type='form'><field var='name' type='text-single' label='Name'><required/></field></x>",
        ))
        .unwrap();
        let mut answer = step(&mut h, Some("s9"), CommandAction::Next, Some(form.clone()));
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Invalid(text)) => assert!(text.contains("Name is required")),
            other => panic!("unexpected {other:?}"),
        }
        let mut answer = step(&mut h, None, CommandAction::Next, Some(form));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Invalid(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_step_error_reaches_the_caller() {
        let mut h = Harness::new();
        let mut answer = step(&mut h, Some("gone"), CommandAction::Next, None);
        let error = StanzaError::new(
            ErrorType::Modify,
            DefinedCondition::BadRequest,
            "en",
            "session expired",
        );
        respond(&mut h, IqResponse::Error(error));
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Server(text)) => assert!(text.contains("session expired")),
            other => panic!("unexpected {other:?}"),
        }
    }
}
