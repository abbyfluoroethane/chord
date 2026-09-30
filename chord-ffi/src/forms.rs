//! Data forms (XEP-0004) and what uses them: ad-hoc commands with steps (XEP-0050), the
//! room owner form (XEP-0045), `change_password` and in-band registration (XEP-0077).
//!
//! The methods live here, apart from `client.rs`, to keep that file small.

use chord_core::features::adhoc::{
    CommandAction as CoreAction, CommandItem as CoreItem, CommandStatus as CoreStatus,
    CommandStep as CoreStep,
};
use chord_core::features::register::{
    OobLink as CoreOob, RegistrationForm as CoreRegistration, RegistrationSubmission,
};
use chord_core::forms::{
    FieldKind as CoreKind, Form as CoreForm, FormField as CoreField, FormKind as CoreFormKind,
    FormMedia as CoreMedia, FormOption as CoreOption,
};
use chord_core::session::register::{DEFAULT_REGISTER_TIMEOUT, RegisterError};

use crate::client::{ChordClient, parse_server};
use crate::error::{ChordError, parse_bare, parse_jid};
use crate::types::FormField;

/// The type of a field of a data form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DataFieldKind {
    Boolean,
    Fixed,
    Hidden,
    JidMulti,
    JidSingle,
    ListMulti,
    ListSingle,
    TextMulti,
    TextPrivate,
    TextSingle,
}

/// The type of a whole data form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DataFormKind {
    Form,
    Submit,
    Cancel,
    Result,
}

/// One choice of a list field.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DataOption {
    pub label: Option<String>,
    pub value: String,
}

/// A media element of a field, for example a CAPTCHA image. A `cid:` image arrives as a
/// `data:` URI.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DataMedia {
    pub uri: String,
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// One field of a data form. The foreign code changes `values` and sends the form back.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DataField {
    /// Only a `Fixed` field has no name.
    pub var: Option<String>,
    pub kind: DataFieldKind,
    pub label: Option<String>,
    pub desc: Option<String>,
    pub required: bool,
    pub values: Vec<String>,
    pub options: Vec<DataOption>,
    pub media: Vec<DataMedia>,
}

/// A data form.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DataForm {
    pub kind: DataFormKind,
    pub title: Option<String>,
    pub instructions: Option<String>,
    pub fields: Vec<DataField>,
}

/// A command that a service offers.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandItem {
    pub jid: String,
    pub node: String,
    pub name: Option<String>,
}

/// What the client does in one step of a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CommandAction {
    Execute,
    Next,
    Prev,
    Complete,
    Cancel,
}

/// Where a command is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CommandStatus {
    Executing,
    Completed,
    Canceled,
}

/// A note of a command.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandNote {
    /// "info", "warn" or "error".
    pub kind: String,
    pub text: String,
}

/// The answer of a service to one step of a command.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandStep {
    pub node: String,
    /// Give it back with the next step.
    pub session_id: Option<String>,
    pub status: CommandStatus,
    pub actions: Vec<CommandAction>,
    pub default_action: Option<CommandAction>,
    pub notes: Vec<CommandNote>,
    pub form: Option<DataForm>,
}

/// A link that a server gives instead of a registration form.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OobLink {
    pub url: String,
    pub desc: Option<String>,
}

/// What a server wants for a registration.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RegistrationInfo {
    pub instructions: Option<String>,
    /// The data form. When it is set, answer with it.
    pub form: Option<DataForm>,
    /// The names of the legacy fields. Used when there is no data form.
    pub fields: Vec<String>,
    pub oob: Option<OobLink>,
    pub registered: bool,
}

/// The answer to a registration form: the filled data form, or the legacy fields.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RegistrationAnswer {
    pub form: Option<DataForm>,
    pub fields: Vec<FormField>,
}

// ---- Conversions ----

impl From<CoreKind> for DataFieldKind {
    fn from(k: CoreKind) -> Self {
        match k {
            CoreKind::Boolean => Self::Boolean,
            CoreKind::Fixed => Self::Fixed,
            CoreKind::Hidden => Self::Hidden,
            CoreKind::JidMulti => Self::JidMulti,
            CoreKind::JidSingle => Self::JidSingle,
            CoreKind::ListMulti => Self::ListMulti,
            CoreKind::ListSingle => Self::ListSingle,
            CoreKind::TextMulti => Self::TextMulti,
            CoreKind::TextPrivate => Self::TextPrivate,
            CoreKind::TextSingle => Self::TextSingle,
        }
    }
}

impl From<DataFieldKind> for CoreKind {
    fn from(k: DataFieldKind) -> Self {
        match k {
            DataFieldKind::Boolean => Self::Boolean,
            DataFieldKind::Fixed => Self::Fixed,
            DataFieldKind::Hidden => Self::Hidden,
            DataFieldKind::JidMulti => Self::JidMulti,
            DataFieldKind::JidSingle => Self::JidSingle,
            DataFieldKind::ListMulti => Self::ListMulti,
            DataFieldKind::ListSingle => Self::ListSingle,
            DataFieldKind::TextMulti => Self::TextMulti,
            DataFieldKind::TextPrivate => Self::TextPrivate,
            DataFieldKind::TextSingle => Self::TextSingle,
        }
    }
}

impl From<CoreFormKind> for DataFormKind {
    fn from(k: CoreFormKind) -> Self {
        match k {
            CoreFormKind::Form => Self::Form,
            CoreFormKind::Submit => Self::Submit,
            CoreFormKind::Cancel => Self::Cancel,
            CoreFormKind::Result => Self::Result,
        }
    }
}

impl From<DataFormKind> for CoreFormKind {
    fn from(k: DataFormKind) -> Self {
        match k {
            DataFormKind::Form => Self::Form,
            DataFormKind::Submit => Self::Submit,
            DataFormKind::Cancel => Self::Cancel,
            DataFormKind::Result => Self::Result,
        }
    }
}

impl From<CoreForm> for DataForm {
    fn from(f: CoreForm) -> Self {
        Self {
            kind: f.kind.into(),
            title: f.title,
            instructions: f.instructions,
            fields: f.fields.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<DataForm> for CoreForm {
    fn from(f: DataForm) -> Self {
        Self {
            kind: f.kind.into(),
            title: f.title,
            instructions: f.instructions,
            fields: f.fields.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<CoreField> for DataField {
    fn from(f: CoreField) -> Self {
        Self {
            var: f.var,
            kind: f.kind.into(),
            label: f.label,
            desc: f.desc,
            required: f.required,
            values: f.values,
            options: f
                .options
                .into_iter()
                .map(|o| DataOption {
                    label: o.label,
                    value: o.value,
                })
                .collect(),
            media: f
                .media
                .into_iter()
                .map(|m| DataMedia {
                    uri: m.uri,
                    mime: m.mime,
                    width: m.width,
                    height: m.height,
                })
                .collect(),
        }
    }
}

impl From<DataField> for CoreField {
    fn from(f: DataField) -> Self {
        Self {
            var: f.var,
            kind: f.kind.into(),
            label: f.label,
            desc: f.desc,
            required: f.required,
            values: f.values,
            options: f
                .options
                .into_iter()
                .map(|o| CoreOption {
                    label: o.label,
                    value: o.value,
                })
                .collect(),
            media: f
                .media
                .into_iter()
                .map(|m| CoreMedia {
                    uri: m.uri,
                    mime: m.mime,
                    width: m.width,
                    height: m.height,
                })
                .collect(),
        }
    }
}

impl From<CoreAction> for CommandAction {
    fn from(a: CoreAction) -> Self {
        match a {
            CoreAction::Execute => Self::Execute,
            CoreAction::Next => Self::Next,
            CoreAction::Prev => Self::Prev,
            CoreAction::Complete => Self::Complete,
            CoreAction::Cancel => Self::Cancel,
        }
    }
}

impl From<CommandAction> for CoreAction {
    fn from(a: CommandAction) -> Self {
        match a {
            CommandAction::Execute => Self::Execute,
            CommandAction::Next => Self::Next,
            CommandAction::Prev => Self::Prev,
            CommandAction::Complete => Self::Complete,
            CommandAction::Cancel => Self::Cancel,
        }
    }
}

impl From<CoreStatus> for CommandStatus {
    fn from(s: CoreStatus) -> Self {
        match s {
            CoreStatus::Executing => Self::Executing,
            CoreStatus::Completed => Self::Completed,
            CoreStatus::Canceled => Self::Canceled,
        }
    }
}

impl From<CoreItem> for CommandItem {
    fn from(i: CoreItem) -> Self {
        Self {
            jid: i.jid,
            node: i.node,
            name: i.name,
        }
    }
}

impl From<CoreStep> for CommandStep {
    fn from(s: CoreStep) -> Self {
        Self {
            node: s.node,
            session_id: s.session_id,
            status: s.status.into(),
            actions: s.actions.into_iter().map(Into::into).collect(),
            default_action: s.default_action.map(Into::into),
            notes: s
                .notes
                .into_iter()
                .map(|n| CommandNote {
                    kind: n.kind,
                    text: n.text,
                })
                .collect(),
            form: s.form.map(Into::into),
        }
    }
}

impl From<CoreOob> for OobLink {
    fn from(o: CoreOob) -> Self {
        Self {
            url: o.url,
            desc: o.desc,
        }
    }
}

impl From<CoreRegistration> for RegistrationInfo {
    fn from(r: CoreRegistration) -> Self {
        Self {
            instructions: r.instructions,
            form: r.form.map(Into::into),
            fields: r.fields,
            oob: r.oob.map(Into::into),
            registered: r.registered,
        }
    }
}

impl From<RegisterError> for ChordError {
    fn from(e: RegisterError) -> Self {
        match e {
            RegisterError::Unreachable(detail) => Self::Unreachable { detail },
            RegisterError::TlsInvalid(detail) => Self::TlsInvalid { detail },
            RegisterError::Timeout => Self::Timeout,
            RegisterError::Refused(detail) => Self::Server { detail },
            RegisterError::Protocol(detail) => Self::Internal { detail },
        }
    }
}

impl RegistrationAnswer {
    fn into_submission(self) -> RegistrationSubmission {
        match self.form {
            Some(form) => RegistrationSubmission::Form(form.into()),
            None => RegistrationSubmission::Fields(
                self.fields.into_iter().map(|f| (f.name, f.value)).collect(),
            ),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ChordClient {
    // ---- Ad-hoc commands with steps (XEP-0050) ----

    /// The commands that a service offers.
    pub async fn list_commands(&self, service: String) -> Result<Vec<CommandItem>, ChordError> {
        let service = parse_jid(&service)?;
        let items = self
            .call(move |h| async move { h.list_commands(service).await })
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Run one step of a command. Start with `Execute`, no session id and no form. For the
    /// next steps, give the session id of the last answer, the action, and the filled form.
    /// The command ends when the status is `Completed` or `Canceled`.
    pub async fn command_step(
        &self,
        service: String,
        node: String,
        session_id: Option<String>,
        action: CommandAction,
        form: Option<DataForm>,
    ) -> Result<CommandStep, ChordError> {
        let service = parse_jid(&service)?;
        let form = form.map(CoreForm::from);
        let step = self
            .call(move |h| async move {
                h.command_step(service, node, session_id, action.into(), form)
                    .await
            })
            .await?;
        Ok(step.into())
    }

    // ---- The owner form of a room (XEP-0045) ----

    /// The whole configuration form of a room that we own.
    pub async fn room_config_form(&self, room: String) -> Result<DataForm, ChordError> {
        let room = parse_bare(&room)?;
        let form = self
            .call(move |h| async move { h.room_config_form(room).await })
            .await?;
        Ok(form.into())
    }

    /// Send the filled configuration form of a room.
    pub async fn submit_room_config_form(
        &self,
        room: String,
        form: DataForm,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        let form = CoreForm::from(form);
        self.call(move |h| async move { h.submit_room_config_form(room, form).await })
            .await
    }

    // ---- Accounts (XEP-0077) ----

    /// Change the password of the account. The session uses the new one from then on.
    /// The caller must store it (the keychain).
    pub async fn change_password(&self, password: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.change_password(password).await })
            .await
    }

    /// Ask a server for its registration fields, before any login. `server` is as for
    /// `login` ("" finds the server with SRV records). Nothing changes on the server.
    pub async fn registration_form(
        &self,
        domain: String,
        server: String,
    ) -> Result<RegistrationInfo, ChordError> {
        let server = parse_server(&server)?;
        let task = self.rt.spawn(async move {
            chord_core::session::register::registration_form(
                &domain,
                server,
                DEFAULT_REGISTER_TIMEOUT,
            )
            .await
        });
        let form = task.await.map_err(|e| ChordError::Internal {
            detail: e.to_string(),
        })??;
        Ok(form.into())
    }

    /// Create an account. `answer` has the filled registration form, or the legacy fields.
    /// Log in with the new account afterwards.
    pub async fn register_account(
        &self,
        domain: String,
        server: String,
        answer: RegistrationAnswer,
    ) -> Result<(), ChordError> {
        let server = parse_server(&server)?;
        let submission = answer.into_submission();
        let task = self.rt.spawn(async move {
            chord_core::session::register::register(
                &domain,
                server,
                &submission,
                DEFAULT_REGISTER_TIMEOUT,
            )
            .await
        });
        task.await.map_err(|e| ChordError::Internal {
            detail: e.to_string(),
        })??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_survives_the_round_trip() {
        let form = CoreForm {
            kind: CoreFormKind::Form,
            title: Some("T".into()),
            instructions: Some("I".into()),
            fields: vec![CoreField {
                var: Some("v".into()),
                kind: CoreKind::ListMulti,
                label: Some("L".into()),
                desc: Some("D".into()),
                required: true,
                values: vec!["a".into(), "b".into()],
                options: vec![CoreOption {
                    label: Some("A".into()),
                    value: "a".into(),
                }],
                media: vec![CoreMedia {
                    uri: "data:image/png;base64,AA".into(),
                    mime: Some("image/png".into()),
                    width: Some(1),
                    height: Some(2),
                }],
            }],
        };
        let ffi = DataForm::from(form.clone());
        assert_eq!(ffi.fields[0].kind, DataFieldKind::ListMulti);
        assert_eq!(CoreForm::from(ffi), form);
    }

    #[test]
    fn every_field_kind_maps_back() {
        use DataFieldKind::*;
        for kind in [
            Boolean,
            Fixed,
            Hidden,
            JidMulti,
            JidSingle,
            ListMulti,
            ListSingle,
            TextMulti,
            TextPrivate,
            TextSingle,
        ] {
            assert_eq!(DataFieldKind::from(CoreKind::from(kind)), kind);
        }
    }

    #[test]
    fn a_registration_answer_picks_the_form_before_the_fields() {
        let answer = RegistrationAnswer {
            form: None,
            fields: vec![FormField {
                name: "username".into(),
                value: "a".into(),
            }],
        };
        assert!(matches!(
            answer.into_submission(),
            RegistrationSubmission::Fields(f) if f == vec![("username".to_owned(), "a".to_owned())]
        ));
    }

    #[test]
    fn a_refusal_maps_to_a_server_error() {
        let e = ChordError::from(RegisterError::Refused("Forbidden".into()));
        assert_eq!(
            e,
            ChordError::Server {
                detail: "Forbidden".into()
            }
        );
    }
}
