//! The whole room configuration form of a room owner (XEP-0045, section 10.2).
//!
//! `room_config_form` fetches the `muc#owner` form and `submit_room_config_form` sends it
//! back. A UI shows the form with a generic form renderer, so every option of the room
//! (password, description, moderated, who may invite, maximum users, who sees the JIDs)
//! is there. `configure_room` in `muc.rs` stays as the simple path that sets three
//! options.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};
use crate::forms::Form;

const NS_MUC_OWNER: &str = "http://jabber.org/protocol/muc#owner";

type FormReply = oneshot::Sender<Result<Form, ClientError>>;
type DoneReply = oneshot::Sender<Result<(), ClientError>>;

#[derive(Debug)]
pub(crate) enum Pending {
    Fetch { reply: FormReply },
    Submit { reply: DoneReply },
}

pub(crate) enum Command {
    Fetch {
        room: BareJid,
        reply: FormReply,
    },
    Submit {
        room: BareJid,
        form: Form,
        reply: DoneReply,
    },
}

impl ClientHandle {
    /// The configuration form of a room that we own. Fails with `Server` when the room
    /// refuses (`forbidden` for a person who is not an owner) and when the answer has no
    /// form.
    pub async fn room_config_form(&self, room: BareJid) -> Result<Form, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::OwnerForm(Command::Fetch { room, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Send the filled configuration form of a room. The form is the one from
    /// `room_config_form` with the values changed. A missing required value fails with
    /// `Invalid` before anything goes out.
    pub async fn submit_room_config_form(
        &self,
        room: BareJid,
        form: Form,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::OwnerForm(Command::Submit {
            room,
            form,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Fetch { room, reply } => {
            let iq = Iq::Get {
                from: None,
                to: Some(Jid::from(room)),
                id: String::new(),
                payload: Element::builder("query", NS_MUC_OWNER).build(),
            };
            ctx.request(iq, FeaturePending::OwnerForm(Pending::Fetch { reply }));
        }
        Command::Submit { room, form, reply } => {
            if let Some(problem) = form.problems().into_iter().next() {
                let _ = reply.send(Err(ClientError::Invalid(problem)));
                return;
            }
            let iq = Iq::Set {
                from: None,
                to: Some(Jid::from(room)),
                id: String::new(),
                payload: Element::builder("query", NS_MUC_OWNER)
                    .append(form.submission())
                    .build(),
            };
            ctx.request(iq, FeaturePending::OwnerForm(Pending::Submit { reply }));
        }
    }
}

/// A command while no session is up.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Fetch { reply, .. } => drop(reply.send(Err(ClientError::NotConnected))),
        Command::Submit { reply, .. } => drop(reply.send(Err(ClientError::NotConnected))),
    }
}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let payload = match response {
        IqResponse::Result(payload) => Ok(payload),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    match pending {
        Pending::Fetch { reply } => {
            let form = payload.and_then(|query| {
                query
                    .as_ref()
                    .and_then(Form::in_element)
                    .ok_or_else(|| ClientError::Server("the room has no configuration form".into()))
            });
            let _ = reply.send(form);
        }
        Pending::Submit { reply } => {
            let _ = reply.send(payload.map(drop));
        }
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
    use crate::features::on_command as features_command;
    use crate::features::testing::Harness;

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn room() -> BareJid {
        BareJid::new("lobby@conference.example.org").unwrap()
    }

    const FORM: &str = "<query xmlns='http://jabber.org/protocol/muc#owner'>\
        <x xmlns='jabber:x:data' type='form'><title>Configuration of lobby</title>\
        <field var='FORM_TYPE' type='hidden'><value>http://jabber.org/protocol/muc#roomconfig</value></field>\
        <field var='muc#roomconfig_roomname' type='text-single' label='Room title'><value>Lobby</value></field>\
        <field var='muc#roomconfig_roomdesc' type='text-single' label='Description'/>\
        <field var='muc#roomconfig_roomsecret' type='text-private' label='Password'/>\
        <field var='muc#roomconfig_moderatedroom' type='boolean' label='Moderated'><value>0</value></field>\
        <field var='muc#roomconfig_maxusers' type='list-single' label='Maximum users'><value>30</value>\
        <option label='10'><value>10</value></option><option label='30'><value>30</value></option></field>\
        <field var='muc#roomconfig_whois' type='list-single' label='Who sees the JIDs'><value>moderators</value>\
        <option label='Moderators only'><value>moderators</value></option><option label='Anyone'><value>anyone</value></option></field>\
        <field var='muc#roomconfig_allowinvites' type='boolean' label='Allow invites'><value>1</value></field>\
        </x></query>";

    #[test]
    fn the_whole_form_comes_back_and_the_changes_go_out() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::OwnerForm(Command::Fetch {
                    room: room(),
                    reply,
                }),
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Get { to, payload, .. } = &iqs[0] else {
            panic!("not a get");
        };
        assert_eq!(to.as_ref().unwrap().to_string(), room().to_string());
        assert!(payload.is("query", NS_MUC_OWNER));
        h.respond(
            |p| matches!(p, FeaturePending::OwnerForm(_)),
            IqResponse::Result(Some(el(FORM))),
        );
        let mut form = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(form.fields.len(), 8);
        assert_eq!(form.title.as_deref(), Some("Configuration of lobby"));

        form.set("muc#roomconfig_roomsecret", vec!["hunter2".into()]);
        form.set("muc#roomconfig_moderatedroom", vec!["1".into()]);
        form.set("muc#roomconfig_maxusers", vec!["10".into()]);
        let (reply, mut done) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::OwnerForm(Command::Submit {
                    room: room(),
                    form,
                    reply,
                }),
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        let x = payload.get_child("x", "jabber:x:data").unwrap();
        assert_eq!(x.attr("type"), Some("submit"));
        let value = |var: &str| {
            x.children()
                .find(|f| f.attr("var") == Some(var))
                .unwrap_or_else(|| panic!("no field {var}"))
                .children()
                .next()
                .unwrap()
                .text()
        };
        assert_eq!(
            value("FORM_TYPE"),
            "http://jabber.org/protocol/muc#roomconfig"
        );
        assert_eq!(value("muc#roomconfig_roomsecret"), "hunter2");
        assert_eq!(value("muc#roomconfig_moderatedroom"), "1");
        assert_eq!(value("muc#roomconfig_maxusers"), "10");
        assert_eq!(value("muc#roomconfig_roomname"), "Lobby");
        assert_eq!(value("muc#roomconfig_whois"), "moderators");
        h.respond(
            |p| matches!(p, FeaturePending::OwnerForm(_)),
            IqResponse::Result(None),
        );
        assert!(done.try_recv().unwrap().unwrap().is_ok());
    }

    #[test]
    fn an_answer_with_no_form_is_an_error() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::OwnerForm(Command::Fetch {
                    room: room(),
                    reply,
                }),
            )
        });
        h.respond(
            |p| matches!(p, FeaturePending::OwnerForm(_)),
            IqResponse::Result(Some(el(
                "<query xmlns='http://jabber.org/protocol/muc#owner'/>",
            ))),
        );
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Server(_))
        ));
    }

    #[test]
    fn a_refusal_reaches_the_caller() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::OwnerForm(Command::Fetch {
                    room: room(),
                    reply,
                }),
            )
        });
        let error = StanzaError::new(
            ErrorType::Auth,
            DefinedCondition::Forbidden,
            "en",
            "owners only",
        );
        h.respond(
            |p| matches!(p, FeaturePending::OwnerForm(_)),
            IqResponse::Error(error),
        );
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Server(text)) => assert!(text.contains("owners only")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_form_with_a_missing_required_value_is_not_sent() {
        let mut h = Harness::new();
        let form = Form::from_element(&el("<x xmlns='jabber:x:data' type='form'>\
            <field var='muc#roomconfig_roomname' type='text-single' label='Room title'><required/></field></x>"))
        .unwrap();
        let (reply, mut done) = oneshot::channel();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::OwnerForm(Command::Submit {
                    room: room(),
                    form,
                    reply,
                }),
            )
        });
        assert!(matches!(
            done.try_recv().unwrap().unwrap(),
            Err(ClientError::Invalid(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }
}
