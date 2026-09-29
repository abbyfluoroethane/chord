#!/usr/bin/env bash
# End-to-end run of the Chord core through chord-cli, against the dev Prosody server.
# Alice makes a room and a private space. Bob becomes a member, joins, and reads the
# room. Alice uploads a file to bob. The script deletes the space at the end.
# Before you run it: start the test server with dev/prosody/setup.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

set -a
# shellcheck disable=SC1091
source dev/prosody/.env
set +a
export CHORD_SERVER=starttls://localhost:5222
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -q -p chord-cli
cli=target/debug/chord-cli

alice() { CHORD_JID=alice@chord.localhost CHORD_PASSWORD="$ALICE_PASSWORD" "$cli" "$@"; }
bob() { CHORD_JID=bob@chord.localhost CHORD_PASSWORD="$BOB_PASSWORD" "$cli" "$@"; }
json_field() { python3 -c "import json,sys; print(json.load(sys.stdin)['$1'])"; }

n=$RANDOM
room="e2e-$n@rooms.chord.localhost"
work=$(mktemp -d)
service="" node=""
cleanup() {
  if [[ -n "$node" ]]; then
    alice space-delete "$service" "$node" >/dev/null 2>&1 || true
  fi
  alice leave "$room" >/dev/null 2>&1 || true
  bob leave "$room" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

echo "1. alice joins $room and makes a private space"
alice join "$room"
created=$(alice --json space-create "E2E $n" --private)
service=$(json_field service <<<"$created")
node=$(json_field node <<<"$created")
alice space-add-room "$service" "$node" "$room" general
alice space-add-member "$service" "$node" bob@chord.localhost

echo "2. bob joins the space and sees the room"
bob space-join "$service" "$node"
channels=$(bob --json channels "$service" "$node")
grep -q "\"$room\"" <<<"$channels" || { echo "FAIL: bob does not see $room: $channels" >&2; exit 1; }

echo "3. bob joins the room, alice sends to it, bob reads it"
bob join "$room"
text="hello room $n"
alice send "$room" "$text"
timeline=$(bob --json timeline "$room" --limit 5)
grep -qF "$text" <<<"$timeline" || { echo "FAIL: bob timeline lacks the message: $timeline" >&2; exit 1; }

echo "4. alice uploads a file to bob"
printf 'chord e2e %s\n' "$n" > "$work/e2e.txt"
alice upload bob@chord.localhost "$work/e2e.txt"

echo "5. bob sees the space in his state"
bob state | grep -F "E2E $n" >/dev/null || { echo "FAIL: the space is not in bob's state" >&2; exit 1; }
echo "PASS: end-to-end run"
