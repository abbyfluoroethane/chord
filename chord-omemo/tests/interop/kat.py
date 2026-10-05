#!/usr/bin/env python3
"""Make the known-answer vectors for chord-omemo: `kat.json`.

It drives the Twomemo backend of python-twomemo (with python-x3dh and python-doubleratchet
under it) through a whole conversation, with a fixed source of "random" bytes. The result is
a list of operations with the bytes that python made. The Rust test `src/kat.rs` runs the same
operations with the same random bytes and compares every output byte for byte.

Run it from the repository root, with the venv of the interop tests:

    chord-omemo/tests/interop/venv/bin/python chord-omemo/tests/interop/kat.py \
        > chord-omemo/tests/interop/kat.json

The output does not change from run to run. Only secret keys of test devices are in it.

The conversation:
  1  A sends a key exchange to B (A starts the session, B has one prekey)
  2  B reads it
  3  B answers
  4  A reads the answer
  5  A sends three messages (m2, m3, m4)
  6  B reads them out of order: m4, m2, m3
  7  B sends two messages (r2, r3)
  8  A reads them out of order: r3, r2
  9  A sends one more message
 10  B reads it
"""

import asyncio
import hashlib
import json
import secrets
from typing import Any, Dict, List

import x3dh
from omemo.session import Initiation
from omemo.storage import Just, Maybe, Nothing, Storage
from twomemo.twomemo import (
    EncryptedKeyMaterialImpl,
    KeyExchangeImpl,
    PlainKeyMaterialImpl,
    Twomemo,
)

A_JID, B_JID = "a@example.org", "b@example.org"
A_DEV, B_DEV = 1111, 2222

# --- the fixed source of random bytes -------------------------------------------------------

_counter = 0
_tape: List[bytes] = []


def fixed_token_bytes(n: int = 32) -> bytes:
    """The n-th call gives the first n bytes of SHA-512("chord-omemo-kat" || n-th number)."""
    global _counter
    out = hashlib.sha512(b"chord-omemo-kat" + _counter.to_bytes(4, "big")).digest()[:n]
    _counter += 1
    _tape.append(out)
    return out


secrets.token_bytes = fixed_token_bytes

# python-doubleratchet makes its ratchet keys with the `cryptography` package. Use the same
# fixed source for them.
from doubleratchet.recommended import diffie_hellman_ratchet_curve25519 as _dh  # noqa: E402

_dh.DiffieHellmanRatchet._generate_priv = staticmethod(lambda: fixed_token_bytes(32))


def take_tape() -> List[str]:
    """The random bytes that were used since the last call, as hex."""
    global _tape
    used, _tape = _tape, []
    return [t.hex() for t in used]


class MemoryStorage(Storage):
    def __init__(self) -> None:
        super().__init__(disable_cache=True)
        self.data: Dict[str, Any] = {}

    async def _load(self, key: str) -> Maybe:
        return Just(self.data[key]) if key in self.data else Nothing()

    async def _store(self, key: str, value: Any) -> None:
        self.data[key] = value

    async def _delete(self, key: str) -> None:
        self.data.pop(key, None)


def material(label: str) -> PlainKeyMaterialImpl:
    """48 bytes of key material for a message: a 32-byte key and a 16-byte MAC."""
    h = hashlib.sha512(b"chord-omemo-kat-material" + label.encode()).digest()
    return PlainKeyMaterialImpl(h[:32], h[32:48])


def hexmat(p: PlainKeyMaterialImpl) -> str:
    return (p.key + p.auth_tag).hex()


async def main() -> None:
    sa, sb = MemoryStorage(), MemoryStorage()
    a, b = Twomemo(sa), Twomemo(sb)
    # Make the keys now, so that the later random bytes are for the sessions only.
    await a.generate_pre_keys(1)
    await b.generate_pre_keys(1)
    bundle_a = await a.get_bundle(A_JID, A_DEV)
    bundle_b = await b.get_bundle(B_JID, B_DEV)
    assert len(bundle_b.bundle.pre_keys) == 1
    setup_tape = take_tape()

    state_a = sa.data["/urn:xmpp:omemo:2/x3dh"]
    state_b = sb.data["/urn:xmpp:omemo:2/x3dh"]

    (pk_pub,) = list(bundle_b.bundle.pre_keys)

    def secret_of(state: dict, name: str) -> str:
        """The secret of a key in the stored state. The state holds bytes as latin-1 text."""
        v = state[name]["secret"] if "secret" in state[name] else state[name]["priv"]
        return v.encode("latin-1").hex()

    ops: List[dict] = []
    sessions: Dict[str, Any] = {}

    async def wrap(who: Twomemo, key: str, dev: int, pkm: PlainKeyMaterialImpl) -> tuple:
        """Encrypt like python-omemo: a key exchange until the other side answered."""
        session = sessions[key]
        ekm = await who.encrypt_key_material(session, pkm)
        if session.initiation is Initiation.ACTIVE and not session.confirmed:
            return session.key_exchange.serialize(ekm.serialize()), True
        return ekm.serialize(), False

    async def send(actor: str, label: str) -> bytes:
        who, key, dev = (a, "a", B_DEV) if actor == "A" else (b, "b", A_DEV)
        pkm = material(label)
        if actor == "A" and "a" not in sessions:
            session, ekm = await a.build_session_active(B_JID, B_DEV, bundle_b, pkm)
            sessions["a"] = session
            data, kex = session.key_exchange.serialize(ekm.serialize()), True
        else:
            data, kex = await wrap(who, key, dev, pkm)
        ops.append(
            {
                "op": "send",
                "actor": actor,
                "label": label,
                "kex": kex,
                "key_material": hexmat(pkm),
                "data": data.hex(),
                "tape": take_tape(),
            }
        )
        return data

    async def recv(actor: str, label: str, data: bytes, kex: bool) -> None:
        if actor == "B":
            if kex:
                ke, auth = KeyExchangeImpl.parse(data)
                ekm = EncryptedKeyMaterialImpl.parse(auth, A_JID, A_DEV)
                session, pkm = await b.build_session_passive(A_JID, A_DEV, ke, ekm)
                sessions["b"] = session
            else:
                ekm = EncryptedKeyMaterialImpl.parse(data, A_JID, A_DEV)
                pkm = await b.decrypt_key_material(sessions["b"], ekm)
        else:
            ekm = EncryptedKeyMaterialImpl.parse(data, B_JID, B_DEV)
            pkm = await a.decrypt_key_material(sessions["a"], ekm)
        ops.append(
            {
                "op": "recv",
                "actor": actor,
                "label": label,
                "kex": kex,
                "key_material": hexmat(pkm),
                "data": data.hex(),
                "tape": take_tape(),
            }
        )

    m1 = await send("A", "m1")
    await recv("B", "m1", m1, True)
    r1 = await send("B", "r1")
    await recv("A", "r1", r1, False)
    ms = [await send("A", f"m{i}") for i in (2, 3, 4)]
    for i in (2, 0, 1):  # m4, m2, m3
        await recv("B", f"m{i + 2}", ms[i], False)
    rs = [await send("B", f"r{i}") for i in (2, 3)]
    for i in (1, 0):  # r3, r2
        await recv("A", f"r{i + 2}", rs[i], False)
    m5 = await send("A", "m5")
    await recv("B", "m5", m5, False)

    out = {
        "about": "Known-answer vectors made by kat.py with python-twomemo 2.0.0.",
        "a": {
            "jid": A_JID,
            "device_id": A_DEV,
            "identity_seed": secret_of(state_a, "identity_key"),
            "identity_key": bundle_a.bundle.identity_key.hex(),
        },
        "b": {
            "jid": B_JID,
            "device_id": B_DEV,
            "identity_seed": secret_of(state_b, "identity_key"),
            "identity_key": bundle_b.bundle.identity_key.hex(),
            "signed_prekey_id": bundle_b.signed_pre_key_id,
            "signed_prekey_secret": secret_of(state_b, "signed_pre_key"),
            "signed_prekey": bundle_b.bundle.signed_pre_key.hex(),
            "signed_prekey_signature": bundle_b.bundle.signed_pre_key_sig.hex(),
            "prekey_id": bundle_b.pre_key_ids[pk_pub],
            "prekey_secret": state_b["pre_keys"][0].encode("latin-1").hex(),
            "prekey": pk_pub.hex(),
        },
        "operations": ops,
    }
    print(json.dumps(out, indent=1))


asyncio.run(main())
