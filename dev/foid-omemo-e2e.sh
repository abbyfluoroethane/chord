#!/usr/bin/env bash
# End-to-end run of OMEMO 2 through chord-cli against chat.foid.space (ejabberd).
#
# Accounts: chordag1a and chordag1b @chat.foid.space. Their passwords are in
# dev/foid/.env (gitignored): CHORDAG1A_PASSWORD and CHORDAG1B_PASSWORD.
# Each run uses new databases. So each run makes new OMEMO devices. The run removes the
# old devices of the two accounts first, and the devices of this run at the end.
#
# Steps:
#   1. A turns OMEMO on for B and sends. B reads the same text.
#   2. B turns OMEMO on for A and replies. A reads it.
#   3. A second device of A gets the carbon of a message of A, and reads it.
#   4. A and B restart (same databases). The next messages still decrypt.
#   5. B fetches the bundles of A (no presence subscription), and the device list is right.
#   6. With B distrusted, the send of A fails, and no plain text goes out. A fresh
#      database of B (no keys) reads the archive of the server: no text, only the
#      undecryptable marker.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

# A worktree has no .env (it is gitignored): take the one of the main checkout.
env_file="${CHORD_FOID_ENV:-dev/foid/.env}"
if [[ ! -f "$env_file" ]]; then
  main=$(git worktree list --porcelain | awk '/^worktree / { print $2; exit }')
  env_file="$main/dev/foid/.env"
fi
set -a
# shellcheck disable=SC1090
source "$env_file"
set +a
unset CHORD_SERVER # SRV lookup, as a real client does
# Parallel agents share target/. Build this checkout into its own directory, so the run
# tests this code and not a binary from another worktree.
export CARGO_TARGET_DIR="${CHORD_E2E_TARGET:-$PWD/target/omemo-live}"
cargo build -q -p chord-cli
cli="$CARGO_TARGET_DIR/debug/chord-cli"
work=$(mktemp -d)

A=chordag1a@chat.foid.space
B=chordag1b@chat.foid.space
# Passwords go in the environment of one process only, never in an argument.
a() { CHORD_JID=$A CHORD_PASSWORD="$CHORDAG1A_PASSWORD" CHORD_DB="$work/a.sqlite3" "$cli" "$@"; }
a2() { CHORD_JID=$A CHORD_PASSWORD="$CHORDAG1A_PASSWORD" CHORD_DB="$work/a2.sqlite3" "$cli" "$@"; }
b() { CHORD_JID=$B CHORD_PASSWORD="$CHORDAG1B_PASSWORD" CHORD_DB="$work/b.sqlite3" "$cli" "$@"; }
fresh() { CHORD_JID=$B CHORD_PASSWORD="$CHORDAG1B_PASSWORD" CHORD_DB="$work/fresh.sqlite3" "$cli" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
retry() {
  local tries=$1 i
  shift
  for ((i = 1; i <= tries; i++)); do
    if "$@"; then return 0; fi
    sleep 3
  done
  return 1
}
# item <text> <python expression>: evaluate the expression on the newest timeline item
# whose body contains the text.
item() {
  python3 -c "
import json, sys
for it in reversed(json.load(sys.stdin)):
    if sys.argv[1] in it['body']:
        print($2)
        break
" "$1"
}
# own_other_ids <command> <jid>: the ids of the devices of our account that are not this one.
own_other_ids() {
  "$1" omemo-refresh "$2" >/dev/null 2>&1 || true
  "$1" --json omemo-devices | python3 -c "
import json, sys
for d in json.load(sys.stdin):
    if not d['this_device']:
        print(d['device_id'])"
}
drop_others() {
  local id
  for id in $(own_other_ids "$1" "$2"); do
    "$1" omemo-remove-device "$id" >/dev/null 2>&1 || true
  done
}
this_id() {
  "$1" --offline --json omemo-devices | python3 -c "
import json, sys
print([d['device_id'] for d in json.load(sys.stdin) if d['this_device']][0])"
}

follower=
cleanup() {
  [[ -z "$follower" ]] || { kill "$follower" 2>/dev/null || true; wait "$follower" 2>/dev/null || true; }
  drop_others a "$A" 2>/dev/null || true
  drop_others b "$B" 2>/dev/null || true
  [[ -n "${KEEP_WORK:-}" ]] || rm -rf "$work"
}
trap cleanup EXIT

if pgrep -fl chord-cli >/dev/null; then
  echo "note: other chord-cli processes run (not touched):" >&2
  pgrep -fl chord-cli >&2 || true
fi
n=$RANDOM

echo "0. start: remove the old devices of A and B, and log the PEP setup"
# The first login publishes the device list and the bundle. Show any OMEMO warning.
CHORD_LOG=warn a login 2>&1 | grep -i "omemo\|pep\|pubsub" | grep -v "(XEP-0503)" >&2 || true
drop_others a "$A"
drop_others b "$B"
a_id=$(this_id a)
b_id=$(this_id b)

echo "1. A turns OMEMO on for B and sends. B reads the same text."
a omemo "$B" on | grep -q "on" || fail "OMEMO is not on for B"
t1="omemo $n one"
a send "$B" "$t1" >/dev/null || fail "A cannot send the first message"
has1() { [[ "$(b --json timeline "$A" --limit 20 | item "$t1" "it['encryption']")" == omemo ]]; }
retry 5 has1 || fail "B does not read the first message as OMEMO"
[[ "$(a --offline --json timeline "$B" --limit 20 | item "$t1" "it['encryption']")" == omemo ]] \
  || fail "A does not show its own message as encrypted"

echo "2. B turns OMEMO on for A and replies. A reads it."
b omemo "$A" on >/dev/null
t2="omemo $n two"
b send "$A" "$t2" >/dev/null || fail "B cannot reply"
has2() { [[ "$(a --json timeline "$B" --limit 20 | item "$t2" "it['encryption']")" == omemo ]]; }
retry 5 has2 || fail "A does not read the reply as OMEMO"

echo "3. a second device of A gets the carbon of a message of A"
a2 --wait 3 login >/dev/null
a omemo-refresh "$A" >/dev/null
ids=$(a --json omemo-devices | python3 -c "
import json, sys
print(sorted(d['device_id'] for d in json.load(sys.stdin)))")
a2_id=$(this_id a2)
[[ "$ids" == *"$a2_id"* ]] || fail "A does not list its second device ($ids)"
# The second device stays online, so the server sends it the carbon.
CHORD_JID=$A CHORD_PASSWORD="$CHORDAG1A_PASSWORD" CHORD_DB="$work/a2.sqlite3" \
  "$cli" timeline "$B" --follow >/dev/null 2>&1 &
follower=$!
sleep 8
t3="omemo $n carbon"
a send "$B" "$t3" >/dev/null || fail "A cannot send the carbon message"
sleep 8
kill "$follower" 2>/dev/null || true
wait "$follower" 2>/dev/null || true
follower=
got=$(a2 --offline --json timeline "$B" --limit 20 | item "$t3" "(it['encryption'], it['outgoing'])")
[[ "$got" == "('omemo', True)" ]] || fail "the second device did not read the carbon: $got"

echo "4. restart A and B (same databases): the next messages still decrypt"
t4="omemo $n after restart A to B"
a send "$B" "$t4" >/dev/null || fail "A cannot send after the restart"
has4() { [[ "$(b --json timeline "$A" --limit 20 | item "$t4" "it['encryption']")" == omemo ]]; }
retry 5 has4 || fail "B cannot read the message after the restart"
t5="omemo $n after restart B to A"
b send "$A" "$t5" >/dev/null || fail "B cannot send after the restart"
has5() { [[ "$(a --json timeline "$B" --limit 20 | item "$t5" "it['encryption']")" == omemo ]]; }
retry 5 has5 || fail "A cannot read the message after the restart"
[[ "$(this_id a)" == "$a_id" && "$(this_id b)" == "$b_id" ]] || fail "a device id changed"

echo "5. B fetches the bundles of A, and the device list has the ids"
# A and B are no contacts: the PEP nodes must be open (access_model=open).
b contacts | grep -qF "$A" && echo "note: A is a contact of B, so this is no open-access proof" >&2
b omemo-refresh "$A" >/dev/null || fail "B cannot fetch the devices of A"
view=$(b --json omemo-devices "$A")
a_view=$(python3 -c "
import json, sys
print(sorted(d['device_id'] for d in json.loads(sys.argv[1]) if d['active']))" "$view")
a_real=$(a --json omemo-devices | python3 -c "
import json, sys
print(sorted(d['device_id'] for d in json.load(sys.stdin) if d['active']))")
[[ "$a_view" == "$a_real" ]] || fail "the device list differs: B sees $a_view, A has $a_real"
python3 -c "
import json, sys
ds = [d for d in json.loads(sys.argv[1]) if d['active']]
assert ds and all(d['fingerprint'] for d in ds), 'a bundle is missing'" "$view" \
  || fail "B has no fingerprint (no bundle) for a device of A"

echo "6. B is distrusted: the send of A fails and no plain text goes out"
for fp in $(a --offline --json omemo-devices "$B" | python3 -c "
import json, sys
for d in json.load(sys.stdin):
    print(d['fingerprint'].replace(' ', ''))"); do
  a --offline omemo-trust "$B" "$fp" distrust >/dev/null
done
t6="omemo $n secret must not leak"
if a send "$B" "$t6" >"$work/send6.out" 2>"$work/send6.err"; then
  fail "the send to a distrusted device did not fail"
fi
grep -q "No trusted OMEMO device" "$work/send6.err" || fail "wrong error: $(cat "$work/send6.err")"
b --json timeline "$A" --limit 30 | grep -qF "$t6" && fail "B got the text of the failed send"
# What the server stores: a database with no keys reads the archive of B.
fresh --wait 3 login >/dev/null
archive() { fresh --json timeline "$A" --limit 50 > "$work/archive.json"; grep -qF "omemo $n one" "$work/archive.json" || grep -q undecryptable "$work/archive.json"; }
retry 5 archive || fail "the fresh database has no archived message"
grep -qF "$t6" "$work/archive.json" && fail "the server archive has the plain text of the failed send"
for t in "$t1" "$t2" "$t4"; do
  grep -qF "$t" "$work/archive.json" && fail "the server archive has the plain text: $t"
done
python3 -c "
import json
items = json.load(open('$work/archive.json'))
bad = [i for i in items if i['encryption'] == 'undecryptable']
assert bad, 'no undecryptable marker in the archive'
assert all(i['encryption'] != 'none' or 'omemo $n' not in i['body'] for i in items)" \
  || fail "the archive shows no undecryptable marker"

echo "OK: OMEMO end-to-end passed"
