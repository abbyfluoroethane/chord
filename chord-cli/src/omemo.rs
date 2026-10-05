//! OMEMO commands: the state of a chat, devices, trust, and the events.

use chord_core::actor::ClientEvent;
use chord_core::features::omemo::{OmemoDevice, OmemoTrust};
use chord_core::jid::BareJid;

use crate::json::{Obj, array};
use crate::{CliError, Client, Opts};

fn bare(s: &str) -> Result<BareJid, CliError> {
    BareJid::new(s).map_err(|e| CliError::from(format!("bad JID {s}: {e}")))
}

fn err(e: impl std::fmt::Display) -> CliError {
    CliError::from(e.to_string())
}

/// The name of a trust level.
pub fn trust_name(trust: OmemoTrust) -> &'static str {
    match trust {
        OmemoTrust::Undecided => "undecided",
        OmemoTrust::Blind => "blind",
        OmemoTrust::Verified => "verified",
        OmemoTrust::Distrusted => "distrusted",
    }
}

/// The trust level for the word of `omemo-trust`.
fn parse_trust(word: &str) -> Option<OmemoTrust> {
    match word {
        "verify" => Some(OmemoTrust::Verified),
        "distrust" => Some(OmemoTrust::Distrusted),
        "blind" => Some(OmemoTrust::Blind),
        "undecided" => Some(OmemoTrust::Undecided),
        _ => None,
    }
}

/// One line for a device.
pub fn device_line(d: &OmemoDevice) -> String {
    let mut line = format!(
        "{}  {}  {}",
        d.device_id,
        d.fingerprint.as_deref().unwrap_or("(no key yet)"),
        d.trust.map_or("unknown", trust_name),
    );
    if d.this_device {
        line.push_str("  (this device)");
    }
    if !d.active {
        line.push_str("  (not in the list)");
    }
    if let Some(label) = &d.label {
        line.push_str(&format!("  \"{label}\""));
    }
    line
}

fn device_json(d: &OmemoDevice) -> String {
    Obj::new()
        .str("jid", &d.jid)
        .num("device_id", i64::from(d.device_id))
        .opt_str("fingerprint", d.fingerprint.as_deref())
        .opt_str("label", d.label.as_deref())
        .opt_str("trust", d.trust.map(trust_name))
        .bool("active", d.active)
        .bool("own", d.own)
        .bool("this_device", d.this_device)
        .finish()
}

/// `omemo <jid> [on|off]`: show or set OMEMO for a chat. Works offline.
pub async fn omemo(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || CliError::from("usage: omemo <jid> [on|off]".to_owned());
    let (peer, rest) = args.split_first().ok_or_else(usage)?;
    let peer = bare(peer)?;
    match rest {
        [] => {}
        ["on"] => client
            .handle
            .set_omemo_enabled(peer.clone(), true)
            .await
            .map_err(err)?,
        ["off"] => client
            .handle
            .set_omemo_enabled(peer.clone(), false)
            .await
            .map_err(err)?,
        _ => return Err(usage()),
    }
    let on = client
        .handle
        .omemo_enabled(peer.clone())
        .await
        .map_err(err)?;
    if opts.json {
        println!(
            "{}",
            Obj::new()
                .str("jid", peer.as_str())
                .bool("enabled", on)
                .finish()
        );
    } else {
        println!("omemo with {peer}: {}", if on { "on" } else { "off" });
    }
    Ok(())
}

/// `omemo-devices [<jid>]`: the devices in the store. No JID: our own account.
pub async fn devices(opts: &Opts, client: &Client, args: &[&str]) -> Result<(), CliError> {
    let list = match args {
        [] => client.handle.own_omemo_devices().await,
        [jid] => client.handle.omemo_devices(bare(jid)?).await,
        _ => return Err("usage: omemo-devices [<jid>]".to_owned().into()),
    }
    .map_err(err)?;
    if opts.json {
        println!("{}", array(list.iter().map(device_json)));
    } else {
        println!("devices ({})", list.len());
        for d in &list {
            println!("  {}", device_line(d));
        }
    }
    Ok(())
}

/// `omemo-refresh <jid>`: fetch the device list and the bundles from the server.
pub async fn refresh(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let [jid] = args else {
        return Err("usage: omemo-refresh <jid>".to_owned().into());
    };
    let jid = bare(jid)?;
    client
        .handle
        .refresh_omemo_devices(jid.clone())
        .await
        .map_err(err)?;
    println!("refreshed the devices of {jid}");
    Ok(())
}

/// `omemo-trust <jid> <fingerprint> verify|distrust|blind|undecided`. Works offline.
pub async fn trust(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::from(
            "usage: omemo-trust <jid> <fingerprint> verify|distrust|blind|undecided".to_owned(),
        )
    };
    let [jid, fingerprint, word] = args else {
        return Err(usage());
    };
    let level = parse_trust(word).ok_or_else(usage)?;
    client
        .handle
        .set_omemo_trust(bare(jid)?, (*fingerprint).to_owned(), level)
        .await
        .map_err(err)?;
    println!("{jid}: {fingerprint} is {}", trust_name(level));
    Ok(())
}

/// `omemo-remove-device <device-id>`: remove one of our other devices.
pub async fn remove_device(client: &Client, args: &[&str]) -> Result<(), CliError> {
    let [id] = args else {
        return Err("usage: omemo-remove-device <device-id>".to_owned().into());
    };
    let id: u32 = id
        .parse()
        .map_err(|_| CliError::from(format!("bad device id: {id}")))?;
    client
        .handle
        .remove_own_omemo_device(id)
        .await
        .map_err(err)?;
    println!("removed device {id}");
    Ok(())
}

/// The line for an OMEMO event. `None` for any other event.
pub fn event_line(event: &ClientEvent) -> Option<String> {
    match event {
        ClientEvent::OmemoKeysReset => {
            Some("omemo: the keys of this device were lost, made a new device".to_owned())
        }
        ClientEvent::OmemoTrustNeeded {
            jid,
            device_id,
            fingerprint,
        } => Some(format!(
            "omemo: {jid} device {device_id} needs a trust decision: {fingerprint}"
        )),
        ClientEvent::OmemoDevicesChanged(jid) => {
            Some(format!("omemo: the devices of {jid} changed"))
        }
        ClientEvent::OmemoSendFailed {
            peer,
            message_id,
            reason,
        } => Some(format!(
            "omemo: message {message_id} to {peer} not sent: {reason}"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> OmemoDevice {
        OmemoDevice {
            jid: "a@x.test".into(),
            device_id: 7,
            fingerprint: Some("aa bb".into()),
            label: None,
            trust: Some(OmemoTrust::Verified),
            active: true,
            own: false,
            this_device: false,
        }
    }

    #[test]
    fn trust_words() {
        assert_eq!(parse_trust("verify"), Some(OmemoTrust::Verified));
        assert_eq!(parse_trust("distrust"), Some(OmemoTrust::Distrusted));
        assert_eq!(parse_trust("blind"), Some(OmemoTrust::Blind));
        assert_eq!(parse_trust("undecided"), Some(OmemoTrust::Undecided));
        assert_eq!(parse_trust("verified"), None);
    }

    #[test]
    fn device_lines() {
        assert_eq!(device_line(&device()), "7  aa bb  verified");
        let mut d = device();
        d.fingerprint = None;
        d.trust = None;
        d.this_device = true;
        d.active = false;
        assert_eq!(
            device_line(&d),
            "7  (no key yet)  unknown  (this device)  (not in the list)"
        );
    }

    #[test]
    fn device_json_has_trust() {
        assert!(device_json(&device()).contains(r#""trust":"verified""#));
    }

    #[test]
    fn event_lines() {
        let jid = BareJid::new("b@x.test").unwrap();
        let line = event_line(&ClientEvent::OmemoDevicesChanged(jid.clone())).unwrap();
        assert!(line.contains("b@x.test"));
        let failed = ClientEvent::OmemoSendFailed {
            peer: jid,
            message_id: "m1".into(),
            reason: "no key".into(),
        };
        assert!(event_line(&failed).unwrap().contains("not sent: no key"));
        assert!(event_line(&ClientEvent::BlockListChanged).is_none());
    }
}
