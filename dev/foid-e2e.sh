#!/usr/bin/env bash
# End-to-end run of the Chord core through chord-cli against chat.foid.space (ejabberd
# 26.07 with Movim). It replaces the local Prosody runs on a laptop: no Docker, no VM.
#
# Accounts: chordtest and chordtest2 @chat.foid.space. Their passwords are in
# dev/foid/.env (gitignored): CHORDTEST_PASSWORD and CHORDTEST2_PASSWORD.
# Each run uses new, empty databases, so the MAM catch-up path runs too.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

set -a
# shellcheck disable=SC1091
source dev/foid/.env
set +a
unset CHORD_SERVER # SRV lookup, as a real client does
# Parallel agents share target/. Build this checkout into its own directory, so the run
# tests this code and not a binary from another worktree.
export CARGO_TARGET_DIR="${CHORD_E2E_TARGET:-$PWD/target/lead}"
cargo build -q -p chord-cli
cli="$CARGO_TARGET_DIR/debug/chord-cli"
work=$(mktemp -d)
trap '[[ -n "${KEEP_WORK:-}" ]] || rm -rf "$work"' EXIT

a() { CHORD_JID=chordtest@chat.foid.space CHORD_PASSWORD="$CHORDTEST_PASSWORD" CHORD_DB="$work/a.sqlite3" "$cli" "$@"; }
b() { CHORD_JID=chordtest2@chat.foid.space CHORD_PASSWORD="$CHORDTEST2_PASSWORD" CHORD_DB="$work/b.sqlite3" "$cli" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
# item <text> <python expression>: evaluate the expression on the newest timeline item
# whose body contains the text. With an empty text, take the newest retracted item.
item() {
  python3 -c "
import json, sys
for it in reversed(json.load(sys.stdin)):
    if (sys.argv[1] and sys.argv[1] in it['body']) or (not sys.argv[1] and it['retracted']):
        print($2)
        break
" "$1"
}
# retry <n> <command...>: run the command until it succeeds, n times, 3 s apart. The
# server delivers to the other account after a short time.
retry() {
  local tries=$1 i
  shift
  for ((i = 1; i <= tries; i++)); do
    if "$@"; then return 0; fi
    sleep 3
  done
  return 1
}

n=$RANDOM
A=chordtest@chat.foid.space
B=chordtest2@chat.foid.space
room=chord-e2e@conference.chat.foid.space

echo "1. chat: A sends to B, and B sees it"
text="e2e $n hello"
a send "$B" "$text" >/dev/null
has_text() { b --json timeline "$A" --limit 20 | grep -qF "$1"; }
retry 5 has_text "$text" || fail "B does not see the message"

echo "2. A edits the message, B reacts and replies, and A sees both"
id=$(a --offline --json timeline "$B" --limit 20 | item "$text" "it['id']")
[[ -n "$id" ]] || fail "A has no item for its message"
a edit "$id" "$text (edited)" >/dev/null
has_edit() { [[ "$(b --json timeline "$A" --limit 20 | item "$text" "it['edited']")" == True ]]; }
retry 5 has_edit || fail "B does not see the edit"
bid=$(b --offline --json timeline "$A" --limit 20 | item "$text" "it['id']")
b react "$bid" "👍" >/dev/null
b reply "$bid" "e2e $n reply" >/dev/null
has_reaction() { [[ "$(a --json timeline "$B" --limit 20 | item "$text" "[r['emoji'] for r in it['reactions']]")" == "['👍']" ]]; }
retry 5 has_reaction || fail "A does not see the reaction"
has_reply() { [[ -n "$(a --json timeline "$B" --limit 20 | item "e2e $n reply" "(it['reply_to'] or {}).get('body', '')")" ]]; }
if ! retry 5 has_reply; then
  a --offline --json timeline "$B" --limit 5 | python3 -c "
import json, sys
for it in json.load(sys.stdin):
    print(it['id'], repr(it['body'][:40]), it['reply_to'])" >&2
  fail "A does not see the reply quote"
fi

echo "3. A retracts the message, and B sees the retraction"
a retract "$id" >/dev/null
has_retraction() { b --json timeline "$A" --limit 20 | item "" "it['id']" | grep -qxF "$bid"; }
retry 5 has_retraction || fail "B does not see the retraction"

echo "4. B marks the chat as read"
b read "$A" >/dev/null
unread=$(b --offline --json channels home | python3 -c "
import json, sys
print([c['unread'] for c in json.load(sys.stdin) if c['jid'] == sys.argv[1]])" "$A")
[[ "$unread" == "[0]" ]] || fail "B still has unread messages: $unread"

echo "5. spaces: A makes an authorize space, B asks to join, and A approves"
created=$(a --json space-create "E2E $n" --authorize)
service=$(python3 -c "import json,sys; print(json.loads(sys.argv[1])['service'])" "$created")
node=$(python3 -c "import json,sys; print(json.loads(sys.argv[1])['node'])" "$created")
cleanup_space() { a space-delete "$service" "$node" >/dev/null 2>&1 || true; }
trap 'cleanup_space; [[ -n "${KEEP_WORK:-}" ]] || rm -rf "$work"' EXIT
b space-browse | grep -qF "E2E $n" || fail "B does not see the new space in browse"
b space-join "$service" "$node" | grep -q "must approve" || fail "the join is not pending"
b --offline space-pending | grep -qF "E2E $n" || fail "B has no pending join"
a space-requests "$service" "$node" | grep -qF "$B" || fail "A does not see the join request"
a space-approve "$service" "$node" "$B" >/dev/null
has_space() { b --json spaces | grep -qF "\"$node\""; }
retry 5 has_space || fail "B does not have the space after the approval"
b --offline space-pending | grep -qF "E2E $n" && fail "the join is still pending"

echo "6. room: both join $room, A sends, and B sees it"
a join "$room" --nick chordtest >/dev/null
# The server makes new rooms members-only. The owner grants B membership first.
a room-member "$room" "$B" member >/dev/null
a room-members "$room" member | grep -qF "$B" || fail "B is not a member of the room"
b join "$room" --nick chordtest2 >/dev/null
rtext="e2e $n room"
a send "$room" "$rtext" >/dev/null
has_room_text() { b --json timeline "$room" --limit 20 | grep -qF "$rtext"; }
retry 5 has_room_text || fail "B does not see the room message"

echo "7. A sends B a private message in the room"
a pm "$room" chordtest2 "e2e $n private" >/dev/null

echo "8. A uploads a file to B"
printf 'chord e2e %s\n' "$n" > "$work/e2e.txt"
a upload "$B" "$work/e2e.txt" >/dev/null


echo "9. B stays in the room, A sends a message and moderates it, and B sees it go"
# ejabberd removes a moderated message from the room archive and does not archive the
# moderation. So only occupants that are online at that time see it. B follows the room.
b timeline "$room" --follow >/dev/null 2>&1 &
follower=$!
sleep 8
mtext="e2e $n moderate me"
a send "$room" "$mtext" >/dev/null
# A CLI session ends before the room echo arrives. The next login gets the stanza-id.
mid=$(a --json timeline "$room" --limit 20 | item "$mtext" "it['id']")
[[ -n "$mid" ]] || fail "A has no item for its room message"
sleep 3
a moderate "$mid" "e2e test" >/dev/null
sleep 6
kill "$follower" 2>/dev/null || true
wait "$follower" 2>/dev/null || true
moderated=$(b --offline --json timeline "$room" --limit 20 | python3 -c "
import json, sys
items = json.load(sys.stdin)
print(any(it['retracted'] and not it['outgoing'] for it in items[-3:]))")
[[ "$moderated" == True ]] || fail "B does not see the moderation"

echo "10. push: A registers a push node, lists it, and removes it"
a push-enable push.chat.foid.space "chord-e2e-$n" >/dev/null
a --offline push-list | grep -qF "chord-e2e-$n" || fail "the push registration is not stored"
a push-disable push.chat.foid.space "chord-e2e-$n" >/dev/null
a --offline push-list | grep -qF "chord-e2e-$n" && fail "the push registration is still stored"

echo "11. notifications: A mutes the chat with B, offline"
a --offline notify "$B" none >/dev/null
a --offline notify "$B" | grep -q "none" || fail "the notification level is not stored"

echo "12. A changes its nick in the room, and its next message still counts as its own"
a nick "$room" "chordtest-$n" >/dev/null
ntext="e2e $n after the nick change"
a send "$room" "$ntext" >/dev/null
own=$(a --json timeline "$room" --limit 20 | item "$ntext" "it['outgoing']")
a nick "$room" chordtest >/dev/null
[[ "$own" == True ]] || fail "A's message after the nick change is not its own: $own"

echo "13. A blocks B, B sends, A does not get it, and A unblocks B"
a block "$B" >/dev/null
a --offline blocked | grep -qF "$B" || fail "B is not in A's block list"
btext="e2e $n while blocked"
b send "$A" "$btext" >/dev/null
sleep 3
a --json timeline "$B" --limit 20 | grep -qF "$btext" && fail "A got a message from a blocked address"
a unblock "$B" >/dev/null
a --offline blocked | grep -qF "$B" && fail "B is still in A's block list"

echo "PASS: foid end-to-end run"
