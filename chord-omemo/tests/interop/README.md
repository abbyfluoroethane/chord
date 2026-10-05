# Interop test with python-twomemo

`twomemo_peer.py` is a small OMEMO 2 peer. It uses the `Twomemo` backend of
[python-twomemo](https://github.com/Syndace/python-twomemo) (MIT), the reference
implementation of Gajim and others. `../interop.rs` starts it and talks to it with one JSON
object for each line.

## Run

From the repository root:

```sh
python3 -m venv chord-omemo/tests/interop/venv
chord-omemo/tests/interop/venv/bin/pip install -r chord-omemo/tests/interop/requirements.txt
cargo test -p chord-omemo --test interop -- --ignored --nocapture
```

To use a venv in another place, set `CHORD_OMEMO_PYTHON` to its `python`. The venv folder
is not in git.

## What the tests prove

- `rust_starts_the_session`: Python publishes a bundle. Rust builds a session from it and
  encrypts a payload (key exchange). Python decrypts it. Then 5 messages each way, one
  out-of-order round each way, and more messages.
- `python_starts_the_session`: Rust publishes a bundle. Python sends a key exchange. Rust
  decrypts it. Empty OMEMO messages both ways. Then the same 5 plus out-of-order rounds.
- `damaged_data_is_refused_on_both_sides`: one changed byte fails on both sides, and a
  good message still works afterwards.

The payload (HKDF, AES-256-CBC, HMAC) is Rust code over RustCrypto crates. Python decrypts
it with its own code, so the tests check that part too.
