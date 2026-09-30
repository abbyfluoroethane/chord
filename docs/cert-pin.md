# Certificate pin

A pin is an opt-in check on top of the normal TLS checks (SECURITYAUTH-13, RFC 7590). The default stays as it is: the certificate must chain to a trusted root and name the JID domain. A pin adds one more rule: the SHA-256 fingerprint of the end-entity certificate must match the stored one.

## How it works

- `chord-core/src/session/cert_pin.rs` has `CertPin`. `SessionConfig::with_pin(pin)` turns it on.
- The check runs in the TLS handshake, before SASL. A wrong certificate stops the connection before the password goes out. The error is `ConnectError::TlsInvalid` and its text names both fingerprints.
- A pin with no stored fingerprint (`CertPin::new(None)`) only learns. After the connection, `pin.observed()` gives the fingerprint. This is trust on first use: show it to the user, and store it only when the user agrees.
- A reconnect uses the same pin. A server that changes its certificate (for example a renewal) stops every connection until the user accepts the new fingerprint.
- The pin works for STARTTLS and for direct TLS, in every SRV mode. `Connector` in `chord-core/src/session/connector.rs` passes the check to each target.
- The hook is a patch in `vendor/tokio-xmpp` (`CertCheck` and `establish_tls_connection_with` in `connect/tls_common.rs`, `starttls_checked` in `connect/starttls.rs`). Drop it when upstream has a hook.

## CLI

```
CHORD_CERT_PIN=learn chord-cli login       # prints: server certificate SHA-256: 35:88:...
CHORD_CERT_PIN=35:88:... chord-cli login   # connects only if the certificate matches
```

The fingerprint can have colons or not, in any case. A mismatch gives exit code 3.

## Still to do: the desktop settings UI

The core and the CLI are done. The app does not show the pin yet. A plan for it:

1. A Tauri command `cert_fingerprint(account)` that returns `pin.observed()` after a login, and a `login` argument `pin: Option<String>`. The command builds `CertPin::new(pin)` and keeps the clone in the client state.
2. Store the pin for each account in `settings.json` (it is not a secret). Keep it apart from the keychain.
3. Settings > Account: a "Pin the server certificate" switch. On, it shows the SHA-256 fingerprint of the current connection and stores it after a click on "Trust this certificate".
4. On a mismatch, the login screen shows both fingerprints and the choices "Cancel" and "Trust the new certificate". Only the second one replaces the stored fingerprint and tries again.
