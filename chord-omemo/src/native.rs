//! The safe layer over the C library: owned buffers, reference-counted objects, and one
//! context with in-memory stores.
//!
//! A [`Ctx`] holds a `signal_context` and a store context. The stores read from a
//! [`StoreState`] that the caller of this crate fills from bytes. When a call ends, the
//! results sit in the same `StoreState`. Nothing lives longer than one call, so a failed
//! call leaves no trace.

use std::collections::BTreeMap;
use std::os::raw::{c_int, c_void};

use zeroize::{Zeroize, Zeroizing};

use crate::Error;
use crate::ffi::{self, signal_buffer};
use crate::provider::{guard, provider};

/// An owned `signal_buffer`. The bytes are wiped when it drops.
pub(crate) struct Buf(*mut signal_buffer);

impl Buf {
    /// Take the ownership of a buffer. `None` for a null pointer.
    pub(crate) unsafe fn from_raw(ptr: *mut signal_buffer) -> Option<Buf> {
        if ptr.is_null() { None } else { Some(Buf(ptr)) }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        // SAFETY: `self.0` is a live buffer. `signal_buffer_data` points to `len` bytes that
        // stay valid while `self` lives.
        unsafe {
            let len = ffi::signal_buffer_len(self.0);
            if len == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(ffi::signal_buffer_data(self.0), len)
            }
        }
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        // SAFETY: we own the buffer and free it once.
        unsafe { ffi::signal_buffer_bzero_free(self.0) }
    }
}

/// A reference-counted libomemo-c object that we own one reference of.
pub(crate) struct Rc<T>(*mut T);

impl<T> Rc<T> {
    pub(crate) unsafe fn from_raw(ptr: *mut T) -> Option<Rc<T>> {
        if ptr.is_null() { None } else { Some(Rc(ptr)) }
    }

    pub(crate) fn as_ptr(&self) -> *mut T {
        self.0
    }
}

impl<T> Drop for Rc<T> {
    fn drop(&mut self) {
        // SAFETY: all these C types start with `signal_type_base`. We drop our reference once.
        unsafe { ffi::signal_type_unref(self.0.cast()) }
    }
}

/// Turn a C result code into a `Result`. Negative is an error.
pub(crate) fn check(code: c_int) -> Result<c_int, Error> {
    if code >= 0 { Ok(code) } else { Err(Error::Library(code)) }
}

/// What the in-memory stores hold during one call.
#[derive(Default)]
pub(crate) struct StoreState {
    pub device_id: u32,
    /// Curve25519 private identity key, 32 bytes.
    pub ik_private: Zeroizing<Vec<u8>>,
    /// Ed25519 form of the public identity key, 32 bytes.
    pub ik_public: Vec<u8>,
    /// Serialized `session_pre_key` records by id.
    pub prekeys: BTreeMap<u32, Vec<u8>>,
    /// Serialized `session_signed_pre_key` records by id.
    pub signed_prekeys: BTreeMap<u32, Vec<u8>>,
    /// The session record that we have before the call.
    pub session: Option<Vec<u8>>,
    /// The session record that the library stored during the call.
    pub new_session: Option<Vec<u8>>,
    /// The prekey that the library asked us to remove.
    pub used_prekey: Option<u32>,
}

impl Drop for StoreState {
    fn drop(&mut self) {
        for v in self.prekeys.values_mut().chain(self.signed_prekeys.values_mut()) {
            v.zeroize();
        }
        if let Some(s) = self.session.as_mut() {
            s.zeroize();
        }
        if let Some(s) = self.new_session.as_mut() {
            s.zeroize();
        }
    }
}

/// The address of the one remote device. The library needs a name and a number, and our
/// context only ever holds one session, so they are fixed.
const ADDRESS_NAME: &[u8] = b"r";

pub(crate) fn address() -> ffi::signal_protocol_address {
    ffi::signal_protocol_address { name: ADDRESS_NAME.as_ptr(), name_len: ADDRESS_NAME.len(), device_id: 1 }
}

pub(crate) struct Ctx {
    pub(crate) global: *mut ffi::signal_context,
    pub(crate) store: *mut ffi::signal_protocol_store_context,
    state: *mut StoreState,
}

impl Ctx {
    pub(crate) fn new(state: StoreState) -> Result<Ctx, Error> {
        let state = Box::into_raw(Box::new(state));
        // From here on `ctx` owns `state` and frees it on drop, also on the early returns.
        let mut ctx = Ctx { global: std::ptr::null_mut(), store: std::ptr::null_mut(), state };
        // SAFETY: all pointers are valid. Out pointers are written by the library only on
        // success. libomemo-c copies the provider and the store tables, so locals are fine.
        unsafe {
            check(ffi::signal_context_create(&mut ctx.global, std::ptr::null_mut()))?;
            let prov = provider();
            check(ffi::signal_context_set_crypto_provider(ctx.global, &prov))?;
            check(ffi::signal_protocol_store_context_create(&mut ctx.store, ctx.global))?;
            let user = state.cast::<c_void>();
            check(ffi::signal_protocol_store_context_set_session_store(
                ctx.store,
                &ffi::signal_protocol_session_store {
                    load_session_func: Some(session_load),
                    get_sub_device_sessions_func: Some(session_sub_devices),
                    store_session_func: Some(session_store),
                    contains_session_func: Some(session_contains),
                    delete_session_func: Some(session_delete),
                    delete_all_sessions_func: Some(session_delete_all),
                    destroy_func: None,
                    user_data: user,
                },
            ))?;
            check(ffi::signal_protocol_store_context_set_pre_key_store(
                ctx.store,
                &ffi::signal_protocol_pre_key_store {
                    load: Some(pre_key_load),
                    store: Some(pre_key_store),
                    contains: Some(pre_key_contains),
                    remove: Some(pre_key_remove),
                    destroy_func: None,
                    user_data: user,
                },
            ))?;
            check(ffi::signal_protocol_store_context_set_signed_pre_key_store(
                ctx.store,
                &ffi::signal_protocol_pre_key_store {
                    load: Some(spk_load),
                    store: Some(spk_store),
                    contains: Some(spk_contains),
                    remove: Some(spk_remove),
                    destroy_func: None,
                    user_data: user,
                },
            ))?;
            check(ffi::signal_protocol_store_context_set_identity_key_store(
                ctx.store,
                &ffi::signal_protocol_identity_key_store {
                    get_identity_key_pair: Some(identity_pair),
                    get_local_registration_id: Some(identity_registration_id),
                    save_identity: Some(identity_save),
                    is_trusted_identity: Some(identity_trusted),
                    destroy_func: None,
                    user_data: user,
                },
            ))?;
        }
        Ok(ctx)
    }

    /// The state after a call. The C library does not run now.
    pub(crate) fn state(&self) -> &StoreState {
        // SAFETY: `self.state` is a live box that we own. The C library does not hold a
        // reference while we are in Rust code, because it is single-threaded and never runs
        // between our calls.
        unsafe { &*self.state }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        // SAFETY: each pointer is null or owned by us. The store context goes first because
        // it points into the global context. The state box is freed last, because the store
        // callbacks point to it.
        unsafe {
            if !self.store.is_null() {
                ffi::signal_protocol_store_context_destroy(self.store);
            }
            if !self.global.is_null() {
                ffi::signal_context_destroy(self.global);
            }
            drop(Box::from_raw(self.state));
        }
    }
}

/// Make a library-owned copy of `bytes` in a new buffer.
unsafe fn give(out: *mut *mut signal_buffer, bytes: &[u8]) -> c_int {
    // SAFETY: `out` is a valid out pointer. `signal_buffer_create` copies the bytes.
    unsafe {
        let buf = ffi::signal_buffer_create(bytes.as_ptr(), bytes.len());
        if buf.is_null() {
            return ffi::SG_ERR_NOMEM;
        }
        *out = buf;
    }
    0
}

/// The state behind the `user_data` pointer.
unsafe fn st<'a>(user: *mut c_void) -> &'a mut StoreState {
    // SAFETY: `user` is the pointer that `Ctx::new` registered. It is live for the whole
    // life of the store context.
    unsafe { &mut *user.cast::<StoreState>() }
}

// Session store: one session, in `session` before the call and in `new_session` after it.

unsafe extern "C" fn session_load(
    record: *mut *mut signal_buffer,
    _user_record: *mut *mut signal_buffer,
    _addr: *const ffi::signal_protocol_address,
    user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: see `st` and `give`.
        let state = unsafe { st(user) };
        // A session that the library made in this call is newer than the one we got.
        match state.new_session.as_ref().or(state.session.as_ref()) {
            Some(bytes) => match unsafe { give(record, bytes) } {
                0 => 1,
                err => err,
            },
            None => 0,
        }
    })
}

unsafe extern "C" fn session_sub_devices(
    _sessions: *mut *mut ffi::signal_int_list,
    _name: *const u8,
    _len: usize,
    _user: *mut c_void,
) -> c_int {
    // Only used for sessions with all devices of a name. We never ask for it.
    ffi::SG_ERR_INVAL
}

unsafe extern "C" fn session_store(
    _addr: *const ffi::signal_protocol_address,
    record: *mut u8,
    len: usize,
    _user_record: *mut u8,
    _user_len: usize,
    user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: `record` has `len` readable bytes. See `st`.
        let (state, bytes) = unsafe { (st(user), std::slice::from_raw_parts(record, len)) };
        if let Some(mut old) = state.new_session.replace(bytes.to_vec()) {
            old.zeroize();
        }
        0
    })
}

unsafe extern "C" fn session_contains(
    _addr: *const ffi::signal_protocol_address,
    user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: see `st`.
        let state = unsafe { st(user) };
        c_int::from(state.new_session.is_some() || state.session.is_some())
    })
}

unsafe extern "C" fn session_delete(
    _addr: *const ffi::signal_protocol_address,
    _user: *mut c_void,
) -> c_int {
    0
}

unsafe extern "C" fn session_delete_all(_name: *const u8, _len: usize, _user: *mut c_void) -> c_int {
    0
}

// Prekey stores.

unsafe extern "C" fn pre_key_load(out: *mut *mut signal_buffer, id: u32, user: *mut c_void) -> c_int {
    guard(|| {
        // SAFETY: see `st` and `give`.
        let state = unsafe { st(user) };
        match state.prekeys.get(&id) {
            Some(bytes) => unsafe { give(out, bytes) },
            None => ffi::SG_ERR_INVALID_KEY_ID,
        }
    })
}

unsafe extern "C" fn pre_key_store(_id: u32, _record: *mut u8, _len: usize, _user: *mut c_void) -> c_int {
    // The library never makes prekeys on its own. Our `Identity` owns them.
    0
}

unsafe extern "C" fn pre_key_contains(id: u32, user: *mut c_void) -> c_int {
    guard(|| {
        // SAFETY: see `st`.
        c_int::from(unsafe { st(user) }.prekeys.contains_key(&id))
    })
}

unsafe extern "C" fn pre_key_remove(id: u32, user: *mut c_void) -> c_int {
    guard(|| {
        // The caller removes the key from its `Identity` after a good decrypt.
        // SAFETY: see `st`.
        unsafe { st(user) }.used_prekey = Some(id);
        0
    })
}

unsafe extern "C" fn spk_load(out: *mut *mut signal_buffer, id: u32, user: *mut c_void) -> c_int {
    guard(|| {
        // SAFETY: see `st` and `give`.
        let state = unsafe { st(user) };
        match state.signed_prekeys.get(&id) {
            Some(bytes) => unsafe { give(out, bytes) },
            None => ffi::SG_ERR_INVALID_KEY_ID,
        }
    })
}

unsafe extern "C" fn spk_store(_id: u32, _record: *mut u8, _len: usize, _user: *mut c_void) -> c_int {
    0
}

unsafe extern "C" fn spk_contains(id: u32, user: *mut c_void) -> c_int {
    guard(|| {
        // SAFETY: see `st`.
        c_int::from(unsafe { st(user) }.signed_prekeys.contains_key(&id))
    })
}

unsafe extern "C" fn spk_remove(_id: u32, _user: *mut c_void) -> c_int {
    0
}

// Identity store. Trust is a decision of the caller, not of this layer: every key passes.

unsafe extern "C" fn identity_pair(
    public: *mut *mut signal_buffer,
    private: *mut *mut signal_buffer,
    user: *mut c_void,
) -> c_int {
    guard(|| {
        // SAFETY: see `st` and `give`.
        let state = unsafe { st(user) };
        if state.ik_public.len() != 32 || state.ik_private.len() != 32 {
            return ffi::SG_ERR_INVALID_KEY;
        }
        let rc = unsafe { give(public, &state.ik_public) };
        if rc != 0 {
            return rc;
        }
        let rc = unsafe { give(private, &state.ik_private) };
        if rc != 0 {
            // SAFETY: `*public` was just made by `give`, and the library has not seen it.
            unsafe { ffi::signal_buffer_free(*public) };
            return rc;
        }
        0
    })
}

unsafe extern "C" fn identity_registration_id(user: *mut c_void, out: *mut u32) -> c_int {
    guard(|| {
        // SAFETY: see `st`. `out` is a valid out pointer.
        unsafe { *out = st(user).device_id };
        0
    })
}

unsafe extern "C" fn identity_save(
    _addr: *const ffi::signal_protocol_address,
    _key: *mut u8,
    _len: usize,
    _user: *mut c_void,
) -> c_int {
    0
}

unsafe extern "C" fn identity_trusted(
    _addr: *const ffi::signal_protocol_address,
    _key: *mut u8,
    _len: usize,
    _user: *mut c_void,
) -> c_int {
    1
}
