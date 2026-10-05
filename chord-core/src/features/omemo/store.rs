//! The tables of OMEMO: keys, devices, sessions, and trust. This file does no protocol
//! cryptography. The crypto library owns the content of the key, prekey, and session
//! blobs. Here they are plain bytes.
//!
//! The secrets are sealed: AES-256-GCM with a 32-byte data key of the account. Each
//! sealed blob is a 12-byte random nonce, then the ciphertext and the tag. The
//! associated data names the table and the row, so a blob that moves to another row does
//! not open. Public data (device identity keys, fingerprints) is not sealed.

// The identity blob holds all keys, so some functions for single prekeys stay unused.
#![allow(dead_code)]

use core::fmt;

use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload, rand_core::RngCore};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rusqlite::{OptionalExtension, params};
use zeroize::Zeroizing;

use crate::secrets::{SecretStore, omemo_data_key};
use crate::store::Store;

/// `messages.encryption` of an OMEMO message that we read or sent.
pub(crate) const ENCRYPTION_OMEMO: &str = "omemo:2";
/// `messages.encryption` of an OMEMO message that we could not decrypt.
pub(crate) const ENCRYPTION_FAILED: &str = "omemo:2:failed";

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// Unix time in ms, from SQLite's clock (the same as `store::queries`).
const NOW_MS: &str = "CAST(unixepoch('subsec') * 1000 AS INTEGER)";

/// An error of the OMEMO store.
#[derive(Debug)]
pub(crate) enum Error {
    Sqlite(rusqlite::Error),
    /// The database has sealed rows, but the data key is gone (the keychain was wiped).
    /// The caller must call `reset` and start a new device.
    KeysLost,
    /// The secret store failed.
    Secrets(String),
    /// A sealed blob does not open: wrong key, wrong row, or changed bytes.
    Open,
    /// The data key in the secret store or the database is not valid.
    BadKey,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "database error: {e}"),
            Self::KeysLost => write!(
                f,
                "the OMEMO keys of this account are lost: the data key is gone. Start a new device."
            ),
            Self::Secrets(e) => write!(f, "secret store error: {e}"),
            Self::Open => write!(f, "an OMEMO secret in the database does not open"),
            Self::BadKey => write!(f, "the OMEMO data key is not valid"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

type Result<T> = core::result::Result<T, Error>;

// ---------------------------------------------------------------------------------------
// Sealing

/// Seals and opens the secrets of one account.
pub(crate) struct Sealer {
    key: Zeroizing<[u8; KEY_LEN]>,
}

impl fmt::Debug for Sealer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sealer(..)")
    }
}

impl Sealer {
    /// Load the data key of the account, or make it.
    ///
    /// With a secret store (the system keychain), the key lives there under
    /// `secrets::omemo_data_key(account_jid)`, as hex text.
    ///
    /// With no secret store (CLI, tests, Android for now), the key lives in the table
    /// `omemo_data_key`, next to the data that it seals. This is weaker: a copy of the
    /// database file has the key too. It only keeps the secrets out of a plain dump.
    /// A key in that table moves to the secret store when a store becomes available.
    ///
    /// If the key is missing but sealed rows exist, the result is `Error::KeysLost`.
    pub(crate) fn open_or_create(
        store: &Store,
        account_id: i64,
        account_jid: &str,
        secrets: Option<&dyn SecretStore>,
    ) -> Result<Self> {
        let conn = store.conn();
        let in_table: Option<Vec<u8>> = conn
            .query_row(
                "SELECT key FROM omemo_data_key WHERE account_id = ?1",
                params![account_id],
                |r| r.get(0),
            )
            .optional()?;
        let in_table = in_table.map(Zeroizing::new);
        let in_table = in_table.map(|k| to_key(&k)).transpose()?;
        let Some(secrets) = secrets else {
            if let Some(key) = in_table {
                return Ok(Self { key });
            }
            if has_sealed_rows(store, account_id)? {
                return Err(Error::KeysLost);
            }
            let key = new_key();
            conn.execute(
                "INSERT INTO omemo_data_key (account_id, key) VALUES (?1, ?2)",
                params![account_id, &key[..]],
            )?;
            return Ok(Self { key });
        };
        let name = omemo_data_key(account_jid);
        let stored = secrets.get(&name).map_err(Error::Secrets)?;
        if let Some(text) = stored {
            let text = Zeroizing::new(text);
            let bytes = Zeroizing::new(from_hex(&text).ok_or(Error::BadKey)?);
            return Ok(Self {
                key: to_key(&bytes)?,
            });
        }
        // The store has no key. Take the one of the table, or make one.
        let key = match in_table {
            Some(key) => key,
            None if has_sealed_rows(store, account_id)? => return Err(Error::KeysLost),
            None => new_key(),
        };
        secrets
            .set(&name, &to_hex(&key[..]))
            .map_err(Error::Secrets)?;
        // The key is safe in the store. Drop the copy in the database.
        conn.execute(
            "DELETE FROM omemo_data_key WHERE account_id = ?1",
            params![account_id],
        )?;
        Ok(Self { key })
    }

    /// Seal `plain`. `ad` binds the blob to its row, for example `"prekey|1|42"`.
    pub(crate) fn seal(&self, ad: &str, plain: &[u8]) -> Vec<u8> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key[..]));
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let sealed = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: ad.as_bytes(),
                },
            )
            // AES-GCM fails here only for a message of more than 64 GB.
            .expect("the plain text is too long to seal");
        let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        out
    }

    /// Open a blob that `seal` made with the same `ad`.
    pub(crate) fn open(&self, ad: &str, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if sealed.len() < NONCE_LEN {
            return Err(Error::Open);
        }
        let (nonce, body) = sealed.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.key[..]));
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: body,
                    aad: ad.as_bytes(),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| Error::Open)
    }
}

fn new_key() -> Zeroizing<[u8; KEY_LEN]> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    OsRng.fill_bytes(&mut key[..]);
    key
}

fn to_key(bytes: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    if bytes.len() != KEY_LEN {
        return Err(Error::BadKey);
    }
    key.copy_from_slice(bytes);
    Ok(key)
}

fn to_hex(bytes: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.is_ascii() || !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok())
        .collect()
}

/// True if a table holds a sealed row of the account.
fn has_sealed_rows(store: &Store, account_id: i64) -> Result<bool> {
    Ok(store.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM omemo_identity WHERE account_id = ?1)
             OR EXISTS (SELECT 1 FROM omemo_signed_prekeys WHERE account_id = ?1)
             OR EXISTS (SELECT 1 FROM omemo_prekeys WHERE account_id = ?1)
             OR EXISTS (SELECT 1 FROM omemo_sessions WHERE account_id = ?1)",
        params![account_id],
        |r| r.get(0),
    )?)
}

/// Delete every OMEMO row of the account, also the data key in the database. The caller
/// starts a new device after this. It does not touch the secret store.
pub(crate) fn reset(store: &Store, account_id: i64) -> Result<()> {
    let tx = store.conn().unchecked_transaction()?;
    for table in [
        "omemo_identity",
        "omemo_signed_prekeys",
        "omemo_prekeys",
        "omemo_devices",
        "omemo_sessions",
        "omemo_trust",
        "omemo_conversations",
        "omemo_data_key",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE account_id = ?1"),
            params![account_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// The associated data of each sealed column: the table and the row key.
fn ad_identity(account_id: i64) -> String {
    format!("omemo_identity|{account_id}")
}
fn ad_signed_prekey(account_id: i64, id: u32) -> String {
    format!("omemo_signed_prekeys|{account_id}|{id}")
}
fn ad_prekey(account_id: i64, id: u32) -> String {
    format!("omemo_prekeys|{account_id}|{id}")
}
fn ad_session(account_id: i64, jid: &str, device_id: u32) -> String {
    format!("omemo_sessions|{account_id}|{jid}|{device_id}")
}

// ---------------------------------------------------------------------------------------
// Own identity

/// Our own device: its id and the sealed identity key pair.
pub(crate) fn load_identity(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
) -> Result<Option<(u32, Zeroizing<Vec<u8>>)>> {
    let row: Option<(u32, Vec<u8>)> = store
        .conn()
        .query_row(
            "SELECT device_id, identity FROM omemo_identity WHERE account_id = ?1",
            params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(id, sealed)| Ok((id, sealer.open(&ad_identity(account_id), &sealed)?)))
        .transpose()
}

/// Save our own device. It replaces the old one.
pub(crate) fn save_identity(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
    device_id: u32,
    identity: &[u8],
) -> Result<()> {
    store.conn().execute(
        &format!(
            "INSERT INTO omemo_identity (account_id, device_id, identity, created_at)
             VALUES (?1, ?2, ?3, {NOW_MS})
             ON CONFLICT (account_id) DO UPDATE SET
                 device_id = excluded.device_id, identity = excluded.identity"
        ),
        params![
            account_id,
            device_id,
            sealer.seal(&ad_identity(account_id), identity)
        ],
    )?;
    Ok(())
}

/// The device id of our own device, without the secrets. `None` before the first start.
pub(crate) fn own_device_id(store: &Store, account_id: i64) -> Result<Option<u32>> {
    Ok(store
        .conn()
        .query_row(
            "SELECT device_id FROM omemo_identity WHERE account_id = ?1",
            params![account_id],
            |r| r.get(0),
        )
        .optional()?)
}

// ---------------------------------------------------------------------------------------
// Signed prekeys

/// A signed prekey of ours.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SignedPrekey {
    pub id: u32,
    pub record: Zeroizing<Vec<u8>>,
    pub created_at: i64,
    /// When a newer key replaced this one. `None` for the current key.
    pub replaced_at: Option<i64>,
}

/// All signed prekeys, the newest first.
pub(crate) fn list_signed_prekeys(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
) -> Result<Vec<SignedPrekey>> {
    let mut stmt = store.conn().prepare_cached(
        "SELECT id, record, created_at, replaced_at FROM omemo_signed_prekeys
         WHERE account_id = ?1 ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map(params![account_id], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get(2)?,
                r.get(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, sealed, created_at, replaced_at)| {
            Ok(SignedPrekey {
                id,
                record: sealer.open(&ad_signed_prekey(account_id, id), &sealed)?,
                created_at,
                replaced_at,
            })
        })
        .collect()
}

/// Save a new signed prekey. It replaces a key with the same id.
pub(crate) fn save_signed_prekey(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
    id: u32,
    record: &[u8],
) -> Result<()> {
    store.conn().execute(
        &format!(
            "INSERT OR REPLACE INTO omemo_signed_prekeys
                 (account_id, id, record, created_at, replaced_at)
             VALUES (?1, ?2, ?3, {NOW_MS}, NULL)"
        ),
        params![
            account_id,
            id,
            sealer.seal(&ad_signed_prekey(account_id, id), record)
        ],
    )?;
    Ok(())
}

/// Mark every signed prekey except `current_id` as replaced, if it is not marked yet.
/// Returns how many keys changed.
pub(crate) fn mark_signed_prekeys_replaced(
    store: &Store,
    account_id: i64,
    current_id: u32,
) -> Result<usize> {
    Ok(store.conn().execute(
        &format!(
            "UPDATE omemo_signed_prekeys SET replaced_at = {NOW_MS}
             WHERE account_id = ?1 AND id != ?2 AND replaced_at IS NULL"
        ),
        params![account_id, current_id],
    )?)
}

/// Delete the signed prekeys that a newer key replaced before `cutoff` (Unix ms). A peer
/// can still use an old key for a short time, so the caller picks a delay. Returns how
/// many keys went.
pub(crate) fn delete_signed_prekeys_replaced_before(
    store: &Store,
    account_id: i64,
    cutoff: i64,
) -> Result<usize> {
    Ok(store.conn().execute(
        "DELETE FROM omemo_signed_prekeys
         WHERE account_id = ?1 AND replaced_at IS NOT NULL AND replaced_at < ?2",
        params![account_id, cutoff],
    )?)
}

// ---------------------------------------------------------------------------------------
// One-time prekeys. A prekey that a peer used is deleted, so there is no "consumed" state.

pub(crate) fn load_prekey(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
    id: u32,
) -> Result<Option<Zeroizing<Vec<u8>>>> {
    let sealed: Option<Vec<u8>> = store
        .conn()
        .query_row(
            "SELECT record FROM omemo_prekeys WHERE account_id = ?1 AND id = ?2",
            params![account_id, id],
            |r| r.get(0),
        )
        .optional()?;
    sealed
        .map(|s| sealer.open(&ad_prekey(account_id, id), &s))
        .transpose()
}

/// Save many prekeys in one transaction. A key with a known id is replaced.
pub(crate) fn save_prekeys(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
    prekeys: &[(u32, &[u8])],
) -> Result<()> {
    let tx = store.conn().unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO omemo_prekeys (account_id, id, record) VALUES (?1, ?2, ?3)",
        )?;
        for (id, record) in prekeys {
            stmt.execute(params![
                account_id,
                id,
                sealer.seal(&ad_prekey(account_id, *id), record)
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Delete a prekey. Returns false if there was none.
pub(crate) fn delete_prekey(store: &Store, account_id: i64, id: u32) -> Result<bool> {
    Ok(store.conn().execute(
        "DELETE FROM omemo_prekeys WHERE account_id = ?1 AND id = ?2",
        params![account_id, id],
    )? > 0)
}

pub(crate) fn count_prekeys(store: &Store, account_id: i64) -> Result<u32> {
    Ok(store.conn().query_row(
        "SELECT count(*) FROM omemo_prekeys WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?)
}

/// The highest prekey id, so that the next ids do not repeat.
pub(crate) fn max_prekey_id(store: &Store, account_id: i64) -> Result<Option<u32>> {
    Ok(store.conn().query_row(
        "SELECT max(id) FROM omemo_prekeys WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------------------
// Devices of contacts and of our own account

/// A device that we know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeviceRow {
    pub jid: String,
    pub device_id: u32,
    /// The public identity key, once we have seen the bundle.
    pub identity_key: Option<Vec<u8>>,
    pub label: Option<String>,
    /// False when the device is not in the last device list.
    pub active: bool,
    pub first_seen: i64,
    pub last_seen: i64,
}

fn device_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DeviceRow> {
    Ok(DeviceRow {
        jid: r.get(0)?,
        device_id: r.get(1)?,
        identity_key: r.get(2)?,
        label: r.get(3)?,
        active: r.get(4)?,
        first_seen: r.get(5)?,
        last_seen: r.get(6)?,
    })
}

const DEVICE_COLUMNS: &str = "jid, device_id, identity_key, label, active, first_seen, last_seen";

/// Add a device, or update it: it becomes active, and `last_seen` changes. A `None` key or
/// label keeps the old value.
pub(crate) fn upsert_device(
    store: &Store,
    account_id: i64,
    jid: &str,
    device_id: u32,
    identity_key: Option<&[u8]>,
    label: Option<&str>,
) -> Result<()> {
    store.conn().execute(
        &format!(
            "INSERT INTO omemo_devices
                 (account_id, jid, device_id, identity_key, label, active, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, {NOW_MS}, {NOW_MS})
             ON CONFLICT (account_id, jid, device_id) DO UPDATE SET
                 identity_key = COALESCE(excluded.identity_key, identity_key),
                 label = COALESCE(excluded.label, label),
                 active = 1,
                 last_seen = excluded.last_seen"
        ),
        params![account_id, jid, device_id, identity_key, label],
    )?;
    Ok(())
}

/// The devices of `jid`, active and inactive, by device id.
pub(crate) fn list_devices(store: &Store, account_id: i64, jid: &str) -> Result<Vec<DeviceRow>> {
    let mut stmt = store.conn().prepare_cached(&format!(
        "SELECT {DEVICE_COLUMNS} FROM omemo_devices
         WHERE account_id = ?1 AND jid = ?2 ORDER BY device_id"
    ))?;
    let rows = stmt.query_map(params![account_id, jid], device_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Set `active = 0` for the devices of `jid` that are not in `active_ids`, the new device
/// list. Returns how many devices changed.
pub(crate) fn mark_devices_inactive_except(
    store: &Store,
    account_id: i64,
    jid: &str,
    active_ids: &[u32],
) -> Result<usize> {
    let tx = store.conn().unchecked_transaction()?;
    let mut changed = 0;
    {
        let mut stmt = tx.prepare_cached(
            "SELECT device_id FROM omemo_devices
             WHERE account_id = ?1 AND jid = ?2 AND active = 1",
        )?;
        let current = stmt
            .query_map(params![account_id, jid], |r| r.get::<_, u32>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut update = tx.prepare_cached(
            "UPDATE omemo_devices SET active = 0
             WHERE account_id = ?1 AND jid = ?2 AND device_id = ?3",
        )?;
        for id in current.into_iter().filter(|id| !active_ids.contains(id)) {
            changed += update.execute(params![account_id, jid, id])?;
        }
    }
    tx.commit()?;
    Ok(changed)
}

/// The active devices of our own account, except this one.
pub(crate) fn list_own_other_devices(
    store: &Store,
    account_id: i64,
    own_jid: &str,
    own_device_id: u32,
) -> Result<Vec<DeviceRow>> {
    let mut stmt = store.conn().prepare_cached(&format!(
        "SELECT {DEVICE_COLUMNS} FROM omemo_devices
         WHERE account_id = ?1 AND jid = ?2 AND device_id != ?3 AND active = 1
         ORDER BY device_id"
    ))?;
    let rows = stmt.query_map(params![account_id, own_jid, own_device_id], device_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---------------------------------------------------------------------------------------
// Sessions

pub(crate) fn load_session(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
    jid: &str,
    device_id: u32,
) -> Result<Option<Zeroizing<Vec<u8>>>> {
    let sealed: Option<Vec<u8>> = store
        .conn()
        .query_row(
            "SELECT state FROM omemo_sessions
             WHERE account_id = ?1 AND jid = ?2 AND device_id = ?3",
            params![account_id, jid, device_id],
            |r| r.get(0),
        )
        .optional()?;
    sealed
        .map(|s| sealer.open(&ad_session(account_id, jid, device_id), &s))
        .transpose()
}

pub(crate) fn save_session(
    store: &Store,
    sealer: &Sealer,
    account_id: i64,
    jid: &str,
    device_id: u32,
    state: &[u8],
) -> Result<()> {
    store.conn().execute(
        &format!(
            "INSERT INTO omemo_sessions (account_id, jid, device_id, state, updated_at)
             VALUES (?1, ?2, ?3, ?4, {NOW_MS})
             ON CONFLICT (account_id, jid, device_id) DO UPDATE SET
                 state = excluded.state, updated_at = excluded.updated_at"
        ),
        params![
            account_id,
            jid,
            device_id,
            sealer.seal(&ad_session(account_id, jid, device_id), state)
        ],
    )?;
    Ok(())
}

/// Delete a session. Returns false if there was none.
pub(crate) fn delete_session(
    store: &Store,
    account_id: i64,
    jid: &str,
    device_id: u32,
) -> Result<bool> {
    Ok(store.conn().execute(
        "DELETE FROM omemo_sessions WHERE account_id = ?1 AND jid = ?2 AND device_id = ?3",
        params![account_id, jid, device_id],
    )? > 0)
}

// ---------------------------------------------------------------------------------------
// Trust

/// How much we trust one identity key of a contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrustLevel {
    Undecided,
    /// Trusted at once, because the user has verified no key of this contact yet.
    Blind,
    Verified,
    Distrusted,
}

impl TrustLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Undecided => "undecided",
            Self::Blind => "blind",
            Self::Verified => "verified",
            Self::Distrusted => "distrusted",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "undecided" => Self::Undecided,
            "blind" => Self::Blind,
            "verified" => Self::Verified,
            "distrusted" => Self::Distrusted,
            _ => return None,
        })
    }
}

/// The trust in one key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrustEntry {
    /// The public identity key bytes.
    pub fingerprint: Vec<u8>,
    pub level: TrustLevel,
    pub decided_at: i64,
}

fn trust_level(text: String) -> rusqlite::Result<TrustLevel> {
    TrustLevel::parse(&text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown trust level {text:?}").into(),
        )
    })
}

pub(crate) fn get_trust(
    store: &Store,
    account_id: i64,
    jid: &str,
    fingerprint: &[u8],
) -> Result<Option<TrustLevel>> {
    let level: Option<String> = store
        .conn()
        .query_row(
            "SELECT level FROM omemo_trust
             WHERE account_id = ?1 AND jid = ?2 AND fingerprint = ?3",
            params![account_id, jid, fingerprint],
            |r| r.get(0),
        )
        .optional()?;
    Ok(level.map(trust_level).transpose()?)
}

pub(crate) fn set_trust(
    store: &Store,
    account_id: i64,
    jid: &str,
    fingerprint: &[u8],
    level: TrustLevel,
) -> Result<()> {
    store.conn().execute(
        &format!(
            "INSERT INTO omemo_trust (account_id, jid, fingerprint, level, decided_at)
             VALUES (?1, ?2, ?3, ?4, {NOW_MS})
             ON CONFLICT (account_id, jid, fingerprint) DO UPDATE SET
                 level = excluded.level, decided_at = excluded.decided_at"
        ),
        params![account_id, jid, fingerprint, level.as_str()],
    )?;
    Ok(())
}

/// The trust in all keys of `jid`.
pub(crate) fn list_trust(store: &Store, account_id: i64, jid: &str) -> Result<Vec<TrustEntry>> {
    let mut stmt = store.conn().prepare_cached(
        "SELECT fingerprint, level, decided_at FROM omemo_trust
         WHERE account_id = ?1 AND jid = ?2 ORDER BY decided_at, fingerprint",
    )?;
    let rows = stmt.query_map(params![account_id, jid], |r| {
        Ok(TrustEntry {
            fingerprint: r.get(0)?,
            level: trust_level(r.get(1)?)?,
            decided_at: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---------------------------------------------------------------------------------------
// The switch of a conversation

/// True if OMEMO is on for the conversation. A conversation with no row is off.
pub(crate) fn conversation_enabled(store: &Store, account_id: i64, jid: &str) -> Result<bool> {
    let on: Option<bool> = store
        .conn()
        .query_row(
            "SELECT enabled FROM omemo_conversations WHERE account_id = ?1 AND jid = ?2",
            params![account_id, jid],
            |r| r.get(0),
        )
        .optional()?;
    Ok(on.unwrap_or(false))
}

pub(crate) fn set_conversation_enabled(
    store: &Store,
    account_id: i64,
    jid: &str,
    enabled: bool,
) -> Result<()> {
    store.conn().execute(
        "INSERT INTO omemo_conversations (account_id, jid, enabled) VALUES (?1, ?2, ?3)
         ON CONFLICT (account_id, jid) DO UPDATE SET enabled = excluded.enabled",
        params![account_id, jid, enabled],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::testing::MemorySecrets;
    use crate::store::queries::ensure_account;
    use crate::store::{migrations, schema::MIGRATIONS};

    const ALICE: &str = "alice@chord.localhost";
    const BOB: &str = "bob@chord.localhost";

    fn setup() -> (Store, i64, Sealer) {
        let store = Store::open_in_memory().unwrap();
        let account = ensure_account(store.conn(), ALICE).unwrap();
        let sealer = Sealer::open_or_create(&store, account, ALICE, None).unwrap();
        (store, account, sealer)
    }

    fn count(store: &Store, table: &str, account: i64) -> i64 {
        store
            .conn()
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE account_id = ?1"),
                params![account],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn migration_runs_on_an_empty_database_and_on_the_previous_version() {
        let store = Store::open_in_memory().unwrap();
        let version: i64 = store
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        assert_eq!(count(&store, "omemo_trust", 1), 0);

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        let last = MIGRATIONS.len() - 1;
        for (version, sql) in (1..).zip(&MIGRATIONS[..last]) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        conn.execute(
            "INSERT INTO accounts (id, jid, created_at) VALUES (1, 'a@b.c', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (account_id, key_kind, key, direction, peer, sender, body, timestamp, kind)
             VALUES (1, 'stanza-id', 'k', 'in', 'p@b.c', 'p@b.c', 'hi', 1, 'chat')",
            [],
        )
        .unwrap();
        assert_eq!(
            migrations::migrate(&mut conn).unwrap(),
            MIGRATIONS.len() as i64
        );
        let (body, encryption): (String, Option<String>) = conn
            .query_row("SELECT body, encryption FROM messages", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((body.as_str(), encryption), ("hi", None));
    }

    #[test]
    fn seal_and_open_round_trip() {
        let (_store, _, sealer) = setup();
        let sealed = sealer.seal("row|1", b"secret");
        assert!(!sealed.windows(6).any(|w| w == b"secret"));
        assert_eq!(&sealer.open("row|1", &sealed).unwrap()[..], b"secret");
        // A new nonce for each call.
        assert_ne!(sealed, sealer.seal("row|1", b"secret"));
    }

    #[test]
    fn open_fails_with_wrong_ad_tampered_byte_or_short_blob() {
        let (_store, _, sealer) = setup();
        let sealed = sealer.seal("row|1", b"secret");
        assert!(matches!(sealer.open("row|2", &sealed), Err(Error::Open)));
        for i in 0..sealed.len() {
            let mut bad = sealed.clone();
            bad[i] ^= 1;
            assert!(sealer.open("row|1", &bad).is_err(), "byte {i}");
        }
        assert!(sealer.open("row|1", &sealed[..5]).is_err());
        // Another key does not open it either.
        let other = Sealer { key: new_key() };
        assert!(other.open("row|1", &sealed).is_err());
    }

    #[test]
    fn a_blob_copied_to_another_row_does_not_open() {
        let (store, account, sealer) = setup();
        save_prekeys(&store, &sealer, account, &[(1, b"one"), (2, b"two")]).unwrap();
        store
            .conn()
            .execute(
                "UPDATE omemo_prekeys SET record = (SELECT record FROM omemo_prekeys WHERE id = 1)
                 WHERE id = 2",
                [],
            )
            .unwrap();
        assert!(load_prekey(&store, &sealer, account, 2).is_err());
        assert!(load_prekey(&store, &sealer, account, 1).unwrap().is_some());
    }

    #[test]
    fn the_key_stays_the_same_in_the_database_and_the_secret_store() {
        let (store, account, sealer) = setup();
        let again = Sealer::open_or_create(&store, account, ALICE, None).unwrap();
        assert_eq!(&again.open("a", &sealer.seal("a", b"x")).unwrap()[..], b"x");

        let secrets = MemorySecrets::default();
        let first = Sealer::open_or_create(&store, account, ALICE, Some(&secrets)).unwrap();
        // The table key moved to the secret store, and the table is empty.
        assert_eq!(count(&store, "omemo_data_key", account), 0);
        assert_eq!(&first.open("a", &sealer.seal("a", b"x")).unwrap()[..], b"x");
        let second = Sealer::open_or_create(&store, account, ALICE, Some(&secrets)).unwrap();
        assert_eq!(&second.open("a", &first.seal("a", b"y")).unwrap()[..], b"y");
        let stored = secrets.map.lock().unwrap()[&omemo_data_key(ALICE)].clone();
        assert_eq!(stored.len(), 64);
    }

    #[test]
    fn a_secret_store_that_lost_its_key_gives_keys_lost() {
        let store = Store::open_in_memory().unwrap();
        let account = ensure_account(store.conn(), ALICE).unwrap();
        let secrets = MemorySecrets::default();
        let sealer = Sealer::open_or_create(&store, account, ALICE, Some(&secrets)).unwrap();
        save_identity(&store, &sealer, account, 7, b"ik").unwrap();

        secrets.map.lock().unwrap().clear();
        let lost = Sealer::open_or_create(&store, account, ALICE, Some(&secrets));
        assert!(matches!(lost, Err(Error::KeysLost)));
        assert!(lost.unwrap_err().to_string().contains("Start a new device"));

        reset(&store, account).unwrap();
        let fresh = Sealer::open_or_create(&store, account, ALICE, Some(&secrets)).unwrap();
        assert!(load_identity(&store, &fresh, account).unwrap().is_none());
    }

    #[test]
    fn a_database_key_that_is_gone_gives_keys_lost() {
        let (store, account, sealer) = setup();
        save_identity(&store, &sealer, account, 7, b"ik").unwrap();
        store
            .conn()
            .execute("DELETE FROM omemo_data_key", [])
            .unwrap();
        assert!(matches!(
            Sealer::open_or_create(&store, account, ALICE, None),
            Err(Error::KeysLost)
        ));
    }

    #[test]
    fn a_bad_key_in_the_secret_store_is_an_error() {
        let store = Store::open_in_memory().unwrap();
        let account = ensure_account(store.conn(), ALICE).unwrap();
        let secrets = MemorySecrets::default();
        secrets.set(&omemo_data_key(ALICE), "zz").unwrap();
        assert!(matches!(
            Sealer::open_or_create(&store, account, ALICE, Some(&secrets)),
            Err(Error::BadKey)
        ));
        secrets
            .fail
            .store(true, std::sync::atomic::Ordering::SeqCst);
        secrets.map.lock().unwrap().clear();
        assert!(matches!(
            Sealer::open_or_create(&store, account, ALICE, Some(&secrets)),
            Err(Error::Secrets(_))
        ));
    }

    #[test]
    fn identity_round_trip_and_replace() {
        let (store, account, sealer) = setup();
        assert!(load_identity(&store, &sealer, account).unwrap().is_none());
        save_identity(&store, &sealer, account, 123, b"pair").unwrap();
        let (id, pair) = load_identity(&store, &sealer, account).unwrap().unwrap();
        assert_eq!((id, &pair[..]), (123, &b"pair"[..]));
        // The database holds no plain text.
        let raw: Vec<u8> = store
            .conn()
            .query_row("SELECT identity FROM omemo_identity", [], |r| r.get(0))
            .unwrap();
        assert!(!raw.windows(4).any(|w| w == b"pair"));
        save_identity(&store, &sealer, account, 456, b"pair2").unwrap();
        let (id, pair) = load_identity(&store, &sealer, account).unwrap().unwrap();
        assert_eq!((id, &pair[..]), (456, &b"pair2"[..]));
        assert_eq!(count(&store, "omemo_identity", account), 1);
    }

    #[test]
    fn signed_prekeys_round_trip_replace_and_delete() {
        let (store, account, sealer) = setup();
        save_signed_prekey(&store, &sealer, account, 1, b"spk1").unwrap();
        save_signed_prekey(&store, &sealer, account, 2, b"spk2").unwrap();
        assert_eq!(mark_signed_prekeys_replaced(&store, account, 2).unwrap(), 1);
        // A second call changes nothing: the first mark stays.
        assert_eq!(mark_signed_prekeys_replaced(&store, account, 2).unwrap(), 0);
        let keys = list_signed_prekeys(&store, &sealer, account).unwrap();
        assert_eq!(keys.len(), 2);
        let one = keys.iter().find(|k| k.id == 1).unwrap();
        let two = keys.iter().find(|k| k.id == 2).unwrap();
        assert_eq!(&one.record[..], b"spk1");
        assert!(one.replaced_at.is_some());
        assert_eq!(&two.record[..], b"spk2");
        assert!(two.replaced_at.is_none());

        let replaced = one.replaced_at.unwrap();
        assert_eq!(
            delete_signed_prekeys_replaced_before(&store, account, replaced).unwrap(),
            0
        );
        assert_eq!(
            delete_signed_prekeys_replaced_before(&store, account, replaced + 1).unwrap(),
            1
        );
        let keys = list_signed_prekeys(&store, &sealer, account).unwrap();
        assert_eq!(keys.iter().map(|k| k.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn prekeys_round_trip_count_max_and_delete() {
        let (store, account, sealer) = setup();
        assert_eq!(count_prekeys(&store, account).unwrap(), 0);
        assert_eq!(max_prekey_id(&store, account).unwrap(), None);
        save_prekeys(&store, &sealer, account, &[(5, b"a"), (9, b"b"), (7, b"c")]).unwrap();
        assert_eq!(count_prekeys(&store, account).unwrap(), 3);
        assert_eq!(max_prekey_id(&store, account).unwrap(), Some(9));
        assert_eq!(
            &load_prekey(&store, &sealer, account, 7).unwrap().unwrap()[..],
            b"c"
        );
        assert!(load_prekey(&store, &sealer, account, 8).unwrap().is_none());
        assert!(delete_prekey(&store, account, 9).unwrap());
        assert!(!delete_prekey(&store, account, 9).unwrap());
        assert_eq!(max_prekey_id(&store, account).unwrap(), Some(7));
        assert_eq!(count_prekeys(&store, account).unwrap(), 2);
    }

    #[test]
    fn devices_upsert_list_and_mark_inactive() {
        let (store, account, _) = setup();
        upsert_device(&store, account, BOB, 1, Some(b"k1"), Some("phone")).unwrap();
        upsert_device(&store, account, BOB, 2, None, None).unwrap();
        upsert_device(&store, account, ALICE, 10, Some(b"me"), None).unwrap();
        upsert_device(&store, account, ALICE, 11, Some(b"laptop"), None).unwrap();
        // An update with no key or label keeps them.
        upsert_device(&store, account, BOB, 1, None, None).unwrap();
        let devices = list_devices(&store, account, BOB).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].identity_key.as_deref(), Some(&b"k1"[..]));
        assert_eq!(devices[0].label.as_deref(), Some("phone"));
        assert!(devices[0].active && devices[1].active);
        assert_eq!(devices[1].identity_key, None);
        assert!(devices[0].first_seen <= devices[0].last_seen);

        assert_eq!(
            mark_devices_inactive_except(&store, account, BOB, &[2, 3]).unwrap(),
            1
        );
        let devices = list_devices(&store, account, BOB).unwrap();
        assert_eq!(
            devices
                .iter()
                .map(|d| (d.device_id, d.active))
                .collect::<Vec<_>>(),
            vec![(1, false), (2, true)]
        );
        // The device comes back with the next list.
        upsert_device(&store, account, BOB, 1, None, None).unwrap();
        assert!(list_devices(&store, account, BOB).unwrap()[0].active);

        let others = list_own_other_devices(&store, account, ALICE, 10).unwrap();
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].device_id, 11);
        mark_devices_inactive_except(&store, account, ALICE, &[10]).unwrap();
        assert!(
            list_own_other_devices(&store, account, ALICE, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn sessions_round_trip_replace_and_delete() {
        let (store, account, sealer) = setup();
        assert!(
            load_session(&store, &sealer, account, BOB, 1)
                .unwrap()
                .is_none()
        );
        save_session(&store, &sealer, account, BOB, 1, b"s1").unwrap();
        save_session(&store, &sealer, account, BOB, 2, b"s2").unwrap();
        save_session(&store, &sealer, account, BOB, 1, b"s1b").unwrap();
        assert_eq!(
            &load_session(&store, &sealer, account, BOB, 1)
                .unwrap()
                .unwrap()[..],
            b"s1b"
        );
        assert_eq!(count(&store, "omemo_sessions", account), 2);
        assert!(delete_session(&store, account, BOB, 1).unwrap());
        assert!(!delete_session(&store, account, BOB, 1).unwrap());
        assert!(
            load_session(&store, &sealer, account, BOB, 1)
                .unwrap()
                .is_none()
        );
        assert!(
            load_session(&store, &sealer, account, BOB, 2)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn trust_round_trip_and_list() {
        let (store, account, _) = setup();
        assert_eq!(get_trust(&store, account, BOB, b"fp").unwrap(), None);
        set_trust(&store, account, BOB, b"fp", TrustLevel::Blind).unwrap();
        set_trust(&store, account, BOB, b"fp2", TrustLevel::Undecided).unwrap();
        set_trust(&store, account, BOB, b"fp", TrustLevel::Verified).unwrap();
        assert_eq!(
            get_trust(&store, account, BOB, b"fp").unwrap(),
            Some(TrustLevel::Verified)
        );
        let list = list_trust(&store, account, BOB).unwrap();
        assert_eq!(list.len(), 2);
        assert!(
            list.iter()
                .any(|t| t.fingerprint == b"fp2" && t.level == TrustLevel::Undecided)
        );
        assert!(list_trust(&store, account, "eve@x").unwrap().is_empty());
        set_trust(&store, account, BOB, b"fp", TrustLevel::Distrusted).unwrap();
        assert_eq!(
            get_trust(&store, account, BOB, b"fp").unwrap(),
            Some(TrustLevel::Distrusted)
        );
        // The CHECK refuses an unknown level.
        assert!(
            store
                .conn()
                .execute(
                    "UPDATE omemo_trust SET level = 'maybe' WHERE account_id = ?1",
                    params![account]
                )
                .is_err()
        );
    }

    #[test]
    fn conversation_switch_defaults_to_off() {
        let (store, account, _) = setup();
        assert!(!conversation_enabled(&store, account, BOB).unwrap());
        set_conversation_enabled(&store, account, BOB, true).unwrap();
        assert!(conversation_enabled(&store, account, BOB).unwrap());
        set_conversation_enabled(&store, account, BOB, false).unwrap();
        assert!(!conversation_enabled(&store, account, BOB).unwrap());
    }

    #[test]
    fn reset_removes_one_account_only() {
        let (store, account, sealer) = setup();
        let carol = "carol@chord.localhost";
        let other = ensure_account(store.conn(), carol).unwrap();
        let other_sealer = Sealer::open_or_create(&store, other, carol, None).unwrap();
        for (id, s) in [(account, &sealer), (other, &other_sealer)] {
            save_identity(&store, s, id, 1, b"ik").unwrap();
            save_signed_prekey(&store, s, id, 1, b"spk").unwrap();
            save_prekeys(&store, s, id, &[(1, b"pk")]).unwrap();
            upsert_device(&store, id, BOB, 1, Some(b"k"), None).unwrap();
            save_session(&store, s, id, BOB, 1, b"st").unwrap();
            set_trust(&store, id, BOB, b"fp", TrustLevel::Blind).unwrap();
            set_conversation_enabled(&store, id, BOB, true).unwrap();
        }
        reset(&store, account).unwrap();
        for table in [
            "omemo_identity",
            "omemo_signed_prekeys",
            "omemo_prekeys",
            "omemo_devices",
            "omemo_sessions",
            "omemo_trust",
            "omemo_conversations",
            "omemo_data_key",
        ] {
            assert_eq!(count(&store, table, account), 0, "{table}");
            assert_eq!(
                count(&store, table, other),
                1,
                "{table} of the other account"
            );
        }
        assert_eq!(
            &load_session(&store, &other_sealer, other, BOB, 1)
                .unwrap()
                .unwrap()[..],
            b"st"
        );
    }

    #[test]
    fn the_timeline_shows_the_encryption_of_a_message() {
        use crate::store::queries::{
            Direction, KeyKind, MessageExtras, MessageKind, NewMessage, insert_message,
        };
        let (store, account, _) = setup();
        for (key, encryption) in [
            ("a", None),
            ("b", Some(ENCRYPTION_OMEMO)),
            ("c", Some(ENCRYPTION_FAILED)),
        ] {
            let new = NewMessage {
                kind: MessageKind::Chat,
                key_kind: KeyKind::StanzaId,
                key,
                direction: Direction::In,
                peer: BOB,
                sender: BOB,
                body: "x",
                timestamp: None,
                extras: MessageExtras {
                    encryption: encryption.map(str::to_owned),
                    ..MessageExtras::default()
                },
            };
            insert_message(store.conn(), account, &new)
                .unwrap()
                .unwrap();
        }
        let stored: Vec<Option<String>> = store
            .conn()
            .prepare("SELECT encryption FROM messages ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            stored,
            vec![
                None,
                Some("omemo:2".to_owned()),
                Some("omemo:2:failed".to_owned())
            ]
        );
    }
}
