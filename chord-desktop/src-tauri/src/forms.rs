//! The commands for data forms and what uses them: ad-hoc commands with steps, the room
//! owner form, `change_password` and in-band registration. Each command calls one
//! chord-core function. The UI renders every form with one generic component.

use chord_core::features::adhoc::{CommandAction, CommandItem, CommandStep};
use chord_core::features::register::{RegistrationForm, RegistrationSubmission};
use chord_core::forms::Form;
use chord_core::jid::{BareJid, Jid};
use chord_core::session::register::{DEFAULT_REGISTER_TIMEOUT, RegisterError};
use tauri::State;

use crate::commands::parse_server;
use crate::error::{ChordError, Res};
use crate::keychain;
use crate::state::AppState;

impl From<RegisterError> for ChordError {
    fn from(error: RegisterError) -> Self {
        let code = match &error {
            RegisterError::Unreachable(_) => "unreachable",
            RegisterError::TlsInvalid(_) => "tlsInvalid",
            RegisterError::Timeout => "timeout",
            RegisterError::Refused(_) | RegisterError::Protocol(_) => "server",
        };
        Self::new(code, error.to_string())
    }
}

fn read_bare(s: &str) -> Res<BareJid> {
    s.parse()
        .map_err(|e| ChordError::invalid(format!("not a bare JID ({s:?}): {e}")))
}

fn read_full(s: &str) -> Res<Jid> {
    s.parse()
        .map_err(|e| ChordError::invalid(format!("not a JID ({s:?}): {e}")))
}

/// The commands that a service offers (XEP-0050).
#[tauri::command]
pub async fn list_commands(state: State<'_, AppState>, service: String) -> Res<Vec<CommandItem>> {
    Ok(state.handle()?.list_commands(read_full(&service)?).await?)
}

/// Run one step of a command. Start with `execute`, no session id and no form. For the next
/// steps, give the session id of the last answer, the action, and the filled form.
#[tauri::command]
pub async fn command_step(
    state: State<'_, AppState>,
    service: String,
    node: String,
    session_id: Option<String>,
    action: CommandAction,
    form: Option<Form>,
) -> Res<CommandStep> {
    Ok(state
        .handle()?
        .command_step(read_full(&service)?, node, session_id, action, form)
        .await?)
}

/// The whole configuration form of a room that we own (XEP-0045, section 10.2).
#[tauri::command]
pub async fn room_config_form(state: State<'_, AppState>, room: String) -> Res<Form> {
    Ok(state.handle()?.room_config_form(read_bare(&room)?).await?)
}

/// Send the filled configuration form of a room.
#[tauri::command]
pub async fn submit_room_config_form(
    state: State<'_, AppState>,
    room: String,
    form: Form,
) -> Res<()> {
    Ok(state
        .handle()?
        .submit_room_config_form(read_bare(&room)?, form)
        .await?)
}

/// Change the password of the account (XEP-0077). When the keychain holds a password for
/// the account, it gets the new one.
#[tauri::command]
pub async fn change_password(state: State<'_, AppState>, password: String) -> Res<()> {
    let client = state.client()?;
    client.handle.change_password(password.clone()).await?;
    let account = client.account.as_str();
    if keychain::get(account).ok().flatten().is_some() {
        keychain::set(account, &password)?;
    }
    Ok(())
}

/// Ask a server for its registration fields, before any login. `server` is as for `login`.
#[tauri::command]
pub async fn registration_form(domain: String, server: Option<String>) -> Res<RegistrationForm> {
    let server = parse_server(server.as_deref())?;
    Ok(
        chord_core::session::register::registration_form(&domain, server, DEFAULT_REGISTER_TIMEOUT)
            .await?,
    )
}

/// Create an account: `answer` has the filled data form, or the legacy fields.
#[tauri::command]
pub async fn register_account(
    domain: String,
    server: Option<String>,
    answer: RegistrationSubmission,
) -> Res<()> {
    let server = parse_server(server.as_deref())?;
    Ok(
        chord_core::session::register::register(&domain, server, &answer, DEFAULT_REGISTER_TIMEOUT)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_errors_keep_a_stable_code() {
        let cases = [
            (RegisterError::Unreachable("x".into()), "unreachable"),
            (RegisterError::TlsInvalid("x".into()), "tlsInvalid"),
            (RegisterError::Timeout, "timeout"),
            (RegisterError::Refused("Forbidden".into()), "server"),
            (RegisterError::Protocol("x".into()), "server"),
        ];
        for (error, code) in cases {
            assert_eq!(ChordError::from(error).code, code);
        }
    }

    #[test]
    fn the_ui_sends_a_form_and_a_registration_answer_as_json() {
        let form: Form = serde_json::from_value(serde_json::json!({
            "kind": "form",
            "title": null,
            "instructions": null,
            "fields": [{
                "var": "a", "kind": "list-multi", "label": null, "desc": null,
                "required": true, "values": ["x"],
                "options": [{"label": "X", "value": "x"}], "media": []
            }]
        }))
        .unwrap();
        assert_eq!(form.fields[0].values, vec!["x"]);
        let answer: RegistrationSubmission = serde_json::from_value(
            serde_json::json!({"fields": [["username", "a"], ["password", "b"]]}),
        )
        .unwrap();
        assert!(matches!(answer, RegistrationSubmission::Fields(f) if f.len() == 2));
        let action: CommandAction = serde_json::from_value(serde_json::json!("complete")).unwrap();
        assert_eq!(action, CommandAction::Complete);
    }

    #[test]
    fn a_step_leaves_as_camel_case_json() {
        let step = CommandStep {
            node: "n".into(),
            session_id: Some("s".into()),
            status: chord_core::features::adhoc::CommandStatus::Executing,
            actions: vec![CommandAction::Next],
            default_action: Some(CommandAction::Next),
            notes: vec![],
            form: None,
        };
        let json = serde_json::to_value(step).unwrap();
        assert_eq!(json["sessionId"], "s");
        assert_eq!(json["status"], "executing");
        assert_eq!(json["defaultAction"], "next");
    }
}
