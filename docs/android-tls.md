# TLS on Android

This page says how the native core checks certificates on Android. It also says what was broken and how to test it.

## What the app must do

Call `NativeInit.init(applicationContext)` once in `Application.onCreate`. Do it before the first login or upload.

- `NativeInit` is in `chord-android/app/src/main/kotlin/space/foid/chord/NativeInit.kt`.
- The Rust part is `chord-ffi/src/android.rs`. It exists only on Android.
- You can call `init` more than once. Only the first call has an effect.

## What `init` does

1. It gives the JVM and the application context to `rustls-platform-verifier`. This crate asks the Android system to verify a certificate. It panics when it has no JVM.
2. It gives the same context to `ndk-context`. The DNS resolver of `tokio-xmpp` (`hickory-resolver`) needs it. Android has no `/etc/resolv.conf`, so the resolver asks `ConnectivityManager` for the DNS servers. It panics when it has no context.
3. It writes Rust panics to logcat (tag `chord`). Android drops stderr, so a panic left no trace before.

## What was broken

| Path | Before | Cause |
|---|---|---|
| XMPP login | Panic, then `ActorGone` | The DNS resolver had no Android context. |
| XMPP login, after the DNS fix | `TlsInvalid: UnknownIssuer` for every server | `rustls-native-certs` finds no root certificate on Android. It looks in `/etc/ssl/certs`. Android keeps the roots in `/apex/com.android.conscrypt/cacerts` and `/system/etc/security/cacerts`. |
| HTTP upload (`reqwest`) | Panic on first use | `rustls-platform-verifier` had no JVM, and the app had no Kotlin part of the crate. |

## What changed

- `vendor/tokio-xmpp/src/connect/tls_common.rs`: a new function `server_verifier`.
  - On Android it returns the system verifier (`rustls-platform-verifier`).
  - On every other system it builds the same `WebPkiServerVerifier` as before.
  - `PinnedVerifier` (the certificate pin) now wraps any verifier, not only `WebPkiServerVerifier`. The pin rules did not change.
- `vendor/tokio-xmpp/Cargo.toml`: `rustls-platform-verifier` as a dependency on Android only.
- `chord-ffi/Cargo.toml` and `chord-ffi/src/android.rs`: the JNI entry points. The dependencies are for Android only.
- Gradle: the Kotlin part of `rustls-platform-verifier` (`org.rustls:rustls-platform-verifier`).
  - `settings.gradle.kts` adds the Maven repository of the crate. Only the group `org.rustls` can come from it.
  - `app/build.gradle.kts` reads the version from `Cargo.lock`, so the Rust crate and the Kotlin part always match.

Nothing turns off verification. The system verifier checks the chain, the name and the dates. It also uses the user-installed CAs and the distrust list of the system. The features `dev-insecure` and `insecure-tcp` stay off (`dev/check-features.sh` passes).

## Debug check

The debug build has `TlsCheckActivity`. It logs the result under the tag `ChordTls`.

```
adb shell am start -n space.foid.chord/.TlsCheckActivity \
  --es jid nobody@chat.foid.space --es password dummy \
  [--es server xmpps://chat.foid.space:5223] \
  [--es urls https://example.com/,https://expired.badssl.com/]
adb logcat -d -s ChordTls chord
```

- A good certificate and a wrong password give `AuthFailed`. That proves the certificate passed.
- A bad certificate gives `TlsInvalid`.
- Each URL gets one HEAD request through `reqwest` with the same set-up as the upload. `NativeInit.probeHttps` does it.

The release build has no `TlsCheckActivity`. `probeHttps` stays in the library. It is small and it only sends a HEAD request to the URL that the caller gives.

## Test results (emulator, API 36, x86_64)

XMPP, with a dummy password:

| Server | Result |
|---|---|
| `chat.foid.space` (SRV) | `AuthFailed` (certificate good) |
| `chat.foid.space` (`starttls://...:5222`) | `AuthFailed` |
| `chat.foid.space` (`xmpps://...:5223`) | `AuthFailed` |
| `conversations.im` (SRV) | `AuthFailed` |
| JID `example.org`, server `xmpps://chat.foid.space:5223` | `TlsInvalid`: certificate not valid for name `example.org` |
| Self-signed server (`openssl s_server`) at `xmpps://10.0.2.2:5333`, right name | `TlsInvalid: UnknownIssuer` |

HTTPS through `reqwest`:

| URL | Result |
|---|---|
| `https://example.com/` | `ok 200` |
| `https://wrong.host.badssl.com/` | refused: `NotValidForNameContext` |
| `https://expired.badssl.com/` | refused: `Expired` |
| `https://self-signed.badssl.com/` | refused: `UnknownIssuer` |
| `https://untrusted-root.badssl.com/` | refused: `UnknownIssuer` |

No login with a real password ran: no credentials were available in the worktree. A good login only differs from `AuthFailed` after the TLS step, so the TLS result holds.

## Not covered

- The self-signed pin exception (`CertPin::trusting`) was not run on a device. The code path is the same as on desktop. Only the inner verifier is different.
- A server that the user trusts through a user-installed CA passes on Android. That is the system rule.
- The arm64-v8a library was not built or run. It uses the same code.
