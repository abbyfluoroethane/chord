#!/usr/bin/env python3
"""A python-twomemo peer for the interop tests of chord-omemo.

It reads one JSON object for each line on stdin and writes one JSON reply for each line on
stdout. All byte values are lowercase hex strings. It uses the `Twomemo` backend of
python-twomemo (OMEMO 2, namespace urn:xmpp:omemo:2) directly, with a storage in memory.
The Rust test (`tests/interop.rs`) drives it.

Commands (every reply has "ok": true, or "error": "text"):

  init                       Make a device. Reply: {"device_id", "bundle"}.
  set_remote                 {"device_id", "bundle"}: remember the bundle of the Rust device.
  encrypt                    {"device_id", "plaintext"}: encrypt for the Rust device. Builds a
                             session on the first call. Reply: {"data", "kex", "payload"}.
  empty                      {"device_id"}: an empty OMEMO message. Reply: {"data", "kex"}.
  decrypt                    {"device_id", "data", "kex", "payload"}: decrypt a message of the
                             Rust device. "payload" is null for an empty message.
                             Reply: {"plaintext" or "empty": true, "remote_ik"}.

A bundle is {"ik", "spk_id", "spk", "spks", "prekeys": [[id, key], ...]}.
"""

import asyncio
import json
import secrets
import sys
from typing import Any, Dict, Optional

import x3dh
from omemo.session import Initiation
from omemo.storage import Just, Maybe, Nothing, Storage
from twomemo.twomemo import (
    BundleImpl,
    ContentImpl,
    EncryptedKeyMaterialImpl,
    KeyExchangeImpl,
    Twomemo,
)

OWN_JID = "python@example.org"
REMOTE_JID = "rust@example.org"


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


class Peer:
    def __init__(self) -> None:
        self.backend: Optional[Twomemo] = None
        self.device_id = 0
        self.remote_bundles: Dict[int, BundleImpl] = {}
        self.sessions: Dict[int, Any] = {}

    async def init(self, _: dict) -> dict:
        self.backend = Twomemo(MemoryStorage())
        await self.backend.generate_pre_keys(100)
        self.device_id = secrets.randbelow(0x7FFFFFFF) + 1
        b = await self.backend.get_bundle(OWN_JID, self.device_id)
        ids = b.pre_key_ids
        return {
            "device_id": self.device_id,
            "bundle": {
                "ik": b.bundle.identity_key.hex(),
                "spk_id": b.signed_pre_key_id,
                "spk": b.bundle.signed_pre_key.hex(),
                "spks": b.bundle.signed_pre_key_sig.hex(),
                "prekeys": [[ids[k], k.hex()] for k in sorted(b.bundle.pre_keys, key=lambda k: ids[k])],
            },
        }

    async def set_remote(self, cmd: dict) -> dict:
        j = cmd["bundle"]
        pks = {bytes.fromhex(k): i for i, k in j["prekeys"]}
        bundle = x3dh.Bundle(
            bytes.fromhex(j["ik"]),
            bytes.fromhex(j["spk"]),
            bytes.fromhex(j["spks"]),
            frozenset(pks),
        )
        self.remote_bundles[cmd["device_id"]] = BundleImpl(REMOTE_JID, cmd["device_id"], bundle, j["spk_id"], pks)
        return {}

    async def encrypt(self, cmd: dict) -> dict:
        dev = cmd["device_id"]
        content, pkm = await self.backend.encrypt_plaintext(bytes.fromhex(cmd["plaintext"]))
        data, kex = await self._wrap(dev, pkm)
        return {"data": data.hex(), "kex": kex, "payload": content.ciphertext.hex()}

    async def empty(self, cmd: dict) -> dict:
        _, pkm = await self.backend.encrypt_empty()
        data, kex = await self._wrap(cmd["device_id"], pkm)
        return {"data": data.hex(), "kex": kex}

    async def _wrap(self, dev: int, pkm) -> tuple:
        session = self.sessions.get(dev)
        if session is None:
            session, ekm = await self.backend.build_session_active(REMOTE_JID, dev, self.remote_bundles[dev], pkm)
            self.sessions[dev] = session
        else:
            ekm = await self.backend.encrypt_key_material(session, pkm)
        # Like python-omemo: send a key exchange until the other device answered.
        if session.initiation is Initiation.ACTIVE and not session.confirmed:
            return session.key_exchange.serialize(ekm.serialize()), True
        return ekm.serialize(), False

    async def decrypt(self, cmd: dict) -> dict:
        dev = cmd["device_id"]
        data = bytes.fromhex(cmd["data"])
        remote_ik = None
        if cmd["kex"]:
            ke, auth = KeyExchangeImpl.parse(data)
            remote_ik = ke.identity_key
            ekm = EncryptedKeyMaterialImpl.parse(auth, REMOTE_JID, dev)
            session = self.sessions.get(dev)
            if (
                session is not None
                and session.initiation is Initiation.PASSIVE
                and session.key_exchange.builds_same_session(ke)
            ):
                pkm = await self.backend.decrypt_key_material(session, ekm)
            else:
                session, pkm = await self.backend.build_session_passive(REMOTE_JID, dev, ke, ekm)
                self.sessions[dev] = session
        else:
            session = self.sessions[dev]
            ekm = EncryptedKeyMaterialImpl.parse(data, REMOTE_JID, dev)
            pkm = await self.backend.decrypt_key_material(session, ekm)
        if cmd.get("payload") is None:
            assert pkm.key == b"\x00" * 32, "an empty message must carry 32 zero bytes"
            return {"empty": True, "remote_ik": remote_ik.hex() if remote_ik else None}
        plaintext = await self.backend.decrypt_plaintext(ContentImpl(bytes.fromhex(cmd["payload"])), pkm)
        return {"plaintext": plaintext.hex(), "remote_ik": remote_ik.hex() if remote_ik else None}


async def main() -> None:
    peer = Peer()
    loop = asyncio.get_running_loop()
    while True:
        line = await loop.run_in_executor(None, sys.stdin.readline)
        if not line:
            return
        cmd = json.loads(line)
        try:
            reply = await getattr(peer, cmd["cmd"])(cmd)
            reply["ok"] = True
        except Exception as e:  # report it to the test, which decides if it is expected
            reply = {"error": f"{type(e).__name__}: {e}"}
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    asyncio.run(main())
