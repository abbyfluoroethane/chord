//! Data forms (XEP-0004) in the CLI: ad-hoc commands with steps, the room owner form,
//! and the account commands that use forms (in-band registration, change password).

use chord_core::features::adhoc::{CommandAction, CommandStatus, CommandStep};
use chord_core::features::register::{RegistrationForm, RegistrationSubmission};
use chord_core::forms::{FieldKind, Form, FormField};
use chord_core::jid::{BareJid, Jid};
use chord_core::session::register::{DEFAULT_REGISTER_TIMEOUT, register, registration_form};

use crate::json::{Obj, array};
use crate::{CliError, Client, Opts};

fn err(e: impl std::fmt::Display) -> CliError {
    CliError::from(e.to_string())
}

fn parse_jid(s: &str) -> Result<Jid, CliError> {
    Jid::new(s).map_err(|e| CliError::from(format!("bad JID {s}: {e}")))
}

/// `name=value` arguments. A repeated name adds a value (for list-multi and jid-multi).
fn assignments(args: &[&str], usage: &str) -> Result<Vec<(String, String)>, CliError> {
    args.iter()
        .map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .ok_or_else(|| CliError::from(usage.to_owned()))
        })
        .collect()
}

/// Put the assignments into the form. A name that the form does not have is an error.
fn fill(form: &mut Form, values: &[(String, String)]) -> Result<(), CliError> {
    let mut seen: Vec<&str> = Vec::new();
    for (name, value) in values {
        let Some(field) = form.get_mut(name) else {
            return Err(format!("the form has no field {name}").into());
        };
        if seen.contains(&name.as_str()) {
            field.values.push(value.clone());
        } else {
            field.values = vec![value.clone()];
            seen.push(name);
        }
    }
    Ok(())
}

fn kind_name(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Boolean => "boolean",
        FieldKind::Fixed => "fixed",
        FieldKind::Hidden => "hidden",
        FieldKind::JidMulti => "jid-multi",
        FieldKind::JidSingle => "jid-single",
        FieldKind::ListMulti => "list-multi",
        FieldKind::ListSingle => "list-single",
        FieldKind::TextMulti => "text-multi",
        FieldKind::TextPrivate => "text-private",
        FieldKind::TextSingle => "text-single",
    }
}

/// A private field shows no value.
fn shown_values(field: &FormField) -> String {
    if field.kind == FieldKind::TextPrivate && !field.value().is_empty() {
        return "<hidden>".to_owned();
    }
    field.values.join(" | ")
}

fn print_form(form: &Form) {
    if let Some(title) = &form.title {
        println!("  form: {title}");
    }
    if let Some(text) = &form.instructions {
        println!("  {text}");
    }
    for field in &form.fields {
        let var = field.var.as_deref().unwrap_or("-");
        let label = field.label.as_deref().unwrap_or("");
        let required = if field.required { " required" } else { "" };
        println!(
            "  {var} ({}{required}) {label}: {}",
            kind_name(field.kind),
            shown_values(field)
        );
        for option in &field.options {
            println!(
                "      option {} {}",
                option.value,
                option.label.as_deref().unwrap_or("")
            );
        }
        for media in &field.media {
            // A data URI is long. Name its type only.
            let uri = media.uri.split(',').next().unwrap_or("");
            println!("      media {uri}");
        }
    }
}

fn form_json(form: &Form) -> String {
    let fields = form.fields.iter().map(|f| {
        let options = array(f.options.iter().map(|o| {
            Obj::new()
                .str("value", &o.value)
                .opt_str("label", o.label.as_deref())
                .finish()
        }));
        let values = array(f.values.iter().map(|v| Obj::new().str("v", v).finish()));
        Obj::new()
            .opt_str("var", f.var.as_deref())
            .str("type", kind_name(f.kind))
            .opt_str("label", f.label.as_deref())
            .opt_str("desc", f.desc.as_deref())
            .bool("required", f.required)
            .raw("values", &values)
            .raw("options", &options)
            .finish()
    });
    Obj::new()
        .opt_str("title", form.title.as_deref())
        .opt_str("instructions", form.instructions.as_deref())
        .raw("fields", &array(fields))
        .finish()
}

fn status_name(status: CommandStatus) -> &'static str {
    match status {
        CommandStatus::Executing => "executing",
        CommandStatus::Completed => "completed",
        CommandStatus::Canceled => "canceled",
    }
}

fn print_step(opts: &Opts, number: usize, step: &CommandStep) {
    if opts.json {
        let actions = array(
            step.actions
                .iter()
                .map(|a| Obj::new().str("action", a.as_str()).finish()),
        );
        let mut o = Obj::new()
            .num("step", number as i64)
            .str("node", &step.node)
            .str("status", status_name(step.status))
            .opt_str("sessionId", step.session_id.as_deref())
            .raw("actions", &actions);
        if let Some(form) = &step.form {
            o = o.raw("form", &form_json(form));
        }
        println!("{}", o.finish());
        return;
    }
    println!(
        "step {number}: {} ({})",
        step.node,
        status_name(step.status)
    );
    for note in &step.notes {
        println!("  note [{}] {}", note.kind, note.text);
    }
    if !step.actions.is_empty() {
        let names: Vec<_> = step.actions.iter().map(|a| a.as_str()).collect();
        println!("  actions: {}", names.join(" "));
    }
    if let Some(form) = &step.form {
        print_form(form);
    }
}

/// `adhoc-list <jid>`: the commands that a service offers (XEP-0050).
pub async fn adhoc_list(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let [to] = args else {
        return Err("usage: adhoc-list <jid>".to_owned().into());
    };
    let items = client
        .handle
        .list_commands(parse_jid(to)?)
        .await
        .map_err(err)?;
    if opts.json {
        let items = items.iter().map(|i| {
            Obj::new()
                .str("jid", &i.jid)
                .str("node", &i.node)
                .opt_str("name", i.name.as_deref())
                .finish()
        });
        println!("{}", array(items));
    } else {
        println!("commands ({})", items.len());
        for item in &items {
            println!("  {} {}", item.node, item.name.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

/// At most this many steps, so that a command that never ends stops.
const MAX_STEPS: usize = 12;

/// `adhoc-run <jid> <node> [name=value ...]`: run a command through all its steps. Each
/// `name=value` fills that field in every step that has it. Other fields keep the default
/// that the service sends. The command goes `next`, then `complete` (the default action
/// that the service names). It stops when the command is completed.
pub async fn adhoc_run(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = "usage: adhoc-run <jid> <node> [name=value ...]";
    let [to, node, rest @ ..] = args else {
        return Err(usage.to_owned().into());
    };
    let to = parse_jid(to)?;
    let values = assignments(rest, usage)?;
    let mut step = client
        .handle
        .command_step(
            to.clone(),
            (*node).to_owned(),
            None,
            CommandAction::Execute,
            None,
        )
        .await
        .map_err(err)?;
    for number in 1..=MAX_STEPS {
        print_step(opts, number, &step);
        if step.status != CommandStatus::Executing {
            return Ok(());
        }
        let mut form = step.form.clone();
        if let Some(form) = &mut form {
            let own: Vec<_> = values
                .iter()
                .filter(|(name, _)| form.get(name).is_some())
                .cloned()
                .collect();
            fill(form, &own)?;
        }
        let action = step.default_action.unwrap_or(CommandAction::Complete);
        let action = if step.actions.is_empty() {
            CommandAction::Complete
        } else {
            action
        };
        step = client
            .handle
            .command_step(
                to.clone(),
                (*node).to_owned(),
                step.session_id.clone(),
                action,
                form,
            )
            .await
            .map_err(err)?;
    }
    Err(format!("the command has more than {MAX_STEPS} steps").into())
}

/// `room-form <room> [name=value ...]`: show the whole owner configuration form (XEP-0045,
/// 10.2). With assignments, submit the form with those values.
pub async fn room_form(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = "usage: room-form <room> [name=value ...]";
    let [room, rest @ ..] = args else {
        return Err(usage.to_owned().into());
    };
    let room = BareJid::new(room).map_err(|e| CliError::from(format!("bad room JID: {e}")))?;
    let values = assignments(rest, usage)?;
    let mut form = client
        .handle
        .room_config_form(room.clone())
        .await
        .map_err(err)?;
    if values.is_empty() {
        if opts.json {
            println!("{}", form_json(&form));
        } else {
            print_form(&form);
        }
        return Ok(());
    }
    fill(&mut form, &values)?;
    client
        .handle
        .submit_room_config_form(room.clone(), form)
        .await
        .map_err(err)?;
    println!("configured {room}");
    Ok(())
}

/// `passwd`: change the password of the account (XEP-0077). The new password comes from
/// CHORD_NEW_PASSWORD, never from an argument.
pub async fn passwd(client: &Client, args: &[&str]) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err("usage: passwd (new password in CHORD_NEW_PASSWORD)"
            .to_owned()
            .into());
    }
    let password = std::env::var("CHORD_NEW_PASSWORD")
        .map_err(|_| CliError::from("set CHORD_NEW_PASSWORD".to_owned()))?;
    client.handle.change_password(password).await.map_err(err)?;
    println!("password changed for {}", client.account);
    Ok(())
}

fn print_registration(opts: &Opts, domain: &str, form: &RegistrationForm) {
    if opts.json {
        let names = array(
            form.fields
                .iter()
                .map(|n| Obj::new().str("name", n).finish()),
        );
        let mut o = Obj::new()
            .str("domain", domain)
            .opt_str("instructions", form.instructions.as_deref())
            .raw("fields", &names)
            .bool("registered", form.registered)
            .opt_str("link", form.oob.as_ref().map(|o| o.url.as_str()));
        if let Some(f) = &form.form {
            o = o.raw("form", &form_json(f));
        }
        println!("{}", o.finish());
        return;
    }
    println!("registration at {domain}");
    if let Some(text) = &form.instructions {
        println!("  {text}");
    }
    if let Some(data) = &form.form {
        print_form(data);
    } else if !form.fields.is_empty() {
        println!("  fields: {}", form.fields.join(" "));
    }
    if let Some(oob) = &form.oob {
        println!(
            "  web page: {} {}",
            oob.url,
            oob.desc.as_deref().unwrap_or("")
        );
    }
}

/// The domain and the server of the registration commands. They use CHORD_JID, and they
/// need no password to read the form.
fn target() -> Result<(BareJid, chord_core::session::ServerAddr), CliError> {
    let jid = std::env::var("CHORD_JID").map_err(|_| CliError::from("set CHORD_JID".to_owned()))?;
    let jid = BareJid::new(&jid)
        .map_err(|e| CliError::from(format!("CHORD_JID is not a bare JID: {e}")))?;
    let server = crate::server_addr()?;
    Ok((jid, server))
}

/// `register-form`: read the registration fields of the server of CHORD_JID. No login,
/// nothing changes on the server.
pub async fn register_form(opts: &Opts, args: &[&str]) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err("usage: register-form".to_owned().into());
    }
    let (jid, server) = target()?;
    let domain = jid.domain().as_str().to_owned();
    let form = registration_form(&domain, server, DEFAULT_REGISTER_TIMEOUT)
        .await
        .map_err(err)?;
    print_registration(opts, &domain, &form);
    Ok(())
}

/// `register [name=value ...]`: create the account CHORD_JID on its server (XEP-0077). The
/// password is CHORD_PASSWORD. Other fields of the form (email, a CAPTCHA answer) come
/// as `name=value`.
pub async fn register_account(opts: &Opts, args: &[&str]) -> Result<(), CliError> {
    let usage = "usage: register [name=value ...] (password in CHORD_PASSWORD)";
    let values = assignments(args, usage)?;
    let (jid, server) = target()?;
    let password = std::env::var("CHORD_PASSWORD")
        .map_err(|_| CliError::from("set CHORD_PASSWORD".to_owned()))?;
    let domain = jid.domain().as_str().to_owned();
    let username = jid.node().map_or("", |n| n.as_str()).to_owned();
    let asked = registration_form(&domain, server.clone(), DEFAULT_REGISTER_TIMEOUT)
        .await
        .map_err(err)?;
    let submission = match (asked.form.clone(), asked.fields.is_empty()) {
        (Some(mut form), _) => {
            fill(&mut form, &values)?;
            form.set("username", vec![username]);
            form.set("password", vec![password]);
            let problems = form.problems();
            if !problems.is_empty() {
                print_registration(opts, &domain, &asked);
                return Err(problems.join(", ").into());
            }
            RegistrationSubmission::Form(form)
        }
        (None, false) => {
            let mut fields = Vec::new();
            for name in &asked.fields {
                let value = match name.as_str() {
                    "username" => username.clone(),
                    "password" => password.clone(),
                    other => values
                        .iter()
                        .find(|(k, _)| k == other)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| {
                            CliError::from(format!("the server wants {other}={{value}}"))
                        })?,
                };
                fields.push((name.clone(), value));
            }
            RegistrationSubmission::Fields(fields)
        }
        (None, true) => {
            print_registration(opts, &domain, &asked);
            return Err(
                "the server gives no registration form here, only the link above"
                    .to_owned()
                    .into(),
            );
        }
    };
    register(&domain, server, &submission, DEFAULT_REGISTER_TIMEOUT)
        .await
        .map_err(err)?;
    println!("registered {jid}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::from_element(
            &"<x xmlns='jabber:x:data' type='form'>\
            <field var='a' type='text-single'/><field var='m' type='list-multi'/></x>"
                .parse()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn fill_sets_and_repeats() {
        let mut f = form();
        let values = vec![
            ("a".to_owned(), "1".to_owned()),
            ("m".to_owned(), "x".to_owned()),
            ("m".to_owned(), "y".to_owned()),
        ];
        assert!(fill(&mut f, &values).is_ok());
        assert_eq!(f.get("a").unwrap().values, vec!["1"]);
        assert_eq!(f.get("m").unwrap().values, vec!["x", "y"]);
        assert!(fill(&mut f, &[("nope".to_owned(), "1".to_owned())]).is_err());
    }

    #[test]
    fn a_private_value_stays_hidden() {
        let mut f = FormField {
            var: Some("p".into()),
            kind: FieldKind::TextPrivate,
            values: vec!["secret".into()],
            ..Default::default()
        };
        assert_eq!(shown_values(&f), "<hidden>");
        f.kind = FieldKind::TextSingle;
        assert_eq!(shown_values(&f), "secret");
    }

    #[test]
    fn the_json_of_a_form_lists_the_fields() {
        let json = form_json(&form());
        assert!(json.contains(r#""var":"a""#) && json.contains(r#""type":"list-multi""#));
    }
}
