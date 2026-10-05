//! Trust in the keys of devices: blind trust before verification (BTBV), and the device
//! views for the API.
//!
//! The rule: a new key of a contact is `Blind` (trusted at once) until the user marks any
//! key of that contact `Verified`. After that, a new key of that contact is `Undecided`.
//! `Undecided` and `Distrusted` devices get no message keys. An empty OMEMO message is the
//! one exception (XEP-0384): it moves no content.
//!
//! The trust is stored by account JID and by the `<ik>` bytes of the key, so a device that
//! comes back with the same key has the same trust.

use jid::BareJid;

use super::store::{self, TrustLevel};
use super::{OmemoDevice, crypto};
use crate::actor::{ClientError, ClientEvent};
use crate::features::Ctx;
use crate::store::Store;

/// True if a device with this trust may get message keys.
pub(super) fn usable(level: TrustLevel) -> bool {
    matches!(level, TrustLevel::Blind | TrustLevel::Verified)
}

/// The trust in a key. A key that we have not seen gets its level now (BTBV) and the level
/// is stored.
pub(super) fn level_for(
    store: &Store,
    account_id: i64,
    jid: &str,
    ik: &[u8],
) -> Result<TrustLevel, store::Error> {
    if let Some(level) = store::get_trust(store, account_id, jid, ik)? {
        return Ok(level);
    }
    let verified = store::list_trust(store, account_id, jid)?
        .iter()
        .any(|e| e.level == TrustLevel::Verified);
    let level = if verified {
        TrustLevel::Undecided
    } else {
        TrustLevel::Blind
    };
    store::set_trust(store, account_id, jid, ik, level)?;
    Ok(level)
}

/// How the key of a device compares with the one that we knew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyChange {
    Same,
    /// We had no key for this device.
    New,
    /// The device id has another key now. The old session is not valid.
    Replaced,
}

/// How the key `ik` compares with the one that we know for the device. It changes nothing.
pub(super) fn peek_key(ctx: &Ctx<'_>, jid: &BareJid, device: u32, ik: &[u8]) -> KeyChange {
    let known = store::list_devices(ctx.store, ctx.account_id, jid.as_str())
        .ok()
        .and_then(|rows| rows.into_iter().find(|d| d.device_id == device))
        .and_then(|d| d.identity_key);
    match known.as_deref() {
        None => KeyChange::New,
        Some(old) if old == ik => KeyChange::Same,
        Some(_) => KeyChange::Replaced,
    }
}

/// We see the key of a device (from a bundle, or from a key exchange). Store it, decide the
/// trust, and tell the frontends about a key that needs a decision. A changed key drops the
/// old session of the device.
pub(super) fn learn_key(
    ctx: &mut Ctx<'_>,
    jid: &BareJid,
    device: u32,
    ik: &[u8],
) -> (KeyChange, TrustLevel) {
    let change = peek_key(ctx, jid, device, ik);
    if change != KeyChange::Same {
        if let Err(e) = store::upsert_device(
            ctx.store,
            ctx.account_id,
            jid.as_str(),
            device,
            Some(ik),
            None,
        ) {
            ctx.store_error("store the key of an OMEMO device", e);
        }
        if change == KeyChange::Replaced
            && let Err(e) = store::delete_session(ctx.store, ctx.account_id, jid.as_str(), device)
        {
            ctx.store_error("drop the session of a changed key", e);
        }
    }
    let level = match level_for(ctx.store, ctx.account_id, jid.as_str(), ik) {
        Ok(level) => level,
        Err(e) => {
            ctx.store_error("read the OMEMO trust", e);
            TrustLevel::Undecided
        }
    };
    if change != KeyChange::Same {
        if level == TrustLevel::Undecided
            && let Some(fingerprint) = crypto::fingerprint(ik)
        {
            ctx.emit(ClientEvent::OmemoTrustNeeded {
                jid: jid.clone(),
                device_id: device,
                fingerprint,
            });
        }
        ctx.emit(ClientEvent::OmemoDevicesChanged(jid.clone()));
    }
    (change, level)
}

/// The text of a fingerprint without spaces, in lower case.
fn normalize(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_hexdigit)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The devices of `jid` for the API.
pub(super) fn devices(
    store: &Store,
    account_id: i64,
    account: &BareJid,
    jid: &BareJid,
) -> Result<Vec<OmemoDevice>, store::Error> {
    let own = jid == account;
    let this = if own {
        store::own_device_id(store, account_id)?
    } else {
        None
    };
    let rows = store::list_devices(store, account_id, jid.as_str())?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let trust = match &row.identity_key {
            Some(ik) => store::get_trust(store, account_id, jid.as_str(), ik)?,
            None => None,
        };
        out.push(OmemoDevice {
            jid: row.jid.clone(),
            device_id: row.device_id,
            fingerprint: row.identity_key.as_deref().and_then(crypto::fingerprint),
            label: row.label.clone(),
            trust: trust.map(Into::into),
            active: row.active,
            own,
            this_device: this == Some(row.device_id),
        });
    }
    Ok(out)
}

/// Set the trust in the key with this fingerprint. The key must belong to a device of
/// `jid` that we know.
pub(super) fn set_trust(
    store: &Store,
    account_id: i64,
    jid: &BareJid,
    fingerprint: &str,
    level: TrustLevel,
) -> Result<(), ClientError> {
    let wanted = normalize(fingerprint);
    if wanted.is_empty() {
        return Err(ClientError::Invalid("the fingerprint is empty".into()));
    }
    let rows = store::list_devices(store, account_id, jid.as_str())
        .map_err(|e| ClientError::Invalid(format!("store: {e}")))?;
    let ik = rows
        .iter()
        .filter_map(|r| r.identity_key.as_ref())
        .find(|ik| {
            crypto::fingerprint(ik)
                .map(|f| normalize(&f) == wanted)
                .unwrap_or(false)
        })
        .ok_or_else(|| ClientError::Invalid(format!("no key of {jid} has this fingerprint")))?;
    store::set_trust(store, account_id, jid.as_str(), ik, level)
        .map_err(|e| ClientError::Invalid(format!("store: {e}")))
}
