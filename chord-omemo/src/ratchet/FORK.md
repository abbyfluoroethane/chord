# Fork of vodozemac

This folder holds a fork of the Double Ratchet code of
[vodozemac](https://github.com/matrix-org/vodozemac). vodozemac is a Rust implementation of
Olm and Megolm. It is Apache-2.0. Least Authority audited it in 2022. The license is in
`LICENSE` in this folder. vodozemac has no NOTICE file.

- Upstream: https://github.com/matrix-org/vodozemac
- Tag: `0.11.1`
- Commit: `3e11ae87d157da6bcac3cbf7825d37c8f5ed60af`
- Commit date: 2026-09-30

The first commit of this folder has the upstream files with no change. Every later commit
changes them for OMEMO 2. Read the history of this folder to see only our changes
(`git log -p -- chord-omemo/src/ratchet`). Each changed file has the line "Modified for OMEMO
2 by the Chord authors, 2026" in its header, as Apache-2.0 section 4 asks.

The changes need an outside review before the feature is called stable.

## Files copied (paths are relative to `src/` of vodozemac)

- `types/mod.rs`, `types/curve25519.rs`, `types/ed25519.rs`
- `cipher/mod.rs`, `cipher/key.rs`
- `olm/shared_secret.rs`, `olm/session_keys.rs`, `olm/session_config.rs`
- `olm/session/mod.rs`, `olm/session/chain_key.rs`, `olm/session/double_ratchet.rs`,
  `olm/session/message_key.rs`, `olm/session/ratchet.rs`, `olm/session/receiver_chain.rs`,
  `olm/session/root_key.rs`
- `olm/messages/mod.rs`, `olm/messages/message.rs`, `olm/messages/pre_key.rs`
- `utilities/mod.rs`

We did not copy the Olm account, Megolm, the pickle code, or the libolm compatibility code.

## Files deleted again

- `types/ed25519.rs`: the identity key uses `ed25519-dalek` directly (`../identity.rs`).
- `olm/session_keys.rs`, `olm/session_config.rs`: Olm session ids and Olm session versions.
- `utilities/mod.rs`: base64 helpers and the libolm pickle format.

## Files added

- `mod.rs`, `olm/mod.rs`: the module tree.

## Changes

Sources: XEP-0384 0.9.1 (xmpp.org/extensions/xep-0384.html), and python-twomemo 2.0.0,
python-doubleratchet 1.2.0, python-x3dh 1.2.0 (all MIT).

| File | Change | Reason, source |
|---|---|---|
| `olm/shared_secret.rs` | 3DH became X3DH: four DH values (DH1 to DH4 with the signed prekey and the one-time prekey), 32 bytes of 0xFF in front, HKDF-SHA-256 with 32 zero bytes as salt, info "OMEMO X3DH", 32 bytes of output. A one-time prekey is required. | XEP section 4.2, X3DH spec section 2.2 and 3.3, python-x3dh `get_shared_secret_active` and `get_shared_secret_passive`. |
| `olm/session/root_key.rs` | HKDF info "OLM_RATCHET" became "OMEMO Root Chain". The DH step returns an error for a non-contributory key. | XEP section 4.3 (KDF_RK), python-twomemo `RootChainKDFImpl`. |
| `olm/session/chain_key.rs` | None to the constants: HMAC-SHA-256 with 0x01 for the message key and 0x02 for the next chain key is the same in Olm and OMEMO. The message key also carries `pn`. The libolm code is gone. | XEP section 4.3 (KDF_CK), python-twomemo `MESSAGE_CHAIN_CONSTANT`. |
| `cipher/key.rs` | The message key expansion: salt is 32 zero bytes (it was one zero byte), info "OMEMO Message Key Material" (it was "OLM_KEYS"). The Megolm and pickle variants are gone. | XEP section 4.3 (ENCRYPT), python-twomemo `AEADImpl`. |
| `cipher/mod.rs` | The MAC is HMAC-SHA-256 cut to 16 bytes (it was 8 bytes, or 32), checked in constant time, and the tag must have exactly 16 bytes. The MAC input is the associated data followed by the serialized `OMEMOMessage`. The fuzzing switch that turned off the MAC check is gone. | XEP section 4.3 (authentication tag truncation, ENCRYPT). |
| `olm/messages/*` | The Olm message format became the protobuf messages `OMEMOMessage` (n, pn, dh_pub, ciphertext), `OMEMOAuthenticatedMessage` (mac, message) and `OMEMOKeyExchange` (pk_id, spk_id, ik, ek, message), as `prost` derive structs. Required fields are checked after the parse. All lengths are capped. A public key with the top bit set is refused. | XEP section 12 (Protobuf Schema). |
| `olm/session/message_key.rs` | Encrypt: AES-256-CBC with PKCS#7, then the MAC over the associated data and the message. Decrypt: check the MAC first. | XEP section 4.3 (ENCRYPT). |
| `olm/session/double_ratchet.rs` | Alice runs one root KDF step with a new ratchet key and the signed prekey of Bob when she starts. Both sides make the new sending chain at once when a new ratchet key arrives (vodozemac waited for the next send). The state tracks `pn`, the length of the previous sending chain. The Olm-only start and the libolm and low-level code are gone. | XEP section 4.3 (ratchet initialization), Double Ratchet spec section 3.3 and 3.5, python-doubleratchet `DiffieHellmanRatchet`. |
| `olm/session/receiver_chain.rs` | MAX_SKIP: at most 1000 skipped keys in one message gap and 1000 stored per chain (it was 2000 and 40). A new limit of 1000 for the whole session, with the oldest key dropped first. The decrypt takes the associated data. | XEP section 4.3 (MAX_SKIP, deletion policy), python-twomemo (1000 and 1000). |
| `olm/session/mod.rs` | The Olm `Session` is a session of OMEMO: it holds the associated data (`IK_A \|\| IK_B`, the Ed25519 keys of the initiator and the responder), keeps sending the key exchange until the first answer, and knows the key exchange that made it (a repeat of it is read as a normal message). The pickle is one version byte and JSON. The libolm and low-level code is gone. | XEP section 4.2 (associated data), section 6 (business rules). |
| `olm/session/ratchet.rs` | A failed random source gives an error and no panic. `Debug` of a secret key shows no key. The `Decode` derive for libolm is gone. | Safety. |
| `types/curve25519.rs` | Key generation takes the random bytes from `getrandom` and returns an error if that fails. Base64 and the libolm pickle code are gone. `Debug` of a secret key shows no key. | Safety, fewer dependencies. |

The glue code outside this folder (`../identity.rs`, `../session.rs`) is new. It uses
`ed25519-dalek` for the identity key (`SigningKey::to_scalar_bytes`,
`VerifyingKey::to_montgomery`), signs the signed prekey over its 32 bytes (XEP section 4.2,
"Encode(PK)"), and checks the signature with `verify_strict` before it uses a bundle.

## What we kept without change in meaning

- The structure: `RootKey`, `RemoteRootKey`, `ChainKey`, `RemoteChainKey`, `MessageKey`,
  `ReceiverChain`, `DoubleRatchet`.
- A decrypt works on a copy of the state of a chain and commits only after the MAC and the
  padding are good, so a failure changes nothing.
- `Zeroize` on the key types, and a `Debug` that shows no key.
- Out-of-order messages: old receiver chains stay (up to 5), and each keeps its skipped keys.
