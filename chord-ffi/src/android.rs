//! Android set-up that UniFFI cannot do: a JNI entry point that gives the JVM and the
//! application `Context` to `rustls-platform-verifier`.
//!
//! `reqwest` (the HTTP upload) verifies certificates through `rustls-platform-verifier`.
//! On Android that crate calls the system verifier through the JVM. It panics when no
//! one called `init_with_env` first. Call `NativeInit.init(context)` once in
//! `Application.onCreate`, before any login or upload.

use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JObject};
use jni::{EnvUnowned, jni_mangle};

/// `space.foid.chord.NativeInit.init(context: Context)`. It is safe to call more than once.
#[jni_mangle("space.foid.chord.NativeInit")]
pub fn init<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
) {
    install_panic_log();
    unowned_env
        .with_env(|env| {
            init_ndk_context(env, &context)?;
            rustls_platform_verifier::android::init_with_env(env, context)
        })
        .resolve::<ThrowRuntimeExAndDefault>();
}

/// The DNS resolver of tokio-xmpp (hickory) asks Android for the DNS servers through
/// `ndk-context`, because Android has no `/etc/resolv.conf`. It panics when no one set
/// the context. Set it once. The global reference stays for the life of the process.
fn init_ndk_context(env: &mut jni::Env, context: &JObject) -> jni::errors::Result<()> {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let mut result = Ok(());
    ONCE.call_once(|| {
        result = (|| {
            let vm = env.get_java_vm()?;
            let global = env.new_global_ref(context)?;
            // SAFETY: the pointers stay valid for the life of the process: the VM is
            // global and the leaked global reference is never deleted. `Once` makes this
            // the only call, as `initialize_android_context` requires.
            unsafe {
                ndk_context::initialize_android_context(
                    vm.get_raw().cast(),
                    global.as_raw().cast(),
                );
            }
            std::mem::forget(global);
            Ok(())
        })();
    });
    result
}

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        priority: std::ffi::c_int,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

/// Android drops stderr, so a panic leaves no trace. Write it to logcat (tag `chord`, ERROR).
fn install_panic_log() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let text =
                std::ffi::CString::new(info.to_string().replace('\0', " ")).unwrap_or_default();
            // SAFETY: both pointers are valid, NUL-terminated C strings for the call.
            unsafe { __android_log_write(6, c"chord".as_ptr(), text.as_ptr()) };
            previous(info);
        }));
    });
}

/// `space.foid.chord.NativeInit.probeHttps(url: String): String`. A diagnostic for the
/// debug build. It sends one HEAD request with the same `reqwest` set-up as the upload
/// (rustls, platform verifier). It returns `ok <status>` or `error <text>`. A certificate
/// that the system does not trust gives an error. Call it on a background thread: it blocks.
#[jni_mangle("space.foid.chord.NativeInit")]
pub fn probe_https<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    url: jni::objects::JString<'caller>,
) -> jni::sys::jstring {
    unowned_env
        .with_env(
            |env| -> jni::errors::Result<jni::objects::JString<'caller>> {
                let url = url.try_to_string(env)?;
                let answer = probe(&url);
                env.new_string(answer)
            },
        )
        .resolve::<ThrowRuntimeExAndDefault>()
        .into_raw()
}

fn probe(url: &str) -> String {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return "error cannot start a runtime".into();
    };
    runtime.block_on(async {
        let client = match reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(client) => client,
            Err(e) => return format!("error cannot build the client: {e:?}"),
        };
        match client.head(url).send().await {
            Ok(response) => format!("ok {}", response.status()),
            Err(e) => format!("error {e:?}"),
        }
    })
}
