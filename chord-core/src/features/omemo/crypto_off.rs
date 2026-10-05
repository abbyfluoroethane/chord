//! The crypto layer of a build with no `chord-omemo` (wasm). Every call says that OMEMO is
//! not available. See `crypto.rs` for the real code.

#![allow(dead_code)]

use zeroize::Zeroizing;

use super::xml::BundleXml;

/// True when this build can do OMEMO.
pub(crate) const AVAILABLE: bool = false;

const OFF: &str = "OMEMO is not in this build";

/// No keys exist in this build.
pub(crate) struct Keys(());

impl Keys {
    pub(crate) fn generate() -> Result<Self, String> {
        Err(OFF.into())
    }
    pub(crate) fn from_bytes(_: &[u8]) -> Result<Self, String> {
        Err(OFF.into())
    }
    pub(crate) fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(Vec::new())
    }
    pub(crate) fn device_id(&self) -> u32 {
        0
    }
    pub(crate) fn identity_key(&self) -> &[u8] {
        &[]
    }
    pub(crate) fn bundle(&self) -> Result<BundleXml, String> {
        Err(OFF.into())
    }
    pub(crate) fn prekey_count(&self) -> usize {
        0
    }
    pub(crate) fn remove_prekey(&mut self, _: u32) -> bool {
        false
    }
    pub(crate) fn refill_prekeys(&mut self) -> Result<(), String> {
        Err(OFF.into())
    }
    pub(crate) fn rotate_signed_prekey(&mut self) -> Result<(), String> {
        Err(OFF.into())
    }
    pub(crate) fn signed_prekeys(&self) -> Result<Vec<(u32, u64)>, String> {
        Err(OFF.into())
    }
    pub(crate) fn current_signed_prekey_id(&self) -> u32 {
        0
    }
    pub(crate) fn discard_signed_prekey(&mut self, _: u32) -> bool {
        false
    }
}

pub(crate) const PREKEY_MIN: usize = 25;

pub(crate) fn fingerprint(_: &[u8]) -> Option<String> {
    None
}

pub(crate) fn encrypt_payload(_: &[u8]) -> Result<(Zeroizing<Vec<u8>>, Vec<u8>), String> {
    Err(OFF.into())
}

pub(crate) fn empty_material() -> Vec<u8> {
    vec![0; 32]
}

pub(crate) struct SealedKey {
    pub data: Vec<u8>,
    pub kex: bool,
    pub session: Vec<u8>,
}

pub(crate) fn encrypt_key(
    _: &Keys,
    _: Option<&[u8]>,
    _: Option<&BundleXml>,
    _: &[u8],
) -> Result<SealedKey, String> {
    Err(OFF.into())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OpenError {
    Duplicate,
    Other(String),
}

pub(crate) struct OpenedKey {
    pub material: Zeroizing<Vec<u8>>,
    pub session: Vec<u8>,
    pub used_prekey: Option<u32>,
    pub remote_identity_key: Vec<u8>,
}

pub(crate) fn decrypt_key(
    _: &Keys,
    _: Option<&[u8]>,
    _: &[u8],
    _: bool,
) -> Result<OpenedKey, OpenError> {
    Err(OpenError::Other(OFF.into()))
}

pub(crate) fn decrypt_payload(_: &[u8], _: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    Err(OFF.into())
}
