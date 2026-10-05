# Fork of vodozemac

This folder holds a fork of the Double Ratchet code of
[vodozemac](https://github.com/matrix-org/vodozemac). vodozemac is a Rust
implementation of Olm and Megolm. It is Apache-2.0 and was audited by Least Authority
in 2022. The license is in `LICENSE` in this folder. vodozemac has no NOTICE file.

- Upstream: https://github.com/matrix-org/vodozemac
- Tag: `0.11.1`
- Commit: `3e11ae87d157da6bcac3cbf7825d37c8f5ed60af`
- Commit date: 2026-09-30

The first commit of this folder has the upstream files with no change. Every later
commit changes them for OMEMO 2. Read the history of this folder to see only our
changes.

## Files copied (paths are relative to `src/` of vodozemac)

- `types/mod.rs`, `types/curve25519.rs`, `types/ed25519.rs`
- `cipher/mod.rs`, `cipher/key.rs`
- `olm/shared_secret.rs`, `olm/session_keys.rs`, `olm/session_config.rs`
- `olm/session/mod.rs`, `olm/session/chain_key.rs`, `olm/session/double_ratchet.rs`,
  `olm/session/message_key.rs`, `olm/session/ratchet.rs`,
  `olm/session/receiver_chain.rs`, `olm/session/root_key.rs`
- `olm/messages/mod.rs`, `olm/messages/message.rs`, `olm/messages/pre_key.rs`
- `utilities/mod.rs`
