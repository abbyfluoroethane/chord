#!/usr/bin/env bash
# End-to-end run of the Chord core through chord-cli, against the dev Prosody server.
# Alice makes a room and a private space. Bob becomes a member, joins, and reads the
# room. Alice edits her message, bob reacts and replies, alice retracts the message,
# and bob marks the room as read. Alice uploads a file to bob, and sends a private
# message to bob in the room. The script deletes the space at the end.
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
# find_item <text> <python expression on item>: evaluate the expression on the first
# timeline item whose body contains the text. Prints nothing if no item matches.
find_item() {
  python3 -c "
import json, sys
for item in json.load(sys.stdin):
    if sys.argv[1] in item['body'] or (sys.argv[1] == '' and item['retracted']):
        print($2)
        break
" "$1"
}
fail() { echo "FAIL: $*" >&2; exit 1; }

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
grep -q "\"$room\"" <<<"$channels" || fail "bob does not see $room: $channels"

echo "3. bob joins the room, alice sends to it, bob reads it"
bob join "$room"
text="hello room $n"
alice send "$room" "$text"
timeline=$(bob --json timeline "$room" --limit 5)
grep -qF "$text" <<<"$timeline" || fail "bob timeline lacks the message: $timeline"

echo "4. alice edits the message, and bob sees the edit"
id=$(alice --json timeline "$room" --limit 5 | find_item "$text" "item['id']")
[[ "$id" == m:* ]] || fail "alice has no timeline id for her message: '$id'"
alice edit "$id" "$text (edited)"
edited=$(bob --json timeline "$room" --limit 5 | find_item "$text" "item['body'], item['edited']")
[[ "$edited" == "$text (edited) True" ]] || fail "bob does not see the edit: $edited"

echo "5. bob reacts and replies, and alice sees both"
bob react "$id" "👍"
bob reply "$id" "reply $n"
timeline=$(alice --json timeline "$room" --limit 10)
reaction=$(find_item "$text" "[(r['emoji'], r['count']) for r in item['reactions']]" <<<"$timeline")
[[ "$reaction" == "[('👍', 1)]" ]] || fail "alice does not see the reaction: $reaction"
quote=$(find_item "reply $n" "item['body'], (item['reply_to'] or {}).get('id')" <<<"$timeline")
[[ "$quote" == "reply $n $id" ]] || fail "alice does not see the reply: $quote"

echo "6. alice retracts the message, and bob sees the retraction"
alice retract "$id"
bob --json timeline "$room" --limit 10 | find_item "" "item['id']" | grep -qxF "$id" ||
  fail "bob does not see the retraction"

echo "7. bob marks the room as read"
bob read "$room"
unread=$(bob --offline --json channels "$service" "$node" | python3 -c "
import json, sys
print([c['unread'] for c in json.load(sys.stdin) if c['jid'] == sys.argv[1]])" "$room")
[[ "$unread" == "[0]" ]] || fail "the room still has unread messages for bob: $unread"

echo "8. alice uploads a file to bob, and sends him a private message in the room"
printf 'chord e2e %s\n' "$n" > "$work/e2e.txt"
alice upload bob@chord.localhost "$work/e2e.txt"
alice pm "$room" bob "private $n"

echo "9. bob sees the space in his state"
bob state | grep -F "E2E $n" >/dev/null || fail "the space is not in bob's state"
echo "PASS: end-to-end run"
