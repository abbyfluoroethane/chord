# End-to-end encryption and modern auth

## Summary

- Chord has no end-to-end encryption. A grep for OMEMO, OpenPGP, XEP-0373, XEP-0384, XEP-0420 and XEP-0454 finds no code, no docs and no UI. All messages are plaintext to the server and to its admin.
- Authentication is legacy SASL 1 only. It uses the SCRAM-SHA-256, SCRAM-SHA-1 and PLAIN mechanisms of tokio-xmpp 6.0.0. There is no SASL2, Bind2, FAST, ISR or SCRAM downgrade protection (XEP-0388, 0386, 0484, 0397, 0474).
- Transport security is sound. STARTTLS is required and certificate errors stop the login with no retry. There is no TOFU or pinning. The user cannot accept a self-signed server, which blocks small self-hosted servers.
- The password is stored in the system keychain on desktop. It is not in the log or in a file. The core keeps it in a plain `String` with no zeroize. In-band registration (XEP-0077), password change, invites (XEP-0401) and pre-authenticated subscription (XEP-0379) are all missing.
- The xmpp: link handler supports `?join` and `?pubsub;action=subscribe`. It does not support `?roster`, `?register`, `?message` or the `preauth` token.

## Findings

| ID | Severity | Type | Spec | Evidence | What to do |
|---|---|---|---|---|---|
| SECURITYAUTH-01 | High | Missing XEP | XEP-0384 (OMEMO 0.8+), XEP-0420 SCE | No match for "omemo" in chord-core, chord-desktop, chord-ffi (grep). README and docs never mention encryption. | Plan OMEMO 0.8 (urn:xmpp:omemo:2) with SCE. Chord needs a Rust crypto crate (Double Ratchet, X3DH). xmpp-rs has no OMEMO support, so this is new code. Effort: 3 to 5 weeks. See next steps. |
| SECURITYAUTH-02 | Medium | Missing XEP | XEP-0384 0.3 (legacy, eu.siacs.conversations.axolotl) | Not present. | Most clients still use the legacy version. Support it only if interop with old clients matters. Effort: add 1 to 2 weeks on top of 0.8. Unverified: which clients the target users run. |
| SECURITYAUTH-03 | Medium | Missing XEP | XEP-0373 and XEP-0374 (OpenPGP for XMPP) | Not present. | Low priority. Few clients use it. Skip unless users ask. Effort: 2 to 3 weeks. |
| SECURITYAUTH-04 | Medium | Missing XEP | XEP-0454 (OMEMO media sharing) | chord-core/src/features/upload.rs uploads plain files with XEP-0363. There is no aesgcm or encrypted URL. | Add after OMEMO. Encrypt the file with AES-256-GCM, upload, send an `aesgcm://` URL or the XEP-0454 form. Effort: 3 to 5 days. |
| SECURITYAUTH-05 | Medium | Missing XEP | XEP-0450 (ATM), TOFU and BTBV | Not present. No trust store table in chord-core/src/store/schema.rs. | Design trust with OMEMO. Use BTBV first (simple). Add ATM later. Effort: BTBV 3 to 5 days, ATM 1 to 2 weeks. |
| SECURITYAUTH-06 | Low | UX gap | XEP-0380 (Explicit Message Encryption) | No `encryption` element is read. An OMEMO message from a peer shows as a fallback text body or as nothing. | Show a clear "encrypted message that Chord cannot read" notice when the element is present. Effort: 0.5 day. |
| SECURITYAUTH-07 | Medium | Missing XEP | XEP-0388 SASL2 | chord-core/src/session/native.rs:493-500 calls `tokio_xmpp::client_login`, which is SASL 1 only. The file is a copy of `client_auth` (native.rs:459 "TOKIO-XMPP-COPY"). | tokio-xmpp 6.0.0 has no SASL2. Write a SASL2 client in the copy of the login function. Use the `sasl` crate for the mechanisms. Effort: 3 to 5 days. |
| SECURITYAUTH-08 | Medium | Missing XEP | XEP-0386 Bind2 | Bind is done by tokio-xmpp inside `StanzaStream`. No Bind2 code. | Do it with SASL2 (same handshake). Bind2 also gives the stream management and carbons enable in one round trip. Effort: 3 to 4 days after SASL2. |
| SECURITYAUTH-09 | Medium | Missing XEP | XEP-0484 FAST | Reconnect calls `login(server, &jid, &password)` with the full password (native.rs:143). | Needs SASL2. Store a FAST token in the keychain and use it on reconnect. The password then leaves memory after the first login. Effort: 3 to 4 days. |
| SECURITYAUTH-10 | Medium | Security | XEP-0474 (SCRAM downgrade protection), XEP-0440 | native.rs:534 picks the first local mechanism in the list `[SCRAM-SHA-256, SCRAM-SHA-1, PLAIN, ANONYMOUS]`. There is no check of a signed mechanism list. | Because STARTTLS is required, a network attacker cannot alter the list without breaking TLS. The risk is low. Add the SCRAM-SHA-256 `d=` downgrade data from XEP-0474 with SASL2. Effort: 1 to 2 days. |
| SECURITYAUTH-11 | Low | Security | XEP-0440 (SASL channel binding types) | native.rs:490-495 turns channel binding off when no `-PLUS` mechanism is offered. Binding data is `tls-unique` or `tls-exporter` from the connector. | This is correct. Note: tokio-xmpp names SCRAM only as `-PLUS` when binding data exists (native.rs:463-466). A server that offers SCRAM and SCRAM-PLUS works. Add a test. Unverified: the behaviour with a server that offers only `SCRAM-SHA-256-PLUS` and `tls-unique` on TLS 1.3. |
| SECURITYAUTH-12 | Medium | Missing XEP | XEP-0397 (ISR) | Stream management resumes (XEP-0198) through tokio-xmpp (native.rs:363-401). No ISR. | Needs SASL2 and Bind2. Wait for server support. Effort: 2 to 3 days after SASL2. |
| SECURITYAUTH-13 | Medium | UX gap | RFC 7590 / RFC 6120 13.7 | native.rs:551 maps a certificate error to `TlsInvalid`. adapt.ts:330 shows only "certificate is not valid". There is no "trust this certificate" path. Stopping is right for the default. | Add an opt-in per-account pin (TOFU): show the SHA-256 fingerprint, store it, and compare it on later connects. Needs a custom rustls verifier in the connector. Effort: 3 to 5 days. |
| SECURITYAUTH-14 | Low | Security | RFC 6120 13.7 | The `dev-insecure` feature allows plain TCP (chord-core/Cargo.toml:13). chord-ffi/Cargo.toml:15 says CI checks it stays off. | Good. Confirm that chord-desktop/src-tauri/Cargo.toml never enables it (grep found no match). No action. |
| SECURITYAUTH-15 | Low | Security | n/a | `SessionConfig.password` is a `String` (session/mod.rs:35). It is cloned for each reconnect (native.rs:142). There is no zeroize. Debug output hides it (mod.rs:57). | Use `secrecy::SecretString` or `zeroize`. This helps little on a desktop. Effort: 0.5 day. |
| SECURITYAUTH-16 | Low | Security | n/a | commands.rs:168-177: the password is saved to the keychain after a good login only when `remember` is set. `forget_password` exists (commands.rs:196). | Correct order. Note: on logout or on a failed login the saved password stays. Add a "sign out and forget" option. Check that the UI offers it. Unverified. |
| SECURITYAUTH-17 | Medium | Security | n/a | Android: chord-ffi/src/client.rs:341 takes the password as a `String`. The FFI has no keystore code. | Unverified: the Android app may use the Android Keystore. The Kotlin side is out of scope. Check it. |
| SECURITYAUTH-18 | Medium | Missing XEP | XEP-0077 (in-band registration) | No match for "register" in chord-core other than the deep-link scheme (chord-desktop/src-tauri/src/links.rs:36). | Add a `register` flow before login: read the form (`jabber:iq:register`, or the XEP-0004 data form), submit it, then log in. Needs an unauthenticated stream. tokio-xmpp 6.0.0 has no such client, so open the stream in the copy of the login function (native.rs:459). Effort: 3 to 5 days. Many servers disable it, so add XEP-0401 and web registration links too. |
| SECURITYAUTH-19 | Medium | Missing feature | XEP-0077 section 3.3 (change password) | No `jabber:iq:register` with `<password/>` on a bound stream. | Add `change_password`. Send the IQ set, and update the keychain on success (commands.rs, keychain.rs:33). Effort: 1 day. |
| SECURITYAUTH-20 | Low | Missing feature | XEP-0077 section 3.2 (cancel registration) | Not present. | Optional. Add "delete account" with a strong confirmation. Effort: 0.5 day. |
| SECURITYAUTH-21 | Medium | Missing XEP | XEP-0401 (easy onboarding) and XEP-0445 | The xmpp: link code (chord-desktop/src/lib/ui/xmpplinks.svelte.ts and the test file) knows only `?join` and `?pubsub;action=subscribe`. It knows no `?register;preauth=`. | Parse `xmpp:domain?register;preauth=TOKEN` and `xmpp:jid?roster;preauth=TOKEN`. Depends on SECURITYAUTH-18. Effort: 2 days. |
| SECURITYAUTH-22 | Medium | Missing XEP | XEP-0379 (pre-authenticated roster subscription) | chord-core/src/features/roster.rs supports RFC 6121 pre-approval (roster.rs:44, 373) but no `<preauth xmlns='urn:xmpp:pars:0'/>` in a subscribe presence. | Add a `preauth` token to `add_contact`. Parse `?roster;preauth=` links. Send the element in the subscription request. Effort: 1 to 2 days. |
| SECURITYAUTH-23 | Info | Missing feature | XEP-0357 / n/a | Sasl condition `credentials-expired` shows "password expired" (session/mod.rs:102). No in-app path to change it. | Link this to SECURITYAUTH-19. |
| SECURITYAUTH-24 | Low | UX gap | n/a | adapt.ts:300-303 says "Wrong address or password" for every SASL failure. The condition is lost. | Show `account-disabled` and `credentials-expired` in plain words. Effort: 0.5 day. |
| SECURITYAUTH-25 | Low | UX gap | XEP-0198 | The saved password is used for auto login (session.svelte.ts:96). There is no lock or session timeout. | Optional. Add an app lock later. |

## Already done well

- TLS: STARTTLS is required, SRV lookup is used, and a bad certificate never retries (session/native.rs:96-101, 551, session/mod.rs:153).
- Channel binding: the login code passes `tls-unique` or `tls-exporter` to SASL and turns it off when the server offers no PLUS mechanism (native.rs:461-500).
- SCRAM-SHA-256 is preferred over SCRAM-SHA-1 and PLAIN (native.rs:530-537).
- Permanent SASL failures stop the reconnect loop, so the app does not lock the account with retries (native.rs:213-275, `sasl_retry`).
- Stream management resume (XEP-0198) works through `StanzaStream` (native.rs:363-401).
- Keychain: macOS, Windows and Linux backends are set, and error text never holds the secret (chord-desktop/src-tauri/src/keychain.rs:1-53, Cargo.toml:57-63).
- The Debug output of `SessionConfig` hides the password (session/mod.rs:54-58).
- `dev-insecure` is off by default. The FFI crate must never enable it (chord-ffi/Cargo.toml:15).
- RFC 6121 section 3.4 subscription pre-approval, with a check of the `urn:xmpp:features:pre-approval` stream feature (roster.rs:44, 373).
- The xmpp: links always ask the user before any action (links.rs:10, deeplink.ts).
- The CLI takes the password only from the environment, not from an argument (chord-cli/src/main.rs:42).

## Recommended next steps

1. Decide the E2EE scope. Choose OMEMO 0.8 with SCE as the main target. Add legacy 0.3 only if interop needs it. Effort: 0.5 day for the decision.
2. Add a visible "not encrypted" state and the XEP-0380 notice (SECURITYAUTH-06). Effort: 0.5 day.
3. Add change password (SECURITYAUTH-19) and a better SASL error text (SECURITYAUTH-24). Effort: 1 to 1.5 days.
4. Add XEP-0379 preauth in roster subscribe and xmpp: links (SECURITYAUTH-22). Effort: 1 to 2 days.
5. Add in-band registration and XEP-0401 links (SECURITYAUTH-18, 21). Effort: 5 to 7 days.
6. Add certificate pinning with TOFU (SECURITYAUTH-13). Effort: 3 to 5 days.
7. Write SASL2, Bind2 and FAST in the login copy, with SCRAM downgrade data (SECURITYAUTH-07 to 10, 12). Effort: 8 to 12 days.
8. Implement OMEMO 0.8 with BTBV trust and XEP-0454 media (SECURITYAUTH-01, 04, 05). Effort: 4 to 6 weeks in total. It is the largest item. It needs a store migration for keys, sessions and trust, PEP publish of device lists and bundles, and MUC support.
