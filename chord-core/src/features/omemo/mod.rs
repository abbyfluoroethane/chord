//! OMEMO 2 end-to-end encryption (XEP-0384) for 1:1 chats. See `docs/omemo-plan.md`.
//!
//! Layout:
//! - `xml`: the stanza elements (device list, bundle, `<encrypted>`) and the XEP-0420
//!   envelope.
//! - `store`: the tables of keys, devices, sessions, and trust.
//! - `crypto`: the only code that calls `chord-omemo`. Chord writes no crypto. A build
//!   without `native-session` (wasm) uses `crypto_off` and has no OMEMO.
//! - `device`: our own device: keys, device list, bundle, prekeys, and the device lists of
//!   contacts.
//! - `trust`: trust decisions (blind trust before verification) and the device views.
//! - `jobs`: the fetch of device lists and bundles that a send or a refresh waits for.
//! - `send`: build, encrypt, and send a message.
//! - `receive`: find, decrypt, and check an encrypted message, before `chat` stores it.
//!
//! What is secret: when OMEMO is on for a chat, everything that a message says goes into
//! the encrypted XEP-0420 envelope: the body, corrections, replies, reactions,
//! retractions, links, and file data. Only a short list stays outside (see
//! `send::is_outside`): the ids, the XEP-0334 hints, chat states, markers, and receipts.
//! A message that has nothing secret (a marker) goes out as it is. A message that has
//! secret parts never goes out as plain text: if it cannot be encrypted it fails.
//!
//! The identity blob in `omemo_identity` holds the whole `chord_omemo::Identity`: the
//! identity key, the signed prekeys, and the one-time prekeys. The tables `omemo_prekeys`
//! and `omemo_signed_prekeys` stay empty.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_channel::oneshot;
use jid::BareJid;

use super::{Ctx, FeatureCommand, IqResponse};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::secrets::SecretStore;
use crate::store::Store;
use crate::views::ViewKey;

#[cfg(feature = "native-session")]
mod crypto;
#[cfg(not(feature = "native-session"))]
#[path = "crypto_off.rs"]
mod crypto;
mod device;
mod jobs;
pub(crate) mod receive;
pub(crate) mod send;
pub(crate) mod store;
mod trust;
pub(crate) mod xml;

pub(crate) use store::{ENCRYPTION_FAILED, ENCRYPTION_OMEMO};

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// How much we trust a key of a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum OmemoTrust {
    /// The user has not decided. The device gets no message keys.
    Undecided,
    /// Trusted at once, because the user verified no key of this contact yet.
    Blind,
    /// The user compared the fingerprint.
    Verified,
    /// The user does not trust the key. The device gets no message keys.
    Distrusted,
}

impl From<store::TrustLevel> for OmemoTrust {
    fn from(level: store::TrustLevel) -> Self {
        match level {
            store::TrustLevel::Undecided => Self::Undecided,
            store::TrustLevel::Blind => Self::Blind,
            store::TrustLevel::Verified => Self::Verified,
            store::TrustLevel::Distrusted => Self::Distrusted,
        }
    }
}

impl From<OmemoTrust> for store::TrustLevel {
    fn from(trust: OmemoTrust) -> Self {
        match trust {
            OmemoTrust::Undecided => Self::Undecided,
            OmemoTrust::Blind => Self::Blind,
            OmemoTrust::Verified => Self::Verified,
            OmemoTrust::Distrusted => Self::Distrusted,
        }
    }
}

/// One OMEMO device of a contact or of our own account.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct OmemoDevice {
    /// The bare JID of the account that owns the device.
    pub jid: String,
    pub device_id: u32,
    /// The fingerprint to compare: 8 groups of 8 hex characters. `None` until we have seen
    /// the key of the device.
    pub fingerprint: Option<String>,
    pub label: Option<String>,
    /// `None` until we have seen the key of the device.
    pub trust: Option<OmemoTrust>,
    /// False when the device is not in the last device list.
    pub active: bool,
    /// True when the device belongs to our own account.
    pub own: bool,
    /// True for the device that this client runs on.
    pub this_device: bool,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The device list of an account.
    List {
        jid: BareJid,
    },
    /// The bundle of one device.
    Bundle {
        jid: BareJid,
        device: u32,
    },
    /// Our own bundle, to compare it with the local keys.
    OwnBundle,
    PublishList,
    PublishBundle,
    /// The device list of our account, read to remove one device.
    RemoveFetch {
        device: u32,
        reply: Reply<()>,
    },
    RemovePublish {
        device: u32,
        reply: Reply<()>,
    },
}

/// A command from the public API.
pub(crate) enum Command {
    Enabled {
        peer: BareJid,
        reply: Reply<bool>,
    },
    SetEnabled {
        peer: BareJid,
        enabled: bool,
        reply: Reply<()>,
    },
    Devices {
        jid: BareJid,
        reply: Reply<Vec<OmemoDevice>>,
    },
    OwnDevices {
        reply: Reply<Vec<OmemoDevice>>,
    },
    Refresh {
        jid: BareJid,
        reply: Reply<()>,
    },
    SetTrust {
        jid: BareJid,
        fingerprint: String,
        trust: OmemoTrust,
        reply: Reply<()>,
    },
    RemoveOwnDevice {
        device_id: u32,
        reply: Reply<()>,
    },
}

impl ClientHandle {
    /// True if OMEMO is on for the chat with `peer`. Off by default. Works offline.
    pub async fn omemo_enabled(&self, peer: BareJid) -> Result<bool, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::Enabled { peer, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Turn OMEMO on or off for the chat with `peer`. While it is on, Chord sends no plain
    /// text to the peer: a message that Chord cannot encrypt fails. Works offline. Fails
    /// with `Unsupported` in a build that has no OMEMO.
    pub async fn set_omemo_enabled(&self, peer: BareJid, enabled: bool) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::SetEnabled {
            peer,
            enabled,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The devices that we know for `jid`, with fingerprint, label, trust, and state. Works
    /// offline, from the store. Call `refresh_omemo_devices` first for fresh data.
    pub async fn omemo_devices(&self, jid: BareJid) -> Result<Vec<OmemoDevice>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::Devices { jid, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The devices of our own account. The one with `this_device` is this client.
    pub async fn own_omemo_devices(&self) -> Result<Vec<OmemoDevice>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::OwnDevices { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Fetch the device list and the bundles of `jid` from the server. The answer comes
    /// when all answers are in. A device that gets a new key shows up as
    /// `ClientEvent::OmemoTrustNeeded` or `OmemoDevicesChanged`.
    pub async fn refresh_omemo_devices(&self, jid: BareJid) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::Refresh { jid, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Set the trust in a key of `jid`. `fingerprint` is the text of
    /// `OmemoDevice::fingerprint` (spaces and case do not matter). Works offline.
    /// After the user verifies any key of a contact, new keys of that contact are
    /// `Undecided` until the user decides.
    pub async fn set_omemo_trust(
        &self,
        jid: BareJid,
        fingerprint: String,
        trust: OmemoTrust,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::SetTrust {
            jid,
            fingerprint,
            trust,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Remove one of our other devices from our device list. This device cannot remove
    /// itself.
    pub async fn remove_own_omemo_device(&self, device_id: u32) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Omemo(Command::RemoveOwnDevice {
            device_id,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// Our keys, once they are loaded.
pub(crate) struct Engine {
    pub sealer: store::Sealer,
    pub keys: crypto::Keys,
}

/// The state of OMEMO in one session.
#[derive(Default)]
pub(crate) struct State {
    /// The system keychain, if the app gave one. It stays across sessions.
    secrets: Option<Arc<dyn SecretStore>>,
    /// `None` until the keys are loaded, and when loading failed.
    engine: Option<Engine>,
    /// Why there is no engine, for the error text of a send.
    problem: Option<String>,
    /// When we read the device list of an account in this session, in Unix ms.
    listed: HashMap<BareJid, i64>,
    /// The requests that wait for an answer.
    in_flight: HashSet<jobs::Wait>,
    /// The sends and refreshes that wait for answers.
    jobs: Vec<jobs::Job>,
    next_job: u64,
    /// Bundles that we fetched and did not use yet.
    bundles: HashMap<(BareJid, u32), xml::BundleXml>,
    /// Session ticks since the last key maintenance.
    ticks: u32,
    /// We made new keys at this connect, so the server has no bundle of ours.
    fresh: bool,
}

impl State {
    /// Keep the OMEMO keys in the sealed store with `secrets` as the data key place.
    pub(crate) fn set_secret_store(&mut self, secrets: Arc<dyn SecretStore>) {
        self.secrets = Some(secrets);
    }
}

/// The state for a new session. It keeps the secret store. The keys load again.
pub(crate) fn next_session(ctx: &mut Ctx<'_>) -> State {
    jobs::fail_all(ctx, "the connection was lost");
    State {
        secrets: ctx.state.omemo.secrets.take(),
        ..State::default()
    }
}

/// A new session is up: load or make our keys, and start the device list.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    device::init(ctx);
}

/// Service discovery is done. Read our own device list and publish what is missing.
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    device::start(ctx);
}

/// A session tick: key maintenance once an hour.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    // A tick is 15 s. 240 ticks are one hour.
    ctx.state.omemo.ticks += 1;
    if ctx.state.omemo.ticks >= 240 {
        ctx.state.omemo.ticks = 0;
        device::maintain(ctx);
    }
}

/// True if OMEMO is on for the 1:1 chat with `peer`.
pub(crate) fn is_on(ctx: &Ctx<'_>, peer: &BareJid) -> bool {
    store::conversation_enabled(ctx.store, ctx.account_id, peer.as_str()).unwrap_or(false)
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::List { jid } => jobs::on_list(ctx, jid, response),
        Pending::Bundle { jid, device } => jobs::on_bundle(ctx, jid, device, response),
        Pending::OwnBundle => device::on_own_bundle(ctx, response),
        Pending::PublishList => device::on_published(ctx, "device list", response),
        Pending::PublishBundle => device::on_published(ctx, "bundle", response),
        Pending::RemoveFetch { device, reply } => {
            device::on_remove_fetch(ctx, device, reply, response)
        }
        Pending::RemovePublish { device, reply } => {
            device::on_remove_publish(ctx, device, reply, response)
        }
    }
}

/// A PEP event on the device list node. `from` is the account that owns the node.
pub(crate) fn on_devices_event(
    ctx: &mut Ctx<'_>,
    from: &BareJid,
    payload: xmpp_parsers::pubsub::event::Payload,
) {
    device::on_devices_event(ctx, from, payload);
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Enabled { peer, reply } => {
            let _ = reply.send(enabled(ctx.store, ctx.account_id, &peer));
        }
        Command::SetEnabled {
            peer,
            enabled,
            reply,
        } => {
            let result = set_enabled(ctx.store, ctx.account_id, &peer, enabled);
            if result.is_ok() && enabled {
                // Warm up: read the devices of the peer now, so that the first send is quick.
                jobs::refresh(ctx, peer, None);
            }
            let _ = reply.send(result);
        }
        Command::Devices { jid, reply } => {
            let _ = reply.send(devices(ctx.store, ctx.account_id, ctx.account, &jid));
        }
        Command::OwnDevices { reply } => {
            let own = ctx.account.clone();
            let _ = reply.send(devices(ctx.store, ctx.account_id, ctx.account, &own));
        }
        Command::Refresh { jid, reply } => jobs::refresh(ctx, jid, Some(reply)),
        Command::SetTrust {
            jid,
            fingerprint,
            trust,
            reply,
        } => {
            let result = set_trust(ctx.store, ctx.account_id, &jid, &fingerprint, trust);
            if result.is_ok() {
                ctx.emit(ClientEvent::OmemoDevicesChanged(jid));
            }
            let _ = reply.send(result);
        }
        Command::RemoveOwnDevice { device_id, reply } => device::remove_own(ctx, device_id, reply),
    }
}

/// A command while no session is up. What the store can answer, it answers. The rest need
/// the server.
pub(crate) fn offline(store: &Store, account_id: i64, command: Command) {
    // The account of the actor, from the store: this function has no `Ctx`.
    let account: BareJid = store
        .conn()
        .query_row(
            "SELECT jid FROM accounts WHERE id = ?1",
            [account_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|jid| jid.parse().ok())
        .unwrap_or_else(|| BareJid::new("nobody.invalid").expect("a valid JID"));
    let account = &account;
    match command {
        Command::Enabled { peer, reply } => {
            let _ = reply.send(enabled(store, account_id, &peer));
        }
        Command::SetEnabled {
            peer,
            enabled,
            reply,
        } => {
            let _ = reply.send(set_enabled(store, account_id, &peer, enabled));
        }
        Command::Devices { jid, reply } => {
            let _ = reply.send(devices(store, account_id, account, &jid));
        }
        Command::OwnDevices { reply } => {
            let _ = reply.send(devices(store, account_id, account, account));
        }
        Command::SetTrust {
            jid,
            fingerprint,
            trust,
            reply,
        } => {
            let _ = reply.send(set_trust(store, account_id, &jid, &fingerprint, trust));
        }
        Command::Refresh { reply, .. } | Command::RemoveOwnDevice { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

fn store_error(e: impl std::fmt::Display) -> ClientError {
    ClientError::Invalid(format!("store: {e}"))
}

fn enabled(store: &Store, account_id: i64, peer: &BareJid) -> Result<bool, ClientError> {
    store::conversation_enabled(store, account_id, peer.as_str()).map_err(store_error)
}

fn set_enabled(
    store: &Store,
    account_id: i64,
    peer: &BareJid,
    enabled: bool,
) -> Result<(), ClientError> {
    if enabled && !crypto::AVAILABLE {
        return Err(ClientError::Unsupported("this build has no OMEMO".into()));
    }
    store::set_conversation_enabled(store, account_id, peer.as_str(), enabled).map_err(store_error)
}

fn devices(
    store: &Store,
    account_id: i64,
    account: &BareJid,
    jid: &BareJid,
) -> Result<Vec<OmemoDevice>, ClientError> {
    trust::devices(store, account_id, account, jid).map_err(store_error)
}

fn set_trust(
    store: &Store,
    account_id: i64,
    jid: &BareJid,
    fingerprint: &str,
    trust: OmemoTrust,
) -> Result<(), ClientError> {
    trust::set_trust(store, account_id, jid, fingerprint, trust.into())
}

/// Unix time in ms from SQLite's clock, because `SystemTime::now` panics in the wasm build.
pub(super) fn now_ms(store: &Store) -> i64 {
    store
        .conn()
        .query_row(
            "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
}

/// The current time as an RFC 3339 text in UTC, from SQLite's clock.
pub(super) fn now_rfc3339(store: &Store) -> String {
    store
        .conn()
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
            row.get(0)
        })
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

/// Random bytes for the padding of an envelope. The padding is no secret: it only hides the
/// length of a message.
pub(super) fn random_bytes<const N: usize>() -> [u8; N] {
    use aes_gcm::aead::{OsRng, rand_core::RngCore};
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

/// Mark the timeline of a chat as changed.
pub(super) fn timeline_changed(ctx: &mut Ctx<'_>, peer: &BareJid) {
    ctx.changed(ViewKey::Timeline(peer.clone()));
    ctx.changed(ViewKey::ChannelList(crate::views::ChannelScope::Home));
}

#[cfg(test)]
mod tests;
