# OMEMO plan

This plan covers issue #88 (SECURITYAUTH-01) and the encryption epic #194. It also covers SECURITYAUTH-04 (media), 05 (trust), and 06 (notice for messages that we cannot read). We do not write crypto code yet. This document says what to build, with which parts, and how to prove that it works.

We read the XEPs and the crate pages on 2026-09-30. A line that says "unverified" is a guess or a fact that we could not check.

## Scope

- OMEMO 2: `urn:xmpp:omemo:2` (XEP-0384 version 0.9.1, experimental, dated 2026-04-06).
- Stanza Content Encryption, XEP-0420 (version 0.5.0, experimental), with the profile that XEP-0384 defines.
- Encrypted file sharing (XEP-0454).
- Trust: blind trust before verification first, then automatic trust management (XEP-0450, version 0.4.0, experimental).
- 1:1 chats first. Then rooms that are non-anonymous and members-only.
- Not in scope: legacy OMEMO (`eu.siacs.conversations.axolotl`, SECURITYAUTH-02). Old clients need it, and it needs a second crypto profile (AES-128-GCM, libsignal wire format). Decide after the interop check in phase 0.

The audit said "0.8". The XEP is at 0.9.1 now. We build to the text of 0.9.1 and check that the wire format did not change between the two. (Unverified: the change log between 0.8 and 0.9.1.)

## What the XEP says

From XEP-0384 0.9.1:

- Primitives: Curve25519 and Ed25519, X3DH, Double Ratchet, SHA-256, HKDF-SHA-256, AES-256-CBC with HMAC-SHA-256.
- The identity key goes on the wire in Ed25519 form, and only valid EdDSA signatures go on the wire. An implementation may use Curve25519 inside. The XEP says OMEMO does not need XEdDSA. It also says libsignal does not use XEdDSA by default and needs changes for OMEMO.
- Device list: PEP node `urn:xmpp:omemo:2:devices`, one item with the id `current`. It holds `<device id='...'/>` elements. The id is a positive 32-bit integer. A device can carry a `label` and a `labelsig`.
- Bundles: PEP node `urn:xmpp:omemo:2:bundles`, one item for each device, with the device id as item id. The access model must be `open`, and the node must keep many items (`max`). A bundle has `<spk id>`, `<spks>` (signature), `<ik>`, and `<prekeys>` with `<pk id>` elements. The XEP recommends about 100 prekeys and at least 25.
- Message: `<encrypted xmlns='urn:xmpp:omemo:2'>` with `<header sid>`, one `<keys jid>` element for each recipient account, one `<key rid kex>` element for each recipient device, and an optional `<payload>`.
- `kex='true'` means that the key element is a key exchange message (X3DH parameters plus the first ratchet message).
- Empty OMEMO messages have no payload. They carry 32 zero bytes through the ratchet. They build sessions.
- Heartbeat: when a client gets the first message of a ratchet key with a counter of 53 or higher, it must send an empty message.
- The plaintext is an XEP-0420 envelope. The OMEMO profile requires `<rpad/>`, allows `<time/>`, recommends `<from/>`, and requires `<to/>` in group chats.
- A client must not use a new session to send data before it trusts the key. Empty messages that only move key material are the exception.
- Fingerprint: the public identity key in its byte-encoded Curve25519 form. (Note the difference between the wire form and the fingerprint form. This is an easy bug.)
- Rooms must be non-anonymous, should be members-only, and every message is encrypted for each real JID.

XEP-0454 (version 0.1.0, dated 2021-01-26): the page that we read says the status is Historical. (Unverified: check the XEP list, because this looks odd next to the OMEMO 2 spec.) It encrypts the file with AES-256-GCM (32-byte key, 12-byte IV, tag at the end). The sender uploads the ciphertext with XEP-0363. The message holds an `aesgcm://host/path#IV+KEY` URL, and the OMEMO layer encrypts that message. A client must never make an `aesgcm://` URL clickable, because the key sits in the fragment.

## Crate choice

We need a Double Ratchet and X3DH that match XEP-0384 byte for byte. Here is what we found.

| Candidate | Version and date | Fit | Verdict |
|---|---|---|---|
| Signal `libsignal` (Rust, `signalapp/libsignal`) | Git only. Not on crates.io as far as we could see. | The README says use outside Signal is unsupported and all APIs can change without notice. The license is AGPL-3.0. Chord is MIT, so a link would force AGPL on the whole app. The XEP also says it needs changes to work with OMEMO. | No. |
| `libsignal-protocol` (crates.io) | 0.1.0, 2019, binding to the C library | Old, and legacy OMEMO only. | No. |
| `vodozemac` | 0.11.0, 2026-09-11, Apache-2.0 | It implements Olm and Megolm for Matrix. Olm is a different protocol with its own message format. It does not produce OMEMO messages. The README does not say it exposes a generic Double Ratchet. | No. It is a good code base to read, but it is wire-incompatible. |
| `dziber-omemo` | 0.0.3, 2026-06-14, 92 downloads, BSD-2-Clause | The description says legacy v0 (libsignal-compatible). Not OMEMO 2. Very young. | No, unless a check shows OMEMO 2 support. |
| `double-ratchet-2`, `x3dh-ke`, `xxxdh`, `ratchetx2` and similar | Small, low downloads, last release 2019 to 2025 | General Signal-style code. We did not check the KDF labels, the message format, or the skipped key limits. None is audited as far as we know. | No. Do not build a security feature on them. |
| libomemo-c (C library, fork of libsignal-protocol-c) | Not checked | (Unverified) Dino may use it. The license may be GPL-3.0, which we could not link into an MIT app. | Check the license. Probably no. |

Recommendation: write a small crate, `chord-omemo`, from RustCrypto and dalek parts. It has no XMPP code. It offers the pure protocol: key generation, bundle build, X3DH, Double Ratchet, the OMEMO message and key exchange formats, the key derivation for the payload, and the SCE envelope. The size is about 1500 lines plus tests. Parts:

| Job | Crate | Version and date |
|---|---|---|
| X25519 | `x25519-dalek` | 3.0.0, 2026-07-06 |
| Ed25519 | `ed25519-dalek` | 3.0.0, 2026-07-06 |
| HKDF, HMAC, SHA-256 | `hkdf`, `hmac`, `sha2` (RustCrypto) | Not checked. Pin at spike time. |
| AES-256-CBC | `aes`, `cbc` (RustCrypto) | Not checked. |
| AES-256-GCM for XEP-0454 | `aes-gcm` (RustCrypto) | Not checked. |
| Protobuf for the message formats | `prost` | Not checked. |
| XEdDSA, only if we keep the identity key in Curve25519 form | `xeddsa` | 1.1.0, 2026-05-04, 73,503 downloads |

We prefer the Ed25519 identity key inside. The XEP allows it, and it avoids XEdDSA. The Curve25519 form for the DH step and for the fingerprint comes from a conversion of the Ed25519 key. Check the conversion functions of `ed25519-dalek` and `curve25519-dalek` in the spike. Do not write curve arithmetic ourselves.

Why not a library: no maintained Rust library for OMEMO 2 exists, as far as our search of crates.io found (queries: omemo, x3dh, double ratchet, libsignal). The reference stack in Python (python-omemo, python-x3dh, python-doubleratchet, used by Gajim) is not a Rust option. (Unverified: their names and test vectors.)

Risk: we write security code. Mitigations:

1. Write it as a separate crate with no I/O, so we can fuzz it and review it alone.
2. Test with known vectors. Take the vectors from the XEP text if there are any. Otherwise produce vectors with python-omemo and check them in. (Unverified: whether the XEP or python-omemo has published vectors.)
3. Interop tests with real clients before release (see the test plan).
4. Ask for an outside review before we call it stable. Say in the UI and the README that the feature is new until then.

## Architecture

```
chord-omemo (new crate, pure)          keys, X3DH, Double Ratchet, formats, SCE
chord-core/src/features/omemo.rs       PEP nodes, device cache, session store use, hooks
chord-core/src/store                   new tables, migration
chord-core/src/features/chat.rs        hook: encrypt before send, decrypt after receive
chord-ffi, chord-desktop               settings, indicators, device and trust UI
```

### Device identity and PEP

- On the first start with OMEMO on, make a random device id (positive 32-bit integer that is not in our own list), an identity key pair, a signed prekey (with its signature), and 100 one-time prekeys. Store them.
- Publish the bundle to `urn:xmpp:omemo:2:bundles` with our device id as item id. Create the node with `access_model=open` and `max_items=max`. ejabberd on chat.foid.space has `mod_pubsub` with PEP and `max_items_node: 1000`. (The server config has no explicit `max` support test yet. Check in phase 2.)
- Add our id to the item `current` in `urn:xmpp:omemo:2:devices`. Read the item first. Never drop the ids of other devices. Republish only when our id is missing.
- Subscribe with `urn:xmpp:omemo:2:devices+notify` in the caps (the disco module already supports `+notify` features). Update the cache when an event arrives.
- Replace prekeys: when fewer than 25 remain, add new ones and republish. Rotate the signed prekey on a schedule (the interval is a choice for phase 2, for example every 7 days, and keep the old one for a grace period).
- Fetch the device list and the bundles of a contact only when needed: before the first message, and on a device list event. Cache them.

### Sending

1. Check the setting for the conversation. If it is on, encrypt. If a recipient has no device list, fail with a clear error. Never fall back to plaintext without asking the user.
2. Build the SCE envelope: `<content>` with the body (and the other payload elements), `<rpad/>`, `<time/>`, `<from/>`. Add `<to/>` for a room.
3. Encrypt the envelope with a random 32-byte key, HKDF to a cipher key, a MAC key and an IV, AES-256-CBC, and HMAC (the exact labels come from the XEP). Encrypt the key material for each trusted recipient device, and for our own other devices, with the Double Ratchet session.
4. For a device with no session, run X3DH with its bundle and mark the key element `kex='true'`.
5. Send `<encrypted xmlns='urn:xmpp:omemo:2'>` with a fallback body, an XEP-0380 `<encryption>` element, and a `<store/>` hint.
6. Store the plaintext in our own database, as for a plain message, with a column that says the message was encrypted.

### Receiving

1. Find our device id in the `<keys>` element for our JID. If it is missing, show "not encrypted for this device".
2. Run the ratchet step. On a `kex` message, run the receiving side of X3DH with our prekeys, and delete the used one-time prekey.
3. Decrypt the payload. Check the MAC. Parse the envelope. Check `<from>`, `<to>` and `<time>` against the stanza. Drop anything with a wrong value.
4. Send an empty message when the heartbeat rule needs it, or when we built a session from a key exchange.
5. Handle carbons and MAM. A copy of our own message from another device arrives through a carbon. It is encrypted for our device too, so we can read it. An archive message that was encrypted before this device existed cannot be read. Show "cannot decrypt" and keep the row. Do not retry forever.
6. Keep skipped message keys, with a hard limit (the XEP may set one, check in phase 1).

### Store schema

A new migration. All tables have `account_id`. Table names below are proposals.

```sql
omemo_identity(account_id PK, device_id, ik_public, ik_secret_ref, created_at)
omemo_signed_prekeys(account_id, id, public, secret_ref, signature, created_at, replaced_at)
omemo_prekeys(account_id, id, public, secret_ref, consumed_at)
omemo_devices(account_id, jid, device_id, ik_public, label, active, first_seen, last_seen,
              PRIMARY KEY(account_id, jid, device_id))
omemo_sessions(account_id, jid, device_id, state_blob, needs_kex_ack, updated_at, PRIMARY KEY(...))
omemo_skipped_keys(account_id, jid, device_id, ratchet_key, counter, message_key, created_at)
omemo_trust(account_id, jid, ik_fingerprint, level, source, decided_at,
            PRIMARY KEY(account_id, jid, ik_fingerprint))    -- level: undecided, blind, verified, distrusted
conversation_settings(... encryption)                        -- off, omemo
messages.encryption                                          -- NULL or 'omemo:2', plus a decrypt error column
```

Secrets at rest: the desktop app has a keychain wrapper today (`keychain.rs` in `chord-desktop`, macOS, Windows, and Linux backends). The store must not hold raw private keys in the SQLite file. Two options. (A) Keep the secret column but encrypt it with a data key from the keychain. (B) Use an encrypted SQLite build. We recommend A, because it needs no change of the SQLite build. The `*_secret_ref` columns above hold the ciphertext. Android uses the Android Keystore for the data key (SECURITYAUTH-17 is open). Decide in phase 3. The core needs an interface, for example `SecretBox { seal, open }`, that the desktop and Android layers provide. A wipe of the keychain must lose the keys and start a new device id, with a clear warning.

### Trust

Start with blind trust before verification. A new key of a contact is trusted at once, until the user verifies one key of that contact. After that, a new key stays undecided until the user decides. Show the fingerprint and let the user compare it, with a QR code or a text string. (Unverified: the origin of the name BTBV. XEP-0450 does not mention it.) Own devices get the same rules.

Add automatic trust management (XEP-0450) later. It sends encrypted trust messages between our own devices and to contacts. It needs the same crypto and a second storage. Estimated one to two weeks, as in the audit.

A key that is distrusted or undecided must not receive message keys. Empty messages are the only exception.

### UI

- A lock toggle in the composer of each chat. Off by default at first. When on, show a lock icon in the header and on each message.
- Clear states: encrypted, not encrypted (with a warning in an encrypted chat), and "cannot decrypt".
- A device list for our account, with the fingerprint of each device and a button to remove the others (this republishes the device list without the id).
- A trust dialog for a contact: fingerprints, verify, distrust.
- A notice for an encrypted message from a client that we cannot read (SECURITYAUTH-06). Read the XEP-0380 element or the `urn:xmpp:omemo:2` namespace. Do this first because it takes half a day and helps today.

### Rooms

Needs a non-anonymous room, and the real JID of each occupant from the member lists. Chord's spaces are rooms, so OMEMO in a space channel only works when the room is non-anonymous. Show the requirement in the UI. Encrypt for every member device, and add the `<to/>` element. Watch out for large rooms. The key count is members times devices for each message. Set a size limit, for example 50 members, and refuse above it. (The limit is a choice. Set it after the first measurement.)

### Media (XEP-0454)

Encrypt the file with AES-256-GCM as the XEP says. Upload the ciphertext through the existing `upload.rs` flow. Send the `aesgcm://` URL in an OMEMO message. On receive, download the ciphertext, decrypt it, and cache the file. Never open an `aesgcm://` URL in a browser. Check the current status of the XEP before we start, because the page said Historical.

## Phases and estimates

One developer, in weeks.

| Phase | Work | Weeks |
|---|---|---|
| 0 | Decisions. Check which of Conversations, Dino, Gajim, Monal, and Siskin support OMEMO 2 today. Read the XEP again for KDF labels, skipped key limits, and the wire format change since 0.8. Check the `libomemo-c` license. Test the PEP node config on chat.foid.space. | 0.5 |
| 1 | `chord-omemo`: keys, X3DH, Double Ratchet, formats, SCE, test vectors, fuzzing of the parsers. | 2 |
| 2 | `features/omemo.rs`: PEP publish and fetch, device cache, prekey replacement, events. | 1 |
| 3 | Store migration, the `SecretBox` interface, keychain wiring on desktop. | 0.7 |
| 4 | Send and receive path for 1:1: hooks in `chat.rs`, carbons, MAM, fallback, empty messages, heartbeat. | 1.2 |
| 5 | Trust with BTBV, fingerprints, device removal. | 0.8 |
| 6 | UI: toggle, icons, device and trust dialogs, error states. | 1 |
| 7 | Rooms. | 0.8 |
| 8 | Media (XEP-0454). | 0.7 |
| 9 | Interop and bug fixing. | 1 |
| 10 | Outside review of `chord-omemo`. Not on our schedule. Book early. | n/a |
| Later | XEP-0450 ATM: 1 to 2 weeks. Legacy OMEMO: 1 to 2 weeks. | |

Total: about 10 weeks for phases 0 to 9. The audit said 3 to 5 weeks for the crypto and 4 to 6 weeks in total. We think 10 weeks is more honest, because the audit did not count the store, the keychain, the UI, rooms, and interop. Phases 1 and 2 can run in parallel for two people.

## Test plan

Unit tests:

- Known-answer tests for X3DH and the ratchet, with vectors.
- Round trips: encrypt then decrypt, with out-of-order messages, skipped keys, and the skip limit.
- Tampering: wrong MAC, wrong `sid`, replayed messages, a `kex` with a used prekey, a bad signature on the signed prekey. Each must fail.
- The SCE checks: wrong `from`, wrong `to`, old `time`.
- Parser fuzzing for `<encrypted/>`, the bundle, and the protobuf messages.
- Store: migration up, and a session that survives a restart.

Live tests on chat.foid.space with two test accounts and the CLI:

1. Publish the device list and the bundle. Read them back from a second account. Check `access_model=open` for the bundles.
2. Two Chord devices for one account and one contact device. Send in both directions. Check that our own second device reads the sent message through a carbon.
3. Restart between messages. The session must persist.
4. Drop a device from the list. New messages must not use it.
5. Fetch the archive (MAM) on a fresh device. It must show "cannot decrypt" for old messages and not crash.

Interop, by hand, with a checklist for each client (Conversations, Gajim, Dino, Monal, Siskin), after phase 0 says which ones speak OMEMO 2:

- First message from each side (key exchange).
- Reply, then ten messages both ways.
- A message while the peer is offline.
- A new device on either side.
- A photo (XEP-0454).
- A room (phase 7).
- Trust: a verified key, then a new key.

Keep a log of failures with the stanzas. A failure at the wire level is a bug in the KDF label or in the message format. Check that first.

## Open questions

- Which clients that our users have speak OMEMO 2 today? If the answer is few, legacy OMEMO moves up.
- Does the target server keep the bundles node with `max` items? (chat.foid.space: check in phase 0.)
- What is the limit for the skipped key store, and the policy for a wiped keychain?
- Do we offer per-room encryption in spaces, or only in 1:1 chats at first? We recommend 1:1 first.
