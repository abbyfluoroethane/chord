//! In-band registration (XEP-0077).
//!
//! On a bound stream this feature changes the password of the account (`change_password`,
//! XEP-0077 section 3.3) and deletes it (`delete_account`, section 3.2). Before login, `session::register` uses the parse and build
//! functions of this file to read the registration form of a server and to submit it.
//!
//! A server asks for the fields in one of two ways: the legacy way (a child element for
//! each field, like `<username/>`) or a data form (XEP-0004, with CAPTCHA images in
//! XEP-0231 media). A server that does not allow registration can give a link instead
//! (XEP-0066 `jabber:x:oob`).

use futures_channel::oneshot;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, Effect, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};
use crate::forms::{FieldKind, Form};

pub(crate) const NS_REGISTER: &str = "jabber:iq:register";
const NS_OOB: &str = "jabber:x:oob";

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// A link that the server gives instead of a form (XEP-0066).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct OobLink {
    pub url: String,
    pub desc: Option<String>,
}

/// What a server wants for a registration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct RegistrationForm {
    pub instructions: Option<String>,
    /// The data form. When it is set, answer with it.
    pub form: Option<Form>,
    /// The names of the legacy fields (`username`, `password`, `email` ...). Used when
    /// there is no data form.
    pub fields: Vec<String>,
    /// A web page for the registration. A server gives it when it does not allow
    /// registration in the client, or next to a form.
    pub oob: Option<OobLink>,
    /// True when the account exists already (the query has `<registered/>`).
    pub registered: bool,
}

/// What the user answers: the legacy fields, or the filled data form.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum RegistrationSubmission {
    Fields(Vec<(String, String)>),
    Form(Form),
}

/// Read the answer to a registration request: the `<query xmlns='jabber:iq:register'/>`.
pub fn parse_form(query: &Element) -> RegistrationForm {
    let text = |name: &str| {
        query
            .get_child(name, NS_REGISTER)
            .map(Element::text)
            .filter(|t| !t.trim().is_empty())
    };
    let oob = query.get_child("x", NS_OOB).and_then(|x| {
        Some(OobLink {
            url: x.get_child("url", NS_OOB)?.text().trim().to_owned(),
            desc: x
                .get_child("desc", NS_OOB)
                .map(Element::text)
                .filter(|d| !d.is_empty()),
        })
    });
    let form = Form::in_element(query);
    let fields = if form.is_some() {
        Vec::new()
    } else {
        query
            .children()
            .filter(|c| c.ns() == NS_REGISTER)
            .map(Element::name)
            .filter(|n| !matches!(*n, "instructions" | "registered" | "remove"))
            .map(str::to_owned)
            .collect()
    };
    RegistrationForm {
        instructions: text("instructions").or_else(|| form.as_ref()?.instructions.clone()),
        form,
        fields,
        oob,
        registered: query.get_child("registered", NS_REGISTER).is_some(),
    }
}

/// The `<query/>` of a registration request (a get) or a submission (a set).
pub fn submission_query(submission: &RegistrationSubmission) -> Element {
    let mut query = Element::builder("query", NS_REGISTER);
    match submission {
        RegistrationSubmission::Fields(fields) => {
            for (name, value) in fields {
                let name = NcName::try_from(name.as_str()).ok();
                let Some(name) = name else { continue };
                query = query.append(
                    Element::builder(name.as_str(), NS_REGISTER)
                        .append(value.as_str())
                        .build(),
                );
            }
        }
        RegistrationSubmission::Form(form) => query = query.append(form.submission()),
    }
    query.build()
}

pub(crate) enum Command {
    ChangePassword { password: String, reply: Reply },
    DeleteAccount { confirm: String, reply: Reply },
}

#[derive(Debug)]
pub(crate) enum Pending {
    /// The answer to the request for the fields. The new password waits for it.
    PasswordFields { password: String, reply: Reply },
    /// The answer to the change.
    PasswordSet { password: String, reply: Reply },
    /// The answer to the removal of the account.
    AccountRemoved { reply: Reply },
}

impl ClientHandle {
    /// Change the password of the account (XEP-0077, section 3.3). The session uses the
    /// new password from then on, so a reconnect works. The caller must store it (for
    /// example in the keychain).
    ///
    /// Fails with `Invalid` for an empty password, with `Server` when the server refuses
    /// (for example `not-acceptable` for a weak password or `not-allowed`), and with
    /// `Unsupported` when the server asks for more than the new password.
    pub async fn change_password(&self, password: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Register(Command::ChangePassword {
            password,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Delete the account on the server (XEP-0077, section 3.2). It cannot be undone: the
    /// server drops the roster, the archive and the private data, and closes the stream.
    ///
    /// `confirm` must be the address of the account, typed by the user. A text that differs
    /// (in case only is fine) fails with `Invalid` and sends nothing. Fails with `Server`
    /// when the server refuses (for example `not-allowed`).
    ///
    /// The caller must then log out, forget the saved password and decide what to do with
    /// the local data. The session cannot log in again.
    pub async fn delete_account(&self, confirm: &str) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Register(Command::DeleteAccount {
            confirm: confirm.to_owned(),
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    let (password, reply) = match command {
        Command::ChangePassword { password, reply } => (password, reply),
        Command::DeleteAccount { confirm, reply } => return delete_account(ctx, &confirm, reply),
    };
    if password.is_empty() {
        let _ = reply.send(Err(ClientError::Invalid("the password is empty".into())));
        return;
    }
    // Ask for the fields first: a server can want a data form.
    let iq = Iq::Get {
        from: None,
        to: None,
        id: String::new(),
        payload: Element::builder("query", NS_REGISTER).build(),
    };
    ctx.request(
        iq,
        FeaturePending::Register(Pending::PasswordFields { password, reply }),
    );
}

/// Send the removal, after a check that the typed address is the address of the account.
fn delete_account(ctx: &mut Ctx<'_>, confirm: &str, reply: Reply) {
    if !confirm.trim().eq_ignore_ascii_case(ctx.account.as_str()) {
        let _ = reply.send(Err(ClientError::Invalid(
            "the typed address is not the address of the account".into(),
        )));
        return;
    }
    let iq = Iq::Set {
        from: None,
        to: None,
        id: String::new(),
        payload: Element::builder("query", NS_REGISTER)
            .append(Element::builder("remove", NS_REGISTER).build())
            .build(),
    };
    ctx.request(
        iq,
        FeaturePending::Register(Pending::AccountRemoved { reply }),
    );
}

/// A command while no session is up.
pub(crate) fn offline(command: Command) {
    let (Command::ChangePassword { reply, .. } | Command::DeleteAccount { reply, .. }) = command;
    let _ = reply.send(Err(ClientError::NotConnected));
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::PasswordFields { password, reply } => {
            let form = match response {
                IqResponse::Result(Some(query)) => parse_form(&query),
                // A server that cannot answer the request may take the plain change.
                IqResponse::Result(None) | IqResponse::Error(_) => RegistrationForm::default(),
                IqResponse::Lost => {
                    let _ = reply.send(Err(ClientError::NotConnected));
                    return;
                }
            };
            let submission = match password_submission(
                ctx.account.node().map_or("", |n| n.as_str()),
                &password,
                form,
            ) {
                Ok(submission) => submission,
                Err(e) => {
                    let _ = reply.send(Err(e));
                    return;
                }
            };
            let iq = Iq::Set {
                from: None,
                to: None,
                id: String::new(),
                payload: submission_query(&submission),
            };
            ctx.request(
                iq,
                FeaturePending::Register(Pending::PasswordSet { password, reply }),
            );
        }
        Pending::AccountRemoved { reply } => {
            let outcome = match response {
                IqResponse::Result(_) => Ok(()),
                IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
                // The server closes the stream when it removes the account, and it may do
                // it before the answer arrives. Treat the loss as unknown, not as done.
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            let _ = reply.send(outcome);
        }
        Pending::PasswordSet { password, reply } => {
            let outcome = match response {
                IqResponse::Result(_) => {
                    // The session logs in again with the new password after a reconnect.
                    ctx.effects.push(Effect::NewPassword(password));
                    Ok(())
                }
                IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            let _ = reply.send(outcome);
        }
    }
}

/// The answer that changes the password: the legacy fields, or the data form of the
/// server with the username and the password filled in.
fn password_submission(
    username: &str,
    password: &str,
    asked: RegistrationForm,
) -> Result<RegistrationSubmission, ClientError> {
    let Some(mut form) = asked.form else {
        return Ok(RegistrationSubmission::Fields(vec![
            ("username".to_owned(), username.to_owned()),
            ("password".to_owned(), password.to_owned()),
        ]));
    };
    form.set("username", vec![username.to_owned()]);
    form.set("password", vec![password.to_owned()]);
    // A field that the client cannot know (the old password, a CAPTCHA) stops the change.
    let unknown = form.fields.iter().find(|f| {
        f.required
            && !matches!(f.kind, FieldKind::Fixed | FieldKind::Hidden)
            && !matches!(f.var.as_deref(), Some("username" | "password"))
            && f.values.iter().all(|v| v.is_empty())
    });
    if let Some(field) = unknown {
        let name = field
            .label
            .as_deref()
            .or(field.var.as_deref())
            .unwrap_or("?");
        return Err(ClientError::Unsupported(format!(
            "the server asks for more than the new password: {name}"
        )));
    }
    if form.get("password").is_none() {
        return Err(ClientError::Unsupported(
            "the registration form of the server has no password field".into(),
        ));
    }
    Ok(RegistrationSubmission::Form(form))
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

    fn change(h: &mut Harness, password: &str) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        let password = password.to_owned();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::Register(Command::ChangePassword { password, reply }),
            )
        });
        answer
    }

    fn respond(h: &mut Harness, response: IqResponse) {
        h.respond(|p| matches!(p, FeaturePending::Register(_)), response);
    }

    #[test]
    fn a_legacy_server_gets_the_username_and_the_password() {
        let mut h = Harness::new();
        let mut answer = change(&mut h, "new secret");
        let iqs = h.sent_iqs();
        assert!(
            matches!(&iqs[0], Iq::Get { to: None, payload, .. } if payload.is("query", NS_REGISTER))
        );
        let fields = "<query xmlns='jabber:iq:register'><registered/>\
            <username>alice</username><password/></query>";
        respond(&mut h, IqResponse::Result(Some(el(fields))));
        let iqs = h.sent_iqs();
        let Iq::Set { to, payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert!(to.is_none());
        assert!(payload.is("query", NS_REGISTER));
        let text = |name: &str| payload.get_child(name, NS_REGISTER).unwrap().text();
        assert_eq!(text("username"), "alice");
        assert_eq!(text("password"), "new secret");
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        // The session learns the new password.
        assert!(
            h.effects
                .iter()
                .any(|e| matches!(e, Effect::NewPassword(p) if p == "new secret"))
        );
    }

    #[test]
    fn a_form_server_gets_the_form_with_the_password() {
        let mut h = Harness::new();
        let mut answer = change(&mut h, "pw2");
        let _get = h.sent_iqs();
        let form = "<query xmlns='jabber:iq:register'><x xmlns='jabber:x:data' type='form'>\
            <field var='FORM_TYPE' type='hidden'><value>jabber:iq:register:changepassword</value></field>\
            <field var='username' type='text-single'><required/></field>\
            <field var='password' type='text-private'><required/></field></x></query>";
        respond(&mut h, IqResponse::Result(Some(el(form))));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        let x = payload.get_child("x", "jabber:x:data").unwrap();
        assert_eq!(x.attr("type"), Some("submit"));
        let value = |var: &str| {
            x.children()
                .find(|f| f.attr("var") == Some(var))
                .unwrap()
                .children()
                .next()
                .unwrap()
                .text()
        };
        assert_eq!(value("FORM_TYPE"), "jabber:iq:register:changepassword");
        assert_eq!(value("username"), "alice");
        assert_eq!(value("password"), "pw2");
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
    }

    #[test]
    fn a_form_with_an_unknown_required_field_is_unsupported() {
        let mut h = Harness::new();
        let mut answer = change(&mut h, "pw2");
        let _get = h.sent_iqs();
        let form = "<query xmlns='jabber:iq:register'><x xmlns='jabber:x:data' type='form'>\
            <field var='password' type='text-private'><required/></field>\
            <field var='old_password' type='text-private' label='Old password'><required/></field></x></query>";
        respond(&mut h, IqResponse::Result(Some(el(form))));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Unsupported(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_refusal_reaches_the_caller_and_keeps_the_old_password() {
        let mut h = Harness::new();
        let mut answer = change(&mut h, "x");
        respond(&mut h, IqResponse::Result(None));
        let _ = h.sent_iqs();
        let error = StanzaError::new(
            ErrorType::Modify,
            DefinedCondition::NotAcceptable,
            "en",
            "password too weak",
        );
        respond(&mut h, IqResponse::Error(error));
        match answer.try_recv().unwrap().unwrap() {
            Err(ClientError::Server(text)) => assert!(text.contains("password too weak")),
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            !h.effects
                .iter()
                .any(|e| matches!(e, Effect::NewPassword(_)))
        );
    }

    fn delete(h: &mut Harness, confirm: &str) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        let confirm = confirm.to_owned();
        h.with_ctx(|ctx| {
            features_command(
                ctx,
                FeatureCommand::Register(Command::DeleteAccount { confirm, reply }),
            )
        });
        answer
    }

    #[test]
    fn deleting_the_account_needs_the_typed_address() {
        let mut h = Harness::new();
        for wrong in [
            "",
            "alice",
            "bob@chord.localhost",
            "alice@chord.localhost.evil",
        ] {
            let mut answer = delete(&mut h, wrong);
            assert!(
                matches!(
                    answer.try_recv().unwrap().unwrap(),
                    Err(ClientError::Invalid(_))
                ),
                "{wrong}"
            );
        }
        assert!(
            h.sent_iqs().is_empty(),
            "nothing goes out on a wrong address"
        );
    }

    #[test]
    fn the_account_is_removed_with_a_remove_element() {
        let mut h = Harness::new();
        let typed = format!("  {}  ", crate::features::testing::ACCOUNT.to_uppercase());
        let mut answer = delete(&mut h, &typed);
        let iqs = h.sent_iqs();
        let Iq::Set { to, payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert!(to.is_none());
        assert!(payload.is("query", NS_REGISTER));
        assert!(payload.get_child("remove", NS_REGISTER).is_some());
        assert_eq!(payload.children().count(), 1);
        assert!(answer.try_recv().unwrap().is_none(), "no answer yet");
        respond(&mut h, IqResponse::Result(None));
        assert!(matches!(answer.try_recv().unwrap(), Some(Ok(()))));
    }

    #[test]
    fn a_refused_removal_is_a_server_error() {
        let mut h = Harness::new();
        let mut answer = delete(&mut h, crate::features::testing::ACCOUNT);
        let error = StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::NotAllowed,
            "en",
            "no deletion here",
        );
        respond(&mut h, IqResponse::Error(error));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Server(_))
        ));
    }

    #[test]
    fn an_empty_password_is_refused_before_sending() {
        let mut h = Harness::new();
        let mut answer = change(&mut h, "");
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Invalid(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn parse_form_reads_the_legacy_fields() {
        let q = el(
            "<query xmlns='jabber:iq:register'><instructions>Choose a name.</instructions>\
            <username/><password/><email/></query>",
        );
        let form = parse_form(&q);
        assert_eq!(form.instructions.as_deref(), Some("Choose a name."));
        assert_eq!(form.fields, vec!["username", "password", "email"]);
        assert!(form.form.is_none() && form.oob.is_none() && !form.registered);
    }

    #[test]
    fn parse_form_reads_a_data_form_with_a_captcha_and_a_link() {
        let q = el(
            "<query xmlns='jabber:iq:register'><instructions>Fill it.</instructions>\
            <x xmlns='jabber:x:data' type='form'>\
            <field var='username' type='text-single'><required/></field>\
            <field var='ocr' type='text-single'><required/><media xmlns='urn:xmpp:media-element'><uri type='image/png'>cid:sha1+a@bob.xmpp.org</uri></media></field></x>\
            <x xmlns='jabber:x:oob'><url>https://example.org/register</url><desc>Register on the web</desc></x>\
            <data xmlns='urn:xmpp:bob' cid='sha1+a@bob.xmpp.org' type='image/png'>AAAA</data></query>",
        );
        let form = parse_form(&q);
        assert!(form.fields.is_empty());
        let data = form.form.unwrap();
        assert_eq!(data.fields.len(), 2);
        assert!(
            data.fields[1].media[0]
                .uri
                .starts_with("data:image/png;base64,AAAA")
        );
        let oob = form.oob.unwrap();
        assert_eq!(oob.url, "https://example.org/register");
        assert_eq!(oob.desc.as_deref(), Some("Register on the web"));
        assert_eq!(form.instructions.as_deref(), Some("Fill it."));
    }

    #[test]
    fn a_link_only_answer_has_no_fields() {
        let q = el(
            "<query xmlns='jabber:iq:register'><instructions>Use the web page.</instructions>\
            <x xmlns='jabber:x:oob'><url>https://example.org/r</url></x></query>",
        );
        let form = parse_form(&q);
        assert!(form.fields.is_empty() && form.form.is_none());
        assert_eq!(form.oob.unwrap().url, "https://example.org/r");
    }

    #[test]
    fn a_registered_account_is_reported() {
        let q = el("<query xmlns='jabber:iq:register'><registered/><username>a</username></query>");
        assert!(parse_form(&q).registered);
    }

    #[test]
    fn submission_query_writes_the_fields_or_the_form() {
        let legacy = RegistrationSubmission::Fields(vec![
            ("username".into(), "bob".into()),
            ("password".into(), "p".into()),
            ("bad name".into(), "x".into()),
        ]);
        let q = submission_query(&legacy);
        assert_eq!(q.children().count(), 2);
        assert_eq!(q.get_child("username", NS_REGISTER).unwrap().text(), "bob");
        let mut form = Form::from_element(&el("<x xmlns='jabber:x:data' type='form'>\
            <field var='username' type='text-single'/></x>"))
        .unwrap();
        form.set("username", vec!["bob".into()]);
        let q = submission_query(&RegistrationSubmission::Form(form));
        assert_eq!(
            q.get_child("x", "jabber:x:data").unwrap().attr("type"),
            Some("submit")
        );
    }
}
