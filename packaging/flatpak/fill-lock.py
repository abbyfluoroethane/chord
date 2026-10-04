#!/usr/bin/env python3
"""Add the missing "resolved" and "integrity" fields to an npm package-lock.json.

Some npm versions write lockfile entries with only a version. npm ci then asks the
registry for the package, and an offline build (Flatpak) cannot. flatpak-node-generator
also skips such entries. This script takes the tarball URL and the integrity of the same
version from the registry. It changes no version.

Usage: fill-lock.py path/to/package-lock.json
"""

import json
import sys
import urllib.parse
import urllib.request

REGISTRY = "https://registry.npmjs.org"


def package_name(path: str) -> str:
    # node_modules/a/node_modules/@scope/b -> @scope/b
    return path.rsplit("node_modules/", 1)[1]


def dist(name: str, version: str) -> dict:
    url = f"{REGISTRY}/{urllib.parse.quote(name, safe='@')}/{version}"
    with urllib.request.urlopen(url, timeout=30) as response:
        return json.load(response)["dist"]


def main() -> None:
    path = sys.argv[1]
    with open(path, encoding="utf-8") as f:
        lock = json.load(f)
    filled = 0
    for key, entry in lock.get("packages", {}).items():
        if not key or entry.get("link") or entry.get("inBundle"):
            continue
        if "resolved" in entry and "integrity" in entry:
            continue
        info = dist(package_name(key), entry["version"])
        entry.setdefault("resolved", info["tarball"])
        entry.setdefault("integrity", info["integrity"])
        filled += 1
    if filled:
        # Same layout as npm: two spaces, a newline at the end. Keep the key order of npm.
        for entry in lock["packages"].values():
            if "resolved" in entry and "integrity" in entry:
                keys = list(entry)
                for k in ("resolved", "integrity"):
                    keys.remove(k)
                at = keys.index("version") + 1 if "version" in keys else 0
                keys[at:at] = ["resolved", "integrity"]
                ordered = {k: entry[k] for k in keys}
                entry.clear()
                entry.update(ordered)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(lock, f, indent=2, ensure_ascii=False)
            f.write("\n")
    print(f"{path}: filled {filled} entries")


if __name__ == "__main__":
    main()
