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

- `counters_from_python_show_the_heartbeat_at_53`: the counter of a message of Python reaches
  53, and Rust reports the heartbeat.

## Known-answer vectors

`kat.py` runs one whole conversation of two devices through python-twomemo, with a fixed
source of random bytes, and prints every message and every secret key as JSON. `kat.json` is
that output. It is in git, and the unit test `src/kat.rs` reads it: it runs the same
conversation in Rust with the same random bytes and compares each message byte for byte. That
test needs no Python. To make the file again (the result is the same each time):

```sh
chord-omemo/tests/interop/venv/bin/python chord-omemo/tests/interop/kat.py > chord-omemo/tests/interop/kat.json
```

The payload (HKDF, AES-256-CBC, HMAC) is Rust code over RustCrypto crates. Python decrypts
it with its own code, so the tests check that part too.
