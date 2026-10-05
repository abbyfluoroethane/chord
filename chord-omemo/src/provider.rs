//! The crypto provider of libomemo-c.
//!
//! libomemo-c does not ship random, HMAC, SHA-512 and AES code. It asks the host for them
//! through callbacks. Each callback here is one direct call into a RustCrypto crate. There
//! is no own cipher logic: no padding code, no key schedule, no mode code.

use std::os::raw::{c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

use aes::{Aes128, Aes256};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, StreamCipher};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};

use crate::ffi::{self, signal_buffer};

/// Run `f`. A panic must not cross the FFI border, so it turns into an error code.
pub(crate) fn guard(f: impl FnOnce() -> c_int) -> c_int {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(ffi::SG_ERR_UNKNOWN)
}

/// Give bytes to the C library in a new `signal_buffer`. The library frees it.
unsafe fn out_buffer(out: *mut *mut signal_buffer, bytes: &[u8]) -> c_int {
    // SAFETY: the caller passes a valid `out` pointer. `signal_buffer_create` copies `bytes`.
    unsafe {
        let buf = ffi::signal_buffer_create(bytes.as_ptr(), bytes.len());
        if buf.is_null() {
            return ffi::SG_ERR_NOMEM;
        }
        *out = buf;
    }
    0
}

unsafe extern "C" fn random(data: *mut u8, len: usize, _user: *mut c_void) -> c_int {
    guard(|| {
        if len == 0 {
            return 0;
        }
        // SAFETY: libomemo-c gives a writable buffer of `len` bytes.
        let slice = unsafe { std::slice::from_raw_parts_mut(data, len) };
        match getrandom::fill(slice) {
            Ok(()) => 0,
            Err(_) => ffi::SG_ERR_UNKNOWN,
        }
    })
}

unsafe extern "C" fn hmac_init(
    ctx: *mut *mut c_void,
    key: *const u8,
    key_len: usize,
    _user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: `key` points to `key_len` readable bytes.
        let key = unsafe { std::slice::from_raw_parts(key, key_len) };
        match <Hmac<Sha256> as KeyInit>::new_from_slice(key) {
            Ok(mac) => {
                // SAFETY: `ctx` is a valid out pointer. `hmac_cleanup` frees the box.
                unsafe { *ctx = Box::into_raw(Box::new(mac)).cast() };
                0
            }
            Err(_) => ffi::SG_ERR_INVAL,
        }
    })
}

unsafe extern "C" fn hmac_update(
    ctx: *mut c_void,
    data: *const u8,
    len: usize,
    _user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: `ctx` comes from `hmac_init`. `data` has `len` readable bytes.
        let (mac, data) = unsafe {
            (&mut *ctx.cast::<Hmac<Sha256>>(), std::slice::from_raw_parts(data, len))
        };
        mac.update(data);
        0
    })
}

unsafe extern "C" fn hmac_final(
    ctx: *mut c_void,
    out: *mut *mut signal_buffer,
    _user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: `ctx` comes from `hmac_init` and stays valid until `hmac_cleanup`.
        let mac = unsafe { &*ctx.cast::<Hmac<Sha256>>() };
        let tag = mac.clone().finalize().into_bytes();
        // SAFETY: `out` is a valid out pointer.
        unsafe { out_buffer(out, &tag) }
    })
}

unsafe extern "C" fn hmac_cleanup(ctx: *mut c_void, _user: *mut c_void) {
    let _ = guard(|| {
        if !ctx.is_null() {
            // SAFETY: `ctx` comes from `Box::into_raw` in `hmac_init`, freed once here.
            drop(unsafe { Box::from_raw(ctx.cast::<Hmac<Sha256>>()) });
        }
        0
    });
}

unsafe extern "C" fn sha512_init(ctx: *mut *mut c_void, _user: *mut c_void) -> c_int {
    guard(|| {
        // SAFETY: `ctx` is a valid out pointer. `sha512_cleanup` frees the box.
        unsafe { *ctx = Box::into_raw(Box::new(Sha512::new())).cast() };
        0
    })
}

unsafe extern "C" fn sha512_update(
    ctx: *mut c_void,
    data: *const u8,
    len: usize,
    _user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: `ctx` comes from `sha512_init`. `data` has `len` readable bytes.
        let (hasher, data) =
            unsafe { (&mut *ctx.cast::<Sha512>(), std::slice::from_raw_parts(data, len)) };
        hasher.update(data);
        0
    })
}

unsafe extern "C" fn sha512_final(
    ctx: *mut c_void,
    out: *mut *mut signal_buffer,
    _user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: `ctx` comes from `sha512_init` and stays valid until `sha512_cleanup`.
        let hasher = unsafe { &*ctx.cast::<Sha512>() };
        let digest = hasher.clone().finalize();
        // SAFETY: `out` is a valid out pointer.
        unsafe { out_buffer(out, &digest) }
    })
}

unsafe extern "C" fn sha512_cleanup(ctx: *mut c_void, _user: *mut c_void) {
    let _ = guard(|| {
        if !ctx.is_null() {
            // SAFETY: `ctx` comes from `Box::into_raw` in `sha512_init`, freed once here.
            drop(unsafe { Box::from_raw(ctx.cast::<Sha512>()) });
        }
        0
    });
}

/// AES-CBC with PKCS#7 padding, or AES-CTR. One call into `cbc` or `ctr`.
fn aes(encrypt: bool, cipher: c_int, key: &[u8], iv: &[u8], input: &[u8]) -> Result<Vec<u8>, ()> {
    match cipher {
        ffi::SG_CIPHER_AES_CBC_PKCS5 => match key.len() {
            32 => {
                if encrypt {
                    let c = cbc::Encryptor::<Aes256>::new_from_slices(key, iv).map_err(|_| ())?;
                    Ok(c.encrypt_padded_vec::<Pkcs7>(input))
                } else {
                    let c = cbc::Decryptor::<Aes256>::new_from_slices(key, iv).map_err(|_| ())?;
                    c.decrypt_padded_vec::<Pkcs7>(input).map_err(|_| ())
                }
            }
            16 => {
                if encrypt {
                    let c = cbc::Encryptor::<Aes128>::new_from_slices(key, iv).map_err(|_| ())?;
                    Ok(c.encrypt_padded_vec::<Pkcs7>(input))
                } else {
                    let c = cbc::Decryptor::<Aes128>::new_from_slices(key, iv).map_err(|_| ())?;
                    c.decrypt_padded_vec::<Pkcs7>(input).map_err(|_| ())
                }
            }
            _ => Err(()),
        },
        ffi::SG_CIPHER_AES_CTR_NOPADDING => {
            // CTR is the same operation for both directions.
            let mut out = input.to_vec();
            match key.len() {
                32 => ctr::Ctr128BE::<Aes256>::new_from_slices(key, iv)
                    .map_err(|_| ())?
                    .apply_keystream(&mut out),
                16 => ctr::Ctr128BE::<Aes128>::new_from_slices(key, iv)
                    .map_err(|_| ())?
                    .apply_keystream(&mut out),
                _ => return Err(()),
            }
            Ok(out)
        }
        _ => Err(()),
    }
}

unsafe fn aes_callback(
    encrypt: bool,
    out: *mut *mut signal_buffer,
    cipher: c_int,
    key: (*const u8, usize),
    iv: (*const u8, usize),
    input: (*const u8, usize),
) -> c_int {
    guard(|| {
        // SAFETY: libomemo-c passes pointers that are valid for the given lengths.
        let (key, iv, input) = unsafe {
            (
                std::slice::from_raw_parts(key.0, key.1),
                std::slice::from_raw_parts(iv.0, iv.1),
                if input.1 == 0 { &[][..] } else { std::slice::from_raw_parts(input.0, input.1) },
            )
        };
        match aes(encrypt, cipher, key, iv, input) {
            // SAFETY: `out` is a valid out pointer.
            Ok(bytes) => unsafe { out_buffer(out, &bytes) },
            Err(()) => ffi::SG_ERR_UNKNOWN,
        }
    })
}

unsafe extern "C" fn encrypt(
    out: *mut *mut signal_buffer,
    cipher: c_int,
    key: *const u8,
    key_len: usize,
    iv: *const u8,
    iv_len: usize,
    data: *const u8,
    data_len: usize,
    _user: *mut c_void,
) -> c_int {
    // SAFETY: the pointers come from libomemo-c and are valid for the given lengths.
    unsafe { aes_callback(true, out, cipher, (key, key_len), (iv, iv_len), (data, data_len)) }
}

unsafe extern "C" fn decrypt(
    out: *mut *mut signal_buffer,
    cipher: c_int,
    key: *const u8,
    key_len: usize,
    iv: *const u8,
    iv_len: usize,
    data: *const u8,
    data_len: usize,
    _user: *mut c_void,
) -> c_int {
    // SAFETY: the pointers come from libomemo-c and are valid for the given lengths.
    unsafe { aes_callback(false, out, cipher, (key, key_len), (iv, iv_len), (data, data_len)) }
}

/// The provider table. libomemo-c copies it, so a value on the stack is enough.
pub(crate) fn provider() -> ffi::signal_crypto_provider {
    ffi::signal_crypto_provider {
        random_func: Some(random),
        hmac_sha256_init_func: Some(hmac_init),
        hmac_sha256_update_func: Some(hmac_update),
        hmac_sha256_final_func: Some(hmac_final),
        hmac_sha256_cleanup_func: Some(hmac_cleanup),
        sha512_digest_init_func: Some(sha512_init),
        sha512_digest_update_func: Some(sha512_update),
        sha512_digest_final_func: Some(sha512_final),
        sha512_digest_cleanup_func: Some(sha512_cleanup),
        encrypt_func: Some(encrypt),
        decrypt_func: Some(decrypt),
        user_data: std::ptr::null_mut(),
    }
}
