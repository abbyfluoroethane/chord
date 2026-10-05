//! The keys of this device: identity key, signed prekeys, one-time prekeys. All key
//! generation and signing happens inside libomemo-c.
//!
//! An [`Identity`] is plain data. It turns into bytes with [`Identity::to_bytes`] and back
//! with [`Identity::from_bytes`]. The caller stores the bytes. The bytes hold private keys,
//! so the caller must protect them (see the secrets section of `docs/omemo-plan.md`).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use zeroize::{Zeroize, Zeroizing};

use crate::Error;
use crate::ffi;
use crate::native::{Buf, Ctx, Rc, StoreState, check};

/// How many one-time prekeys a new identity makes. The XEP says "around 100".
pub const PREKEY_COUNT: usize = 100;
/// The XEP says a bundle must always hold at least 25 prekeys.
pub const PREKEY_MIN: usize = 25;

/// libomemo-c wraps prekey ids at this number (`PRE_KEY_MEDIUM_MAX_VALUE`) and treats the
/// value itself as "no prekey".
const PREKEY_ID_LIMIT: u32 = 0xFF_FFFE;

const FORMAT_VERSION: u8 = 1;
const MAGIC: &[u8; 4] = b"COI\0";

/// The public part of a device, in the byte forms of the OMEMO 2 `<bundle>` element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    /// `<ik>`: the Ed25519 public identity key, 32 bytes.
    pub ik: Vec<u8>,
    /// `<spk id>`: the id of the signed prekey.
    pub spk_id: u32,
    /// `<spk>`: the Curve25519 public signed prekey, 32 bytes.
    pub spk: Vec<u8>,
    /// `<spks>`: the Ed25519 signature of `spk` by `ik`, 64 bytes.
    pub spks: Vec<u8>,
    /// `<pk id>` elements: one-time prekeys, Curve25519 public keys of 32 bytes.
    pub prekeys: Vec<(u32, Vec<u8>)>,
}

/// The keys of this device.
pub struct Identity {
    device_id: u32,
    /// Curve25519 private identity key, 32 bytes.
    ik_private: Zeroizing<Vec<u8>>,
    /// Ed25519 public identity key, 32 bytes.
    ik_public: Vec<u8>,
    current_spk: u32,
    next_spk_id: u32,
    next_prekey_id: u32,
    /// Serialized libomemo-c records. They hold private keys.
    signed_prekeys: BTreeMap<u32, Vec<u8>>,
    prekeys: BTreeMap<u32, Vec<u8>>,
}

impl Drop for Identity {
    fn drop(&mut self) {
        for v in self.signed_prekeys.values_mut().chain(self.prekeys.values_mut()) {
            v.zeroize();
        }
    }
}

impl core::fmt::Debug for Identity {
    // Never print the keys.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Identity")
            .field("device_id", &self.device_id)
            .field("prekeys", &self.prekeys.len())
            .finish_non_exhaustive()
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Copy the 32 bytes out of an optional buffer.
fn buf_bytes(buf: *mut ffi::signal_buffer) -> Result<Vec<u8>, Error> {
    // SAFETY: `buf` is a new buffer from the library or null.
    let buf = unsafe { Buf::from_raw(buf) }.ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
    Ok(buf.bytes().to_vec())
}

/// The Curve25519 public key (the fingerprint form) of an Ed25519 public key.
fn mont_of_ed(ctx: &Ctx, ed: &[u8]) -> Result<Vec<u8>, Error> {
    let mut key = std::ptr::null_mut();
    // SAFETY: `ed` is a valid slice. `key` is written on success and then owned by `Rc`.
    unsafe {
        check(ffi::curve_decode_point_ed(&mut key, ed.as_ptr(), ed.len(), ctx.global))
            .map_err(|_| Error::Malformed("identity key"))?;
        let key = Rc::from_raw(key).ok_or(Error::Malformed("identity key"))?;
        buf_bytes(ffi::ec_public_key_get_mont(key.as_ptr()))
    }
}

/// The fingerprint of an identity key, in the form that XEP-0384 says to show to the user:
/// the Curve25519 form of the key, as lowercase hex in 8 groups of 8 characters.
///
/// `ik` is the Ed25519 key of the `<ik>` element (32 bytes).
pub fn fingerprint(ik: &[u8]) -> Result<String, Error> {
    if ik.len() != 32 {
        return Err(Error::Malformed("identity key"));
    }
    let ctx = Ctx::new(StoreState::default())?;
    let mont = mont_of_ed(&ctx, ik)?;
    let hex: String = mont.iter().map(|b| format!("{b:02x}")).collect();
    let groups: Vec<&str> = (0..8).map(|i| &hex[i * 8..i * 8 + 8]).collect();
    Ok(groups.join(" "))
}

/// Make `count` prekeys from `start`. Returns the serialized records, by id.
fn make_prekeys(ctx: &Ctx, start: u32, count: u32) -> Result<Vec<(u32, Vec<u8>)>, Error> {
    let mut head = std::ptr::null_mut();
    // SAFETY: `head` is written on success. We free the list below, also on error paths.
    unsafe {
        check(ffi::signal_protocol_key_helper_generate_pre_keys(&mut head, start, count, ctx.global))?;
        let mut out = Vec::new();
        let mut node = head;
        let mut result = Ok(());
        while !node.is_null() {
            let key = ffi::signal_protocol_key_helper_key_list_element(node);
            let mut buf = std::ptr::null_mut();
            match check(ffi::session_pre_key_serialize(&mut buf, key)) {
                Ok(_) => match Buf::from_raw(buf) {
                    Some(b) => out.push((ffi::session_pre_key_get_id(key), b.bytes().to_vec())),
                    None => {
                        result = Err(Error::Library(ffi::SG_ERR_NOMEM));
                        break;
                    }
                },
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
            node = ffi::signal_protocol_key_helper_key_list_next(node);
        }
        ffi::signal_protocol_key_helper_key_list_free(head);
        result.map(|()| out)
    }
}

/// The public Curve25519 key of a serialized prekey record.
fn prekey_public(ctx: &Ctx, record: &[u8]) -> Result<Vec<u8>, Error> {
    let mut key = std::ptr::null_mut();
    // SAFETY: `record` is a valid slice. The pair is borrowed from `key`, which lives on.
    unsafe {
        check(ffi::session_pre_key_deserialize(&mut key, record.as_ptr(), record.len(), ctx.global))
            .map_err(|_| Error::Malformed("prekey record"))?;
        let key = Rc::from_raw(key).ok_or(Error::Malformed("prekey record"))?;
        let pair = ffi::session_pre_key_get_key_pair(key.as_ptr());
        buf_bytes(ffi::ec_public_key_get_mont(ffi::ec_key_pair_get_public(pair)))
    }
}

/// The public key, the id and the signature of a serialized signed prekey record.
fn spk_parts(ctx: &Ctx, record: &[u8]) -> Result<(u32, Vec<u8>, Vec<u8>, u64), Error> {
    let mut key = std::ptr::null_mut();
    // SAFETY: `record` is a valid slice. All getters borrow from `key`, which lives on.
    unsafe {
        check(ffi::session_signed_pre_key_deserialize(&mut key, record.as_ptr(), record.len(), ctx.global))
            .map_err(|_| Error::Malformed("signed prekey record"))?;
        let key = Rc::from_raw(key).ok_or(Error::Malformed("signed prekey record"))?;
        let pair = ffi::session_signed_pre_key_get_key_pair(key.as_ptr());
        let public = buf_bytes(ffi::ec_public_key_get_mont(ffi::ec_key_pair_get_public(pair)))?;
        let sig_ptr = ffi::session_signed_pre_key_get_signature_omemo(key.as_ptr());
        let sig_len = ffi::session_signed_pre_key_get_signature_omemo_len(key.as_ptr());
        if sig_ptr.is_null() || sig_len != 64 {
            return Err(Error::Malformed("signed prekey record"));
        }
        let sig = std::slice::from_raw_parts(sig_ptr, sig_len).to_vec();
        Ok((
            ffi::session_signed_pre_key_get_id(key.as_ptr()),
            public,
            sig,
            ffi::session_signed_pre_key_get_timestamp(key.as_ptr()),
        ))
    }
}

impl Identity {
    /// Make a new identity: a random device id, an identity key pair, one signed prekey
    /// with its signature, and [`PREKEY_COUNT`] one-time prekeys.
    pub fn generate() -> Result<Identity, Error> {
        let ctx = Ctx::new(StoreState::default())?;

        // A device id in 1..=2^31-1.
        let mut raw = [0u8; 4];
        getrandom::fill(&mut raw).map_err(|_| Error::Rng)?;
        let device_id = (u32::from_le_bytes(raw) % 0x7FFF_FFFF) + 1;

        let mut pair = std::ptr::null_mut();
        // SAFETY: `pair` is written on success and owned by `Rc`. Getters borrow from it.
        let (ik_private, ik_public) = unsafe {
            check(ffi::signal_protocol_key_helper_generate_identity_key_pair(&mut pair, ctx.global))?;
            let pair = Rc::from_raw(pair).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            let mut private = std::ptr::null_mut();
            check(ffi::ec_private_key_serialize(
                &mut private,
                ffi::ratchet_identity_key_pair_get_private(pair.as_ptr()),
            ))?;
            let private = Buf::from_raw(private).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            let public = buf_bytes(ffi::ec_public_key_get_ed(ffi::ratchet_identity_key_pair_get_public(
                pair.as_ptr(),
            )))?;
            (Zeroizing::new(private.bytes().to_vec()), public)
        };

        let mut id = Identity {
            device_id,
            ik_private,
            ik_public,
            current_spk: 0,
            next_spk_id: 1,
            next_prekey_id: 1,
            signed_prekeys: BTreeMap::new(),
            prekeys: BTreeMap::new(),
        };
        id.rotate_signed_prekey()?;
        id.refill_prekeys(PREKEY_COUNT)?;
        Ok(id)
    }

    /// The device id: a number in 1..=2^31-1.
    pub fn device_id(&self) -> u32 {
        self.device_id
    }

    /// The Ed25519 public identity key (the `<ik>` bytes).
    pub fn identity_key(&self) -> &[u8] {
        &self.ik_public
    }

    /// The fingerprint of this device, for display.
    pub fn fingerprint(&self) -> Result<String, Error> {
        fingerprint(&self.ik_public)
    }

    /// The bundle to publish: the current signed prekey and all unused prekeys.
    pub fn bundle(&self) -> Result<Bundle, Error> {
        let ctx = Ctx::new(StoreState::default())?;
        let record = self.signed_prekeys.get(&self.current_spk).ok_or(Error::Malformed("signed prekey"))?;
        let (spk_id, spk, spks, _) = spk_parts(&ctx, record)?;
        let mut prekeys = Vec::with_capacity(self.prekeys.len());
        for (id, record) in &self.prekeys {
            prekeys.push((*id, prekey_public(&ctx, record)?));
        }
        Ok(Bundle { ik: self.ik_public.clone(), spk_id, spk, spks, prekeys })
    }

    /// How many unused one-time prekeys are left.
    pub fn prekey_count(&self) -> usize {
        self.prekeys.len()
    }

    /// Remove a prekey after a key exchange used it. Returns false if it was not there.
    pub fn remove_prekey(&mut self, id: u32) -> bool {
        match self.prekeys.remove(&id) {
            Some(mut record) => {
                record.zeroize();
                true
            }
            None => false,
        }
    }

    /// Make new prekeys until there are `target`. Returns the new public keys. The caller
    /// must publish the bundle again. Does nothing if there are enough.
    pub fn refill_prekeys(&mut self, target: usize) -> Result<Vec<(u32, Vec<u8>)>, Error> {
        let missing = target.saturating_sub(self.prekeys.len());
        if missing == 0 {
            return Ok(Vec::new());
        }
        let ctx = Ctx::new(StoreState::default())?;
        let made = make_prekeys(&ctx, self.next_prekey_id, missing as u32)?;
        let mut added = Vec::with_capacity(made.len());
        for (id, record) in made {
            added.push((id, prekey_public(&ctx, &record)?));
            self.next_prekey_id = (id % PREKEY_ID_LIMIT) + 1;
            self.prekeys.insert(id, record);
        }
        Ok(added)
    }

    /// Make a new signed prekey and use it for the bundle. The old ones stay, because
    /// delayed messages can still use them (XEP-0384, section 4.2). Returns the id, the
    /// public key and the signature.
    pub fn rotate_signed_prekey(&mut self) -> Result<(u32, Vec<u8>, Vec<u8>), Error> {
        let ctx = Ctx::new(StoreState::default())?;
        let id = self.next_spk_id;
        let mut private = std::ptr::null_mut();
        let mut public = std::ptr::null_mut();
        let mut pair = std::ptr::null_mut();
        let mut spk = std::ptr::null_mut();
        let mut buf = std::ptr::null_mut();
        // SAFETY: the out pointers are written on success and owned by `Rc` or `Buf` at once.
        let record = unsafe {
            check(ffi::curve_decode_private_point(&mut private, self.ik_private.as_ptr(), 32, ctx.global))?;
            let private = Rc::from_raw(private).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            check(ffi::curve_decode_point_ed(&mut public, self.ik_public.as_ptr(), 32, ctx.global))?;
            let public = Rc::from_raw(public).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            check(ffi::ratchet_identity_key_pair_create(&mut pair, public.as_ptr(), private.as_ptr()))?;
            let pair = Rc::from_raw(pair).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            check(ffi::signal_protocol_key_helper_generate_signed_pre_key(
                &mut spk,
                pair.as_ptr(),
                id,
                now_ms(),
                ctx.global,
            ))?;
            let spk = Rc::from_raw(spk).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            check(ffi::session_signed_pre_key_serialize(&mut buf, spk.as_ptr()))?;
            Buf::from_raw(buf).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?.bytes().to_vec()
        };
        let (_, public, sig, _) = spk_parts(&ctx, &record)?;
        self.signed_prekeys.insert(id, record);
        self.current_spk = id;
        self.next_spk_id = id.checked_add(1).filter(|n| *n < PREKEY_ID_LIMIT).unwrap_or(1);
        Ok((id, public, sig))
    }

    /// The id of the signed prekey that the bundle uses now.
    pub fn current_signed_prekey_id(&self) -> u32 {
        self.current_spk
    }

    /// The ids of all signed prekeys that we still hold, with the creation time in
    /// milliseconds since the Unix epoch. The caller decides when to drop an old one.
    pub fn signed_prekeys(&self) -> Result<Vec<(u32, u64)>, Error> {
        let ctx = Ctx::new(StoreState::default())?;
        let mut out = Vec::new();
        for record in self.signed_prekeys.values() {
            let (id, _, _, ts) = spk_parts(&ctx, record)?;
            out.push((id, ts));
        }
        Ok(out)
    }

    /// Drop an old signed prekey. The current one stays. Returns true if one was dropped.
    pub fn discard_signed_prekey(&mut self, id: u32) -> bool {
        if id == self.current_spk {
            return false;
        }
        match self.signed_prekeys.remove(&id) {
            Some(mut record) => {
                record.zeroize();
                true
            }
            None => false,
        }
    }

    /// Serialize all of this identity, private keys included.
    ///
    /// Format, all numbers little-endian: magic `COI\0`, version byte, device id, 32 bytes
    /// private identity key, 32 bytes public identity key, current signed prekey id, next
    /// signed prekey id, next prekey id, then two lists (signed prekeys, prekeys). A list
    /// is a count, then for each item an id, a length, and the libomemo-c record.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::new());
        out.extend_from_slice(MAGIC);
        out.push(FORMAT_VERSION);
        out.extend_from_slice(&self.device_id.to_le_bytes());
        out.extend_from_slice(&self.ik_private);
        out.extend_from_slice(&self.ik_public);
        for n in [self.current_spk, self.next_spk_id, self.next_prekey_id] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        for list in [&self.signed_prekeys, &self.prekeys] {
            out.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for (id, record) in list {
                out.extend_from_slice(&id.to_le_bytes());
                out.extend_from_slice(&(record.len() as u32).to_le_bytes());
                out.extend_from_slice(record);
            }
        }
        out
    }

    /// Read an identity from the bytes of [`Identity::to_bytes`]. The records are parsed
    /// by libomemo-c, so a broken record fails here and not at the first message.
    pub fn from_bytes(bytes: &[u8]) -> Result<Identity, Error> {
        let bad = Error::Malformed("identity record");
        let mut r = Reader { bytes, pos: 0 };
        if r.take(4).ok_or(bad.clone())? != MAGIC || r.take(1).ok_or(bad.clone())? != [FORMAT_VERSION] {
            return Err(bad);
        }
        let device_id = r.u32().ok_or(bad.clone())?;
        let ik_private = Zeroizing::new(r.take(32).ok_or(bad.clone())?.to_vec());
        let ik_public = r.take(32).ok_or(bad.clone())?.to_vec();
        let current_spk = r.u32().ok_or(bad.clone())?;
        let next_spk_id = r.u32().ok_or(bad.clone())?;
        let next_prekey_id = r.u32().ok_or(bad.clone())?;
        let mut lists = [BTreeMap::new(), BTreeMap::new()];
        for list in &mut lists {
            let count = r.u32().ok_or(bad.clone())?;
            for _ in 0..count {
                let id = r.u32().ok_or(bad.clone())?;
                let len = r.u32().ok_or(bad.clone())? as usize;
                let record = r.take(len).ok_or(bad.clone())?.to_vec();
                list.insert(id, record);
            }
        }
        if r.pos != bytes.len() || device_id == 0 || device_id > 0x7FFF_FFFF {
            return Err(bad);
        }
        let [signed_prekeys, prekeys] = lists;
        let id = Identity {
            device_id,
            ik_private,
            ik_public,
            current_spk,
            next_spk_id,
            next_prekey_id,
            signed_prekeys,
            prekeys,
        };
        // Check that every record parses, and that the current signed prekey exists.
        id.bundle()?;
        id.signed_prekeys()?;
        Ok(id)
    }

    /// The data for the stores of one call.
    pub(crate) fn store_state(&self) -> StoreState {
        let mut state = StoreState::default();
        state.device_id = self.device_id;
        state.ik_private = self.ik_private.clone();
        state.ik_public = self.ik_public.clone();
        state.prekeys = self.prekeys.clone();
        state.signed_prekeys = self.signed_prekeys.clone();
        state
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}
