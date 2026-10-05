//! Our own device: the keys, the device list, the bundle, and the device lists of the
//! contacts.
//!
//! At each new session `init` loads the keys, or makes them on the first start. When
//! service discovery is done, `start` reads our device list. If our id is missing, we add
//! it (we never drop the ids of other devices) and publish the list and the bundle. If our
//! id is there, we compare the published bundle with the local keys and publish it again if
//! it is different. A PEP event for our own list does the same check, so a client that
//! removes our id by mistake does not remove us for long.

use jid::BareJid;
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::stanza_error::DefinedCondition;

use super::store::{self, Sealer};
use super::xml::{self, BundleXml, Device, DeviceList};
use super::{Engine, Pending, Reply, crypto, jobs};
use crate::actor::{ClientError, ClientEvent};
use crate::features::{Ctx, IqResponse, Pending as FeaturePending, muc};

/// A signed prekey that is older than this is replaced (7 days).
const ROTATE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// A signed prekey that a newer key replaced is deleted after this time (30 days).
const KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Load our keys, or make them. The result goes in `state.omemo.engine`. If it fails,
/// `state.omemo.problem` says why, and every encrypted send fails with that text.
pub(super) fn init(ctx: &mut Ctx<'_>) {
    ctx.state.omemo.engine = None;
    ctx.state.omemo.problem = None;
    if !crypto::AVAILABLE {
        ctx.state.omemo.problem = Some("this build has no OMEMO".into());
        return;
    }
    match load_engine(ctx) {
        Ok(engine) => {
            ctx.state.omemo.engine = Some(engine);
        }
        Err(problem) => {
            log::error!("OMEMO keys: {problem}");
            ctx.state.omemo.problem = Some(problem);
        }
    }
}

fn open_sealer(ctx: &Ctx<'_>) -> Result<Sealer, store::Error> {
    let secrets = ctx.state.omemo.secrets.clone();
    Sealer::open_or_create(
        ctx.store,
        ctx.account_id,
        ctx.account.as_str(),
        secrets.as_deref(),
    )
}

/// The data key or the keys of this device are gone. Start a new device. The trust choices
/// and the switches of the chats stay: they are the choices of the user, not keys of ours.
fn start_over(ctx: &mut Ctx<'_>) -> Result<Sealer, String> {
    let account_id = ctx.account_id;
    let trust: Vec<(String, Vec<u8>, store::TrustLevel)> = ctx
        .store
        .conn()
        .prepare("SELECT jid, fingerprint, level FROM omemo_trust WHERE account_id = ?1")
        .and_then(|mut stmt| {
            stmt.query_map([account_id], |r| {
                let level: String = r.get(2)?;
                Ok((r.get(0)?, r.get(1)?, level))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(jid, fp, level)| {
            let level = match level.as_str() {
                "undecided" => store::TrustLevel::Undecided,
                "blind" => store::TrustLevel::Blind,
                "verified" => store::TrustLevel::Verified,
                "distrusted" => store::TrustLevel::Distrusted,
                _ => return None,
            };
            Some((jid, fp, level))
        })
        .collect();
    let chats: Vec<String> = ctx
        .store
        .conn()
        .prepare("SELECT jid FROM omemo_conversations WHERE account_id = ?1 AND enabled = 1")
        .and_then(|mut stmt| {
            stmt.query_map([account_id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .map_err(|e| e.to_string())?;
    store::reset(ctx.store, account_id).map_err(|e| e.to_string())?;
    for (jid, fp, level) in trust {
        store::set_trust(ctx.store, account_id, &jid, &fp, level).map_err(|e| e.to_string())?;
    }
    for jid in chats {
        store::set_conversation_enabled(ctx.store, account_id, &jid, true)
            .map_err(|e| e.to_string())?;
    }
    ctx.emit(ClientEvent::OmemoKeysReset);
    open_sealer(ctx).map_err(|e| e.to_string())
}

fn load_engine(ctx: &mut Ctx<'_>) -> Result<Engine, String> {
    let mut sealer = match open_sealer(ctx) {
        Ok(sealer) => sealer,
        Err(store::Error::KeysLost) => start_over(ctx)?,
        Err(e) => return Err(e.to_string()),
    };
    let loaded = match store::load_identity(ctx.store, &sealer, ctx.account_id) {
        Ok(loaded) => loaded,
        Err(store::Error::Open) => {
            // A blob does not open: the keys of this device are not usable.
            sealer = start_over(ctx)?;
            None
        }
        Err(e) => return Err(e.to_string()),
    };
    let keys = match loaded {
        Some((_, blob)) => crypto::Keys::from_bytes(&blob)?,
        None => {
            let keys = crypto::Keys::generate()?;
            store::save_identity(
                ctx.store,
                &sealer,
                ctx.account_id,
                keys.device_id(),
                &keys.to_bytes(),
            )
            .map_err(|e| e.to_string())?;
            store::upsert_device(
                ctx.store,
                ctx.account_id,
                ctx.account.as_str(),
                keys.device_id(),
                Some(keys.identity_key()),
                None,
            )
            .map_err(|e| e.to_string())?;
            ctx.state.omemo.fresh = true;
            keys
        }
    };
    Ok(Engine { sealer, keys })
}

/// Save the keys after a change.
pub(super) fn save_keys(ctx: &mut Ctx<'_>) {
    let Some(engine) = &ctx.state.omemo.engine else {
        return;
    };
    if let Err(e) = store::save_identity(
        ctx.store,
        &engine.sealer,
        ctx.account_id,
        engine.keys.device_id(),
        &engine.keys.to_bytes(),
    ) {
        ctx.store_error("save the OMEMO keys", e);
    }
}

/// Service discovery is done: read our device list, and look after the keys.
pub(super) fn start(ctx: &mut Ctx<'_>) {
    if ctx.state.omemo.engine.is_none() {
        return;
    }
    maintain(ctx);
    let own = ctx.account.clone();
    jobs::request_list(ctx, &own);
}

/// The answer to the fetch of our own device list. `list` is `None` if the fetch failed
/// in a way that does not say "there is no list". We change nothing then.
pub(super) fn after_own_list(ctx: &mut Ctx<'_>, list: Option<DeviceList>) {
    let Some(engine) = &ctx.state.omemo.engine else {
        return;
    };
    let Some(mut list) = list else {
        return;
    };
    let id = engine.keys.device_id();
    if list.devices.iter().all(|d| d.id != id) {
        list.devices.push(Device {
            id,
            label: None,
            labelsig: None,
        });
        publish_list(ctx, &list);
        publish_bundle(ctx);
        ctx.state.omemo.fresh = false;
    } else if std::mem::take(&mut ctx.state.omemo.fresh) {
        publish_bundle(ctx);
    } else {
        ctx.request(
            xml::fetch_bundle_iq(None, id),
            FeaturePending::Omemo(Pending::OwnBundle),
        );
    }
}

pub(super) fn publish_list(ctx: &mut Ctx<'_>, list: &DeviceList) {
    ctx.request_publish(
        xml::publish_devices_iq(list),
        FeaturePending::Omemo(Pending::PublishList),
    );
}

pub(super) fn publish_bundle(ctx: &mut Ctx<'_>) {
    let Some(engine) = &ctx.state.omemo.engine else {
        return;
    };
    let id = engine.keys.device_id();
    match engine.keys.bundle() {
        Ok(bundle) => {
            ctx.request_publish(
                xml::publish_bundle_iq(id, &bundle),
                FeaturePending::Omemo(Pending::PublishBundle),
            );
        }
        Err(e) => log::error!("OMEMO bundle: {e}"),
    }
}

/// Compare the bundle that the server has with the local keys. Publish if they differ.
pub(super) fn on_own_bundle(ctx: &mut Ctx<'_>, response: IqResponse) {
    let Some(engine) = &ctx.state.omemo.engine else {
        return;
    };
    let Ok(local) = engine.keys.bundle() else {
        return;
    };
    let published = match response {
        IqResponse::Lost => return,
        IqResponse::Result(Some(payload)) => xml::parse_bundle_result(&payload).ok(),
        IqResponse::Result(None) | IqResponse::Error(_) => None,
    };
    if !published.is_some_and(|p| same_bundle(&p, &local)) {
        publish_bundle(ctx);
    }
}

fn same_bundle(a: &BundleXml, b: &BundleXml) -> bool {
    let ids = |x: &BundleXml| {
        let mut ids: Vec<u32> = x.prekeys.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids
    };
    a.ik == b.ik && a.spk_id == b.spk_id && a.spk == b.spk && ids(a) == ids(b)
}

pub(super) fn on_published(ctx: &mut Ctx<'_>, what: &str, response: IqResponse) {
    if let IqResponse::Error(e) = response {
        let text = muc::error_text(&e);
        log::warn!("cannot publish the OMEMO {what}: {text}");
        ctx.emit(ClientEvent::Notice(format!(
            "Chord could not publish the OMEMO {what}: {text}"
        )));
    }
}

/// A one-time prekey was used by a key exchange. Delete it, make new ones if few are left,
/// and publish the bundle.
pub(super) fn after_used_prekey(ctx: &mut Ctx<'_>, prekey: u32) {
    let Some(engine) = &mut ctx.state.omemo.engine else {
        return;
    };
    engine.keys.remove_prekey(prekey);
    if engine.keys.prekey_count() < crypto::PREKEY_MIN
        && let Err(e) = engine.keys.refill_prekeys()
    {
        log::error!("OMEMO prekeys: {e}");
    }
    save_keys(ctx);
    publish_bundle(ctx);
}

/// Replace the signed prekey every 7 days, and delete the old ones 30 days after their
/// replacement.
pub(super) fn maintain(ctx: &mut Ctx<'_>) {
    let now = super::now_ms(ctx.store);
    maintain_at(ctx, now);
}

/// `maintain` at a given time (Unix ms), so that a test can move the clock.
pub(super) fn maintain_at(ctx: &mut Ctx<'_>, now: i64) {
    let Some(engine) = &mut ctx.state.omemo.engine else {
        return;
    };
    let keys = &mut engine.keys;
    let Ok(mut list) = keys.signed_prekeys() else {
        return;
    };
    let current = keys.current_signed_prekey_id();
    let created = |id: u32, list: &[(u32, u64)]| {
        list.iter()
            .find(|(i, _)| *i == id)
            .map(|(_, t)| *t as i64)
            .unwrap_or(now)
    };
    let mut rotated = false;
    if now - created(current, &list) > ROTATE_MS {
        match keys.rotate_signed_prekey() {
            Ok(()) => {
                rotated = true;
                list = keys.signed_prekeys().unwrap_or(list);
            }
            Err(e) => log::error!("OMEMO signed prekey: {e}"),
        }
    }
    // A key was replaced when the next newer key was made.
    let current = keys.current_signed_prekey_id();
    list.sort_by_key(|(_, t)| *t);
    let mut dropped = false;
    for pair in list.windows(2) {
        let (id, _) = pair[0];
        let replaced_at = pair[1].1 as i64;
        if id != current && now - replaced_at > KEEP_MS {
            dropped |= keys.discard_signed_prekey(id);
        }
    }
    if rotated || dropped {
        save_keys(ctx);
    }
    if rotated {
        publish_bundle(ctx);
    }
}

/// Update the stored devices of `jid` from a device list: the ids in the list are active,
/// the others are not. Returns true if something changed.
pub(super) fn apply_list(ctx: &mut Ctx<'_>, jid: &BareJid, list: &DeviceList) -> bool {
    let account_id = ctx.account_id;
    let before = store::list_devices(ctx.store, account_id, jid.as_str()).unwrap_or_default();
    let mut ids: Vec<u32> = list.devices.iter().map(|d| d.id).collect();
    // Our own id stays active, even if a list does not hold it.
    if *jid == *ctx.account
        && let Some(engine) = &ctx.state.omemo.engine
    {
        ids.push(engine.keys.device_id());
    }
    for d in &list.devices {
        if let Err(e) = store::upsert_device(
            ctx.store,
            account_id,
            jid.as_str(),
            d.id,
            None,
            d.label.as_deref(),
        ) {
            ctx.store_error("store an OMEMO device", e);
        }
    }
    let gone =
        store::mark_devices_inactive_except(ctx.store, account_id, jid.as_str(), &ids).unwrap_or(0);
    let new = ids.iter().any(|id| {
        before
            .iter()
            .find(|b| b.device_id == *id)
            .is_none_or(|b| !b.active)
    });
    gone > 0 || new
}

/// A PEP event on the device list node.
pub(super) fn on_devices_event(ctx: &mut Ctx<'_>, from: &BareJid, payload: Payload) {
    let Ok(list) = xml::parse_devices_event(&payload) else {
        return;
    };
    if apply_list(ctx, from, &list) {
        ctx.emit(ClientEvent::OmemoDevicesChanged(from.clone()));
    }
    if *from == *ctx.account {
        // A list without our id: add it back. The echo of our own publish has the id.
        after_own_list_event(ctx, list);
    }
}

fn after_own_list_event(ctx: &mut Ctx<'_>, mut list: DeviceList) {
    let Some(engine) = &ctx.state.omemo.engine else {
        return;
    };
    let id = engine.keys.device_id();
    if list.devices.iter().all(|d| d.id != id) {
        list.devices.push(Device {
            id,
            label: None,
            labelsig: None,
        });
        publish_list(ctx, &list);
    }
}

/// Remove one of our other devices from our device list.
pub(super) fn remove_own(ctx: &mut Ctx<'_>, device_id: u32, reply: Reply<()>) {
    let Some(engine) = &ctx.state.omemo.engine else {
        let text = ctx.state.omemo.problem.clone().unwrap_or_default();
        let _ = reply.send(Err(ClientError::Invalid(format!(
            "OMEMO is not ready: {text}"
        ))));
        return;
    };
    if engine.keys.device_id() == device_id {
        let _ = reply.send(Err(ClientError::Invalid(
            "this device cannot remove itself".into(),
        )));
        return;
    }
    ctx.request(
        xml::fetch_devices_iq(None),
        FeaturePending::Omemo(Pending::RemoveFetch {
            device: device_id,
            reply,
        }),
    );
}

/// A list read from an answer. `None` for an answer that says nothing about the list.
pub(super) fn list_of(response: &IqResponse) -> Option<DeviceList> {
    match response {
        IqResponse::Result(Some(payload)) => match xml::parse_devices_result(payload) {
            Ok(list) => Some(list),
            Err(xml::Error::NoItem) => Some(DeviceList::default()),
            Err(e) => {
                log::warn!("a device list that does not parse: {e}");
                None
            }
        },
        IqResponse::Result(None) => Some(DeviceList::default()),
        IqResponse::Error(e) if e.defined_condition == DefinedCondition::ItemNotFound => {
            Some(DeviceList::default())
        }
        IqResponse::Error(_) | IqResponse::Lost => None,
    }
}

pub(super) fn on_remove_fetch(
    ctx: &mut Ctx<'_>,
    device: u32,
    reply: Reply<()>,
    response: IqResponse,
) {
    if matches!(response, IqResponse::Lost) {
        let _ = reply.send(Err(ClientError::NotConnected));
        return;
    }
    let Some(mut list) = list_of(&response) else {
        let text = match &response {
            IqResponse::Error(e) => muc::error_text(e),
            _ => "the device list is not valid".into(),
        };
        let _ = reply.send(Err(ClientError::Server(text)));
        return;
    };
    let before = list.devices.len();
    list.devices.retain(|d| d.id != device);
    if list.devices.len() == before {
        // The id is not in the list. Nothing to do on the server.
        finish_remove(ctx, device);
        let _ = reply.send(Ok(()));
        return;
    }
    // The list keeps our own id.
    if let Some(engine) = &ctx.state.omemo.engine {
        let own = engine.keys.device_id();
        if list.devices.iter().all(|d| d.id != own) {
            list.devices.push(Device {
                id: own,
                label: None,
                labelsig: None,
            });
        }
    }
    ctx.request_publish(
        xml::publish_devices_iq(&list),
        FeaturePending::Omemo(Pending::RemovePublish { device, reply }),
    );
}

pub(super) fn on_remove_publish(
    ctx: &mut Ctx<'_>,
    device: u32,
    reply: Reply<()>,
    response: IqResponse,
) {
    let result = match response {
        IqResponse::Result(_) => {
            finish_remove(ctx, device);
            Ok(())
        }
        IqResponse::Error(e) => Err(ClientError::Server(muc::error_text(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    let _ = reply.send(result);
}

/// The device is out of our list. Mark it inactive and drop its session.
fn finish_remove(ctx: &mut Ctx<'_>, device: u32) {
    let own = ctx.account.clone();
    let rows = store::list_devices(ctx.store, ctx.account_id, own.as_str()).unwrap_or_default();
    let keep: Vec<u32> = rows
        .iter()
        .filter(|d| d.active && d.device_id != device)
        .map(|d| d.device_id)
        .collect();
    if let Err(e) =
        store::mark_devices_inactive_except(ctx.store, ctx.account_id, own.as_str(), &keep)
    {
        ctx.store_error("mark an OMEMO device inactive", e);
    }
    if let Err(e) = store::delete_session(ctx.store, ctx.account_id, own.as_str(), device) {
        ctx.store_error("drop an OMEMO session", e);
    }
    ctx.emit(ClientEvent::OmemoDevicesChanged(own));
}
