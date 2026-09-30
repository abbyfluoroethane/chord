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

## A self-signed server (CORESESSION-15)

A self-signed certificate fails the normal validation first, so a plain pin cannot help. A pin made with `CertPin::trusting(fingerprint)` adds one exception: when the normal validation refuses the certificate, and its SHA-256 fingerprint is the stored one, it passes. The rules:

- The pin check sits in `PinnedVerifier` (`vendor/tokio-xmpp/src/connect/tls_common.rs`). It wraps the normal `WebPkiServerVerifier` and runs it first. It never turns the validation off, and there is no switch for it.
- Only a certificate that failed the normal validation and equals the stored fingerprint passes. The name of the certificate is not checked for it: the fingerprint says which certificate the user trusts.
- The TLS handshake signatures still go to the normal verifier, so the server must hold the key of the certificate.
- A plain pin (`CertPin::new`) and a learning pin never give the exception. The exception is per account, and only the user creates it, after a fingerprint was shown (see below).
- The verifier also gives the certificate to the pin when the validation refuses it. So after a refused login, `pin.observed()` holds its fingerprint and the UI can show it. The refused connection stops in the handshake, before SASL: no password goes out.

```
CHORD_CERT_PIN=learn chord-cli login        # refused; stderr: server certificate SHA-256: 52:D3:...
CHORD_CERT_PIN=trust:52:D3:... chord-cli login   # this one certificate passes
```

## The desktop app

- `src-tauri/src/certpin.rs` keeps the pins in `certpins.json` in the app config directory (one entry for each account: `fingerprint`, `trustUntrusted`). It is apart from `settings.json`, which the page owns and replaces as a whole. The page cannot write a fingerprint: `cert_trust` pins the fingerprint that the last login of the account saw.
- Every `login` builds a `CertPin`: the stored one, or a learning one. `cert_status(account)` gives `pinned`, `trustUntrusted`, `observed` and `problem` (`none`, `untrusted`, `changed`). `cert_clear(account)` removes the pin.
- The login screen shows the question when the login fails on the certificate. `untrusted`: the fingerprint and "Trust this certificate". `changed`: both fingerprints and "Trust the new certificate". The second choice gives no exception for a certificate that the system refuses: if the new one is self-signed too, the screen asks again.
- Settings > Account > Server certificate shows the fingerprint of the connection, pins it ("Pin this certificate") and clears the pin ("Stop pinning").

The in-band registration before login (`session::register`) does not use the pin.
