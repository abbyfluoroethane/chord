//! Raw declarations of the libomemo-c functions that this crate uses.
//!
//! Written by hand and checked against the headers in `vendor/libomemo-c/src` (tag v0.5.1).
//! Names and signatures follow the C headers. Nothing here is safe to call without the
//! rules of the C library: see `native.rs` for the safe layer.

#![allow(non_camel_case_types, dead_code)]

use std::os::raw::{c_int, c_void};

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(
    signal_context,
    signal_protocol_store_context,
    signal_buffer,
    signal_type_base,
    ec_public_key,
    ec_private_key,
    ec_key_pair,
    ratchet_identity_key_pair,
    session_pre_key,
    session_signed_pre_key,
    session_pre_key_bundle,
    session_builder,
    session_cipher,
    session_record,
    session_state,
    ciphertext_message,
    pre_key_signal_message,
    signal_message,
    signal_int_list,
    signal_protocol_key_helper_pre_key_list_node,
);

#[repr(C)]
pub struct signal_protocol_address {
    pub name: *const u8,
    pub name_len: usize,
    pub device_id: i32,
}

// Result codes (signal_protocol.h).
pub const SG_ERR_INVAL: c_int = -22;
pub const SG_ERR_NOMEM: c_int = -12;
pub const SG_ERR_UNKNOWN: c_int = -1000;
pub const SG_ERR_DUPLICATE_MESSAGE: c_int = -1001;
pub const SG_ERR_INVALID_KEY: c_int = -1002;
pub const SG_ERR_INVALID_KEY_ID: c_int = -1003;
pub const SG_ERR_INVALID_MAC: c_int = -1004;
pub const SG_ERR_INVALID_MESSAGE: c_int = -1005;
pub const SG_ERR_INVALID_VERSION: c_int = -1006;
pub const SG_ERR_LEGACY_MESSAGE: c_int = -1007;
pub const SG_ERR_NO_SESSION: c_int = -1008;
pub const SG_ERR_STALE_KEY_EXCHANGE: c_int = -1009;
pub const SG_ERR_UNTRUSTED_IDENTITY: c_int = -1010;
pub const SG_ERR_INVALID_PROTO_BUF: c_int = -1100;

pub const SG_CIPHER_AES_CTR_NOPADDING: c_int = 1;
pub const SG_CIPHER_AES_CBC_PKCS5: c_int = 2;

/// Protocol version 4 is OMEMO 2 (`CIPHERTEXT_OMEMO_VERSION` in protocol.h).
pub const CIPHERTEXT_OMEMO_VERSION: u32 = 4;
pub const CIPHERTEXT_PREKEY_TYPE: c_int = 3;

pub type RandomFn = unsafe extern "C" fn(*mut u8, usize, *mut c_void) -> c_int;
pub type HmacInitFn = unsafe extern "C" fn(*mut *mut c_void, *const u8, usize, *mut c_void) -> c_int;
pub type UpdateFn = unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut c_void) -> c_int;
pub type FinalFn = unsafe extern "C" fn(*mut c_void, *mut *mut signal_buffer, *mut c_void) -> c_int;
pub type CleanupFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
pub type DigestInitFn = unsafe extern "C" fn(*mut *mut c_void, *mut c_void) -> c_int;
pub type CipherFn = unsafe extern "C" fn(
    *mut *mut signal_buffer,
    c_int,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *mut c_void,
) -> c_int;

#[repr(C)]
pub struct signal_crypto_provider {
    pub random_func: Option<RandomFn>,
    pub hmac_sha256_init_func: Option<HmacInitFn>,
    pub hmac_sha256_update_func: Option<UpdateFn>,
    pub hmac_sha256_final_func: Option<FinalFn>,
    pub hmac_sha256_cleanup_func: Option<CleanupFn>,
    pub sha512_digest_init_func: Option<DigestInitFn>,
    pub sha512_digest_update_func: Option<UpdateFn>,
    pub sha512_digest_final_func: Option<FinalFn>,
    pub sha512_digest_cleanup_func: Option<CleanupFn>,
    pub encrypt_func: Option<CipherFn>,
    pub decrypt_func: Option<CipherFn>,
    pub user_data: *mut c_void,
}

#[repr(C)]
pub struct signal_protocol_session_store {
    pub load_session_func: Option<
        unsafe extern "C" fn(
            *mut *mut signal_buffer,
            *mut *mut signal_buffer,
            *const signal_protocol_address,
            *mut c_void,
        ) -> c_int,
    >,
    pub get_sub_device_sessions_func: Option<
        unsafe extern "C" fn(*mut *mut signal_int_list, *const u8, usize, *mut c_void) -> c_int,
    >,
    pub store_session_func: Option<
        unsafe extern "C" fn(
            *const signal_protocol_address,
            *mut u8,
            usize,
            *mut u8,
            usize,
            *mut c_void,
        ) -> c_int,
    >,
    pub contains_session_func:
        Option<unsafe extern "C" fn(*const signal_protocol_address, *mut c_void) -> c_int>,
    pub delete_session_func:
        Option<unsafe extern "C" fn(*const signal_protocol_address, *mut c_void) -> c_int>,
    pub delete_all_sessions_func:
        Option<unsafe extern "C" fn(*const u8, usize, *mut c_void) -> c_int>,
    pub destroy_func: Option<unsafe extern "C" fn(*mut c_void)>,
    pub user_data: *mut c_void,
}

/// Used for both the prekey store and the signed prekey store: the C structs have the same
/// layout, only the names differ.
#[repr(C)]
pub struct signal_protocol_pre_key_store {
    pub load: Option<unsafe extern "C" fn(*mut *mut signal_buffer, u32, *mut c_void) -> c_int>,
    pub store: Option<unsafe extern "C" fn(u32, *mut u8, usize, *mut c_void) -> c_int>,
    pub contains: Option<unsafe extern "C" fn(u32, *mut c_void) -> c_int>,
    pub remove: Option<unsafe extern "C" fn(u32, *mut c_void) -> c_int>,
    pub destroy_func: Option<unsafe extern "C" fn(*mut c_void)>,
    pub user_data: *mut c_void,
}

#[repr(C)]
pub struct signal_protocol_identity_key_store {
    pub get_identity_key_pair: Option<
        unsafe extern "C" fn(*mut *mut signal_buffer, *mut *mut signal_buffer, *mut c_void) -> c_int,
    >,
    pub get_local_registration_id: Option<unsafe extern "C" fn(*mut c_void, *mut u32) -> c_int>,
    pub save_identity: Option<
        unsafe extern "C" fn(*const signal_protocol_address, *mut u8, usize, *mut c_void) -> c_int,
    >,
    pub is_trusted_identity: Option<
        unsafe extern "C" fn(*const signal_protocol_address, *mut u8, usize, *mut c_void) -> c_int,
    >,
    pub destroy_func: Option<unsafe extern "C" fn(*mut c_void)>,
    pub user_data: *mut c_void,
}

unsafe extern "C" {
    // signal_protocol.h
    pub fn signal_context_create(context: *mut *mut signal_context, user_data: *mut c_void) -> c_int;
    pub fn signal_context_set_crypto_provider(
        context: *mut signal_context,
        provider: *const signal_crypto_provider,
    ) -> c_int;
    pub fn signal_context_destroy(context: *mut signal_context);
    pub fn signal_protocol_store_context_create(
        context: *mut *mut signal_protocol_store_context,
        global: *mut signal_context,
    ) -> c_int;
    pub fn signal_protocol_store_context_set_session_store(
        context: *mut signal_protocol_store_context,
        store: *const signal_protocol_session_store,
    ) -> c_int;
    pub fn signal_protocol_store_context_set_pre_key_store(
        context: *mut signal_protocol_store_context,
        store: *const signal_protocol_pre_key_store,
    ) -> c_int;
    pub fn signal_protocol_store_context_set_signed_pre_key_store(
        context: *mut signal_protocol_store_context,
        store: *const signal_protocol_pre_key_store,
    ) -> c_int;
    pub fn signal_protocol_store_context_set_identity_key_store(
        context: *mut signal_protocol_store_context,
        store: *const signal_protocol_identity_key_store,
    ) -> c_int;
    pub fn signal_protocol_store_context_destroy(context: *mut signal_protocol_store_context);

    pub fn signal_buffer_create(data: *const u8, len: usize) -> *mut signal_buffer;
    pub fn signal_buffer_data(buffer: *mut signal_buffer) -> *mut u8;
    pub fn signal_buffer_len(buffer: *const signal_buffer) -> usize;
    pub fn signal_buffer_free(buffer: *mut signal_buffer);
    pub fn signal_buffer_bzero_free(buffer: *mut signal_buffer);
    pub fn signal_type_unref(instance: *mut signal_type_base);

    // curve.h
    pub fn curve_decode_point_ed(
        key: *mut *mut ec_public_key,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;
    pub fn curve_decode_private_point(
        key: *mut *mut ec_private_key,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;
    pub fn ec_public_key_get_ed(key: *const ec_public_key) -> *mut signal_buffer;
    pub fn ec_public_key_get_mont(key: *const ec_public_key) -> *mut signal_buffer;
    pub fn ec_private_key_serialize(
        buffer: *mut *mut signal_buffer,
        key: *const ec_private_key,
    ) -> c_int;
    pub fn ec_key_pair_get_public(pair: *const ec_key_pair) -> *mut ec_public_key;
    pub fn curve_decode_point_mont(
        key: *mut *mut ec_public_key,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;

    // ratchet.h
    pub fn ratchet_identity_key_pair_create(
        pair: *mut *mut ratchet_identity_key_pair,
        public: *mut ec_public_key,
        private: *mut ec_private_key,
    ) -> c_int;
    pub fn ratchet_identity_key_pair_get_public(
        pair: *const ratchet_identity_key_pair,
    ) -> *mut ec_public_key;
    pub fn ratchet_identity_key_pair_get_private(
        pair: *const ratchet_identity_key_pair,
    ) -> *mut ec_private_key;

    // key_helper.h
    pub fn signal_protocol_key_helper_generate_identity_key_pair(
        pair: *mut *mut ratchet_identity_key_pair,
        global: *mut signal_context,
    ) -> c_int;
    pub fn signal_protocol_key_helper_generate_pre_keys(
        head: *mut *mut signal_protocol_key_helper_pre_key_list_node,
        start: u32,
        count: u32,
        global: *mut signal_context,
    ) -> c_int;
    pub fn signal_protocol_key_helper_key_list_element(
        node: *const signal_protocol_key_helper_pre_key_list_node,
    ) -> *mut session_pre_key;
    pub fn signal_protocol_key_helper_key_list_next(
        node: *const signal_protocol_key_helper_pre_key_list_node,
    ) -> *mut signal_protocol_key_helper_pre_key_list_node;
    pub fn signal_protocol_key_helper_key_list_free(
        head: *mut signal_protocol_key_helper_pre_key_list_node,
    );
    pub fn signal_protocol_key_helper_generate_signed_pre_key(
        key: *mut *mut session_signed_pre_key,
        identity: *const ratchet_identity_key_pair,
        id: u32,
        timestamp: u64,
        global: *mut signal_context,
    ) -> c_int;

    // session_pre_key.h
    pub fn session_pre_key_serialize(
        buffer: *mut *mut signal_buffer,
        key: *const session_pre_key,
    ) -> c_int;
    pub fn session_pre_key_deserialize(
        key: *mut *mut session_pre_key,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;
    pub fn session_pre_key_get_id(key: *const session_pre_key) -> u32;
    pub fn session_pre_key_get_key_pair(key: *const session_pre_key) -> *mut ec_key_pair;
    pub fn session_signed_pre_key_serialize(
        buffer: *mut *mut signal_buffer,
        key: *const session_signed_pre_key,
    ) -> c_int;
    pub fn session_signed_pre_key_deserialize(
        key: *mut *mut session_signed_pre_key,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;
    pub fn session_signed_pre_key_get_id(key: *const session_signed_pre_key) -> u32;
    pub fn session_signed_pre_key_get_timestamp(key: *const session_signed_pre_key) -> u64;
    pub fn session_signed_pre_key_get_key_pair(key: *const session_signed_pre_key) -> *mut ec_key_pair;
    pub fn session_signed_pre_key_get_signature_omemo(key: *const session_signed_pre_key) -> *const u8;
    pub fn session_signed_pre_key_get_signature_omemo_len(key: *const session_signed_pre_key) -> usize;
    pub fn session_pre_key_bundle_create(
        bundle: *mut *mut session_pre_key_bundle,
        registration_id: u32,
        device_id: c_int,
        pre_key_id: u32,
        pre_key_public: *mut ec_public_key,
        signed_pre_key_id: u32,
        signed_pre_key_public: *mut ec_public_key,
        signature: *const u8,
        signature_len: usize,
        identity_key: *mut ec_public_key,
    ) -> c_int;

    // session_builder.h
    pub fn session_builder_create(
        builder: *mut *mut session_builder,
        store: *mut signal_protocol_store_context,
        address: *const signal_protocol_address,
        global: *mut signal_context,
    ) -> c_int;
    pub fn session_builder_set_version(builder: *mut session_builder, version: u32);
    pub fn session_builder_process_pre_key_bundle(
        builder: *mut session_builder,
        bundle: *mut session_pre_key_bundle,
    ) -> c_int;
    pub fn session_builder_free(builder: *mut session_builder);

    // session_cipher.h
    pub fn session_cipher_create(
        cipher: *mut *mut session_cipher,
        store: *mut signal_protocol_store_context,
        address: *const signal_protocol_address,
        global: *mut signal_context,
    ) -> c_int;
    pub fn session_cipher_set_version(cipher: *mut session_cipher, version: u32);
    pub fn session_cipher_encrypt(
        cipher: *mut session_cipher,
        data: *const u8,
        len: usize,
        out: *mut *mut ciphertext_message,
    ) -> c_int;
    pub fn session_cipher_decrypt_pre_key_signal_message(
        cipher: *mut session_cipher,
        message: *mut pre_key_signal_message,
        decrypt_context: *mut c_void,
        plaintext: *mut *mut signal_buffer,
    ) -> c_int;
    pub fn session_cipher_decrypt_signal_message(
        cipher: *mut session_cipher,
        message: *mut signal_message,
        decrypt_context: *mut c_void,
        plaintext: *mut *mut signal_buffer,
    ) -> c_int;
    pub fn session_cipher_free(cipher: *mut session_cipher);

    // protocol.h
    pub fn ciphertext_message_get_type(message: *const ciphertext_message) -> c_int;
    pub fn ciphertext_message_get_serialized(message: *const ciphertext_message) -> *mut signal_buffer;
    pub fn signal_message_deserialize_omemo(
        message: *mut *mut signal_message,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;
    pub fn pre_key_signal_message_deserialize_omemo(
        message: *mut *mut pre_key_signal_message,
        data: *const u8,
        len: usize,
        registration_id: u32,
        global: *mut signal_context,
    ) -> c_int;
    pub fn pre_key_signal_message_get_pre_key_id(message: *const pre_key_signal_message) -> u32;
    pub fn pre_key_signal_message_get_identity_key(
        message: *const pre_key_signal_message,
    ) -> *mut ec_public_key;

    // session_record.h and session_state.h
    pub fn session_record_deserialize(
        record: *mut *mut session_record,
        data: *const u8,
        len: usize,
        global: *mut signal_context,
    ) -> c_int;
    pub fn session_record_get_state(record: *mut session_record) -> *mut session_state;
    pub fn session_state_get_remote_identity_key(state: *const session_state) -> *mut ec_public_key;
}
