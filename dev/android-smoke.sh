#!/usr/bin/env bash
# Android smoke test, driven with adb: install the debug APK, sign in, open a room, send a
# message, and check that a message from a second account arrives. The script sends the
# second message with chord-cli.
#
# Usage: dev/android-smoke.sh [--server foid|prosody|<server string>] [--no-install]
#
# Servers:
#   foid     (default) chat.foid.space, a real server with a real certificate. The
#            emulator trusts the certificate. The server field stays empty (SRV lookup).
#            Accounts chordtest and chordtest2, or the CHORD_SMOKE_JID pair below.
#            Passwords: dev/foid/.env.
#   prosody  The local Docker server (dev/prosody/). The server field is
#            starttls://10.0.2.2:5222 (10.0.2.2 is the host, seen from the emulator).
#            Android does not trust the mkcert CA, so the sign-in fails until you
#            install the CA in the emulator and the app accepts it. Prefer foid.
#   other    Any string is the text for the server field. Set CHORD_SMOKE_JID,
#            CHORD_SMOKE_PASSWORD, CHORD_SMOKE_JID2, CHORD_SMOKE_PASSWORD2 and
#            CHORD_SMOKE_ROOM yourself.
# CHORD_SMOKE_ROOM sets the room JID. It must exist, or the server must create it on join.
#
# THE TEST TAG CONTRACT. The UI sets testTagsAsResourceId = true at its root, so each
# Modifier.testTag shows as a resource-id in uiautomator. This script needs these tags:
#   login_jid, login_password, login_submit                 the sign-in form
#   login_advanced                                          the "Advanced" expander. The field login_server
#                                                           is in the tree only after a tap on the expander
#   channel_list                                            the list of channels
#   channel_item_<bare jid>                                 one row per channel
#   timeline                                                the message list of a room
#   message_row                                             one row per message
#   composer_input, composer_send                           the message field and button
#   drawer_open_channels                                    the button that opens the channel list
# A message_row must carry the message text, in its own text or in a child.
#
# Needs: adb and a running emulator (dev/android-emulator.sh), python3, cargo.
# The script runs itself in the chord-android toolbox when it starts on the host.
set -euo pipefail
cd "$(dirname "$0")/.."

server_choice=foid
install=1
while (( $# )); do
  case "$1" in
    --server) server_choice=${2:?--server needs a value}; shift 2 ;;
    --no-install) install=0; shift ;;
    *) echo "usage: $0 [--server foid|prosody|<server string>] [--no-install]" >&2; exit 2 ;;
  esac
done

if [[ ! -f /run/.containerenv ]] || ! grep -q 'name="chord-android"' /run/.containerenv; then
  exec toolbox run -c chord-android bash "$PWD/dev/android-smoke.sh" "$@"
fi
# shellcheck disable=SC1091
. /etc/chord-android.sh
export PATH="$HOME/.cargo/bin:$PATH"

fail() { echo "FAIL: $*" >&2; exit 1; }

# The .env files are not in a git worktree. Look in this checkout, then in the main one.
main=$(git worktree list --porcelain | awk '/^worktree / { print $2; exit }')
load_env() {
  local f
  for f in "$PWD/$1" "$main/$1"; do
    if [[ -f "$f" ]]; then
      set -a
      # shellcheck disable=SC1090
      source "$f"
      set +a
      return 0
    fi
  done
  fail "$1 not found. Create it (see its README). It holds the passwords and is not in git."
}

n=$RANDOM
case "$server_choice" in
  foid)
    load_env dev/foid/.env
    server_field=""
    # CHORD_SMOKE_JID, CHORD_SMOKE_PASSWORD and the "2" pair select other accounts.
    jid=${CHORD_SMOKE_JID:-chordtest@chat.foid.space}
    password=${CHORD_SMOKE_PASSWORD:-${CHORDTEST_PASSWORD:?CHORDTEST_PASSWORD is not set in dev/foid/.env}}
    jid2=${CHORD_SMOKE_JID2:-chordtest2@chat.foid.space}
    password2=${CHORD_SMOKE_PASSWORD2:-${CHORDTEST2_PASSWORD:?CHORDTEST2_PASSWORD is not set in dev/foid/.env}}
    room=${CHORD_SMOKE_ROOM:-chord-smoke@conference.chat.foid.space}
    cli_server=""
    ;;
  prosody)
    load_env dev/prosody/.env
    server_field="starttls://10.0.2.2:5222"
    jid=alice@chord.localhost
    password=${ALICE_PASSWORD:?ALICE_PASSWORD is not set in dev/prosody/.env}
    jid2=bob@chord.localhost
    password2=${BOB_PASSWORD:?BOB_PASSWORD is not set in dev/prosody/.env}
    room=${CHORD_SMOKE_ROOM:-smoke@rooms.chord.localhost}
    cli_server="starttls://localhost:5222"
    ;;
  *)
    server_field=$server_choice
    jid=${CHORD_SMOKE_JID:?set CHORD_SMOKE_JID}
    password=${CHORD_SMOKE_PASSWORD:?set CHORD_SMOKE_PASSWORD}
    jid2=${CHORD_SMOKE_JID2:?set CHORD_SMOKE_JID2}
    password2=${CHORD_SMOKE_PASSWORD2:?set CHORD_SMOKE_PASSWORD2}
    room=${CHORD_SMOKE_ROOM:?set CHORD_SMOKE_ROOM}
    cli_server=${server_field/10.0.2.2/localhost}
    ;;
esac

serial=$(adb devices | awk '/^emulator-[0-9]+\tdevice$/ { print $1; exit }')
[[ -n "$serial" ]] || fail "no emulator is running. Run dev/android-emulator.sh."
adb_() { adb -s "$serial" "$@"; }

# The second account: a chord-cli process with its own database.
export CARGO_TARGET_DIR="${CHORD_SMOKE_TARGET:-$PWD/target/smoke}"
cargo build -q -p chord-cli
cli="$CARGO_TARGET_DIR/debug/chord-cli"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cli2() {
  if [[ -n "$cli_server" ]]; then export CHORD_SERVER=$cli_server; else unset CHORD_SERVER; fi
  CHORD_JID=$jid2 CHORD_PASSWORD=$password2 CHORD_DB="$work/second.sqlite3" "$cli" "$@"
}
cli1() {
  if [[ -n "$cli_server" ]]; then export CHORD_SERVER=$cli_server; else unset CHORD_SERVER; fi
  CHORD_JID=$jid CHORD_PASSWORD=$password CHORD_DB="$work/first.sqlite3" "$cli" "$@"
}

# ui_find <tag> [text]: print "x y" of the center of the first node with this resource-id.
# With text, the node or one of its children must contain the text (any case). Exit 1 if none.
ui_find() {
  adb_ shell uiautomator dump /sdcard/chord-ui.xml >/dev/null 2>&1 || return 1
  adb_ pull /sdcard/chord-ui.xml "$work/ui.xml" >/dev/null 2>&1 || return 1
  python3 - "$work/ui.xml" "$1" "${2:-}" <<'PY'
import re, sys
import xml.etree.ElementTree as ET
path, tag, text = sys.argv[1:4]
def label(node):
    return " ".join(n.get("text", "") + " " + n.get("content-desc", "") for n in node.iter())
for node in ET.parse(path).iter("node"):
    rid = node.get("resource-id", "")
    if rid != tag and not rid.endswith("/" + tag):
        continue
    # Ignore case: the keyboard may capitalise the first letter of a typed message.
    if text and text.lower() not in label(node).lower():
        continue
    x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.get("bounds")))
    print((x1 + x2) // 2, (y1 + y2) // 2)
    sys.exit(0)
sys.exit(1)
PY
}

# wait_for <seconds> <tag> [text]: wait until the node exists. Fail with a clear message.
wait_for() {
  local secs=$1 tag=$2 text=${3:-} i
  for ((i = 0; i < secs; i++)); do
    if ui_find "$tag" "$text" >/dev/null; then return 0; fi
    sleep 1
  done
  adb_ exec-out screencap -p >"${TMPDIR:-/tmp}/chord-smoke-fail.png" 2>/dev/null || true
  fail "no node with the test tag '$tag'${text:+ and text "$text"} after $secs s. Screenshot: ${TMPDIR:-/tmp}/chord-smoke-fail.png"
}

tap() {
  local xy
  xy=$(ui_find "$1" "${2:-}") || fail "cannot tap: no node with the test tag '$1'"
  # shellcheck disable=SC2086
  adb_ shell input tap $xy
}

# type_into <tag> <text>: focus the field and type. Spaces need %s for adb input.
type_into() {
  tap "$1"
  sleep 0.3
  adb_ shell input text "${2// /%s}"
}

echo "1. install the debug APK"
if (( install )); then
  apk=$(find chord-android/app/build/outputs/apk/debug -name '*.apk' 2>/dev/null | head -1)
  [[ -n "$apk" ]] || fail "no debug APK. Run dev/android-check.sh or ./gradlew assembleDebug."
  adb_ install -r "$apk" >/dev/null
fi
adb_ shell am force-stop space.foid.chord
adb_ shell pm clear space.foid.chord >/dev/null
# Grant the notification permission (API 33+) first. Its system dialog covers the app.
adb_ shell pm grant space.foid.chord android.permission.POST_NOTIFICATIONS 2>/dev/null || true
adb_ shell monkey -p space.foid.chord -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1

echo "2. both accounts join $room"
cli1 join "$room" >/dev/null
cli2 join "$room" >/dev/null

echo "3. sign in as $jid"
wait_for 30 login_jid
type_into login_jid "$jid"
type_into login_password "$password"
if [[ -n "$server_field" ]]; then
  tap login_advanced # the server field is hidden until the expander opens
  sleep 0.5
  type_into login_server "$server_field"
fi
adb_ shell input keyevent KEYCODE_BACK # close the keyboard
tap login_submit

echo "4. open the room"
wait_for 60 channel_list
# A phone layout can hide the channel list behind the drawer button.
if ! ui_find "channel_item_$room" >/dev/null && ui_find drawer_open_channels >/dev/null; then
  tap drawer_open_channels
fi
wait_for 60 "channel_item_$room"
tap "channel_item_$room"
wait_for 20 timeline

echo "5. send a message from the app"
sent="smoke app $n"
type_into composer_input "$sent"
tap composer_send
wait_for 20 message_row "$sent"
# The server delivers after a short time. Ignore case: the keyboard may capitalise.
for ((i = 0; i < 10; i++)); do
  if cli2 timeline "$room" --limit 10 | grep -qiF "$sent"; then break; fi
  (( i < 9 )) || fail "the second account does not see the message '$sent'"
  sleep 2
done

echo "6. the second account sends, and the app shows the message"
received="smoke cli $n"
cli2 send "$room" "$received" >/dev/null
wait_for 30 message_row "$received"

echo "PASS: Android smoke test ($server_choice)"
