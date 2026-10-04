#!/usr/bin/env bash
# Regenerate the offline sources of the Flatpak build:
#   cargo-sources.json  from Cargo.lock                    (flatpak-cargo-generator.py)
#   node-sources.json   from chord-desktop/package-lock.json (flatpak-node-generator)
#
# Run it from anywhere, after each change to Cargo.lock or package-lock.json, and commit the
# two files:
#
#   packaging/flatpak/update-sources.sh
#
# Needs python3 (3.10 or newer), curl and tar. It downloads flatpak-builder-tools at the
# pinned commit below into a cache directory and installs its Python dependencies in a
# virtual environment there. Nothing goes into the repository but the two JSON files.
# To move to a newer flatpak-builder-tools, change TOOLS_COMMIT.
set -euo pipefail

TOOLS_COMMIT=74697c75b630d7330e77250fc13cb5ea688d9479 # flatpak/flatpak-builder-tools, 2026-09-30
NODE_EXTENSION=org.freedesktop.Sdk.Extension.node24//26.08

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cache=${XDG_CACHE_HOME:-$HOME/.cache}/chord-flatpak-tools/$TOOLS_COMMIT
tools=$cache/flatpak-builder-tools-$TOOLS_COMMIT

if [ ! -d "$tools" ]; then
    mkdir -p "$cache"
    curl -fsSL "https://codeload.github.com/flatpak/flatpak-builder-tools/tar.gz/$TOOLS_COMMIT" |
        tar -xz -C "$cache"
fi
if [ ! -x "$cache/venv/bin/flatpak-node-generator" ]; then
    python3 -m venv "$cache/venv"
    "$cache/venv/bin/pip" install --quiet --disable-pip-version-check "aiohttp>=3.9.5,<4" "PyYAML>=6.0.2,<7" "tomlkit>=0.13.3,<1"
    "$cache/venv/bin/pip" install --quiet --disable-pip-version-check "$tools/node"
fi

"$cache/venv/bin/python" "$tools/cargo/flatpak-cargo-generator.py" \
    "$root/Cargo.lock" -o "$here/cargo-sources.json"

# The generator reads the lockfile only. It wants no node_modules next to it, so give it a
# copy of the lockfile in an empty directory.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "$root/chord-desktop/package-lock.json" "$root/chord-desktop/package.json" "$tmp/"
"$cache/venv/bin/flatpak-node-generator" npm "$tmp/package-lock.json" \
    --node-sdk-extension "$NODE_EXTENSION" -o "$here/node-sources.json"

echo "Wrote $here/cargo-sources.json and $here/node-sources.json"
