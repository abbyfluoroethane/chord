#!/usr/bin/env bash
# Alice sends a message to bob. Bob receives it in a second chord-cli process.
# Before you run it: start the test server with dev/prosody/setup.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

set -a
# shellcheck disable=SC1091
source dev/prosody/.env
set +a
export CHORD_SERVER=starttls://localhost:5222

# rustup installs cargo here. Your shell PATH can omit it.
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -q -p chord-cli
cli=target/debug/chord-cli
text="${1:-hello bob, the time is $(date +%H:%M:%S)}"
bob_log=$(mktemp)
trap 'rm -f "$bob_log"' EXIT

# 1. Bob listens. Other messages can arrive first (for example messages that the server
# stored while bob was offline), so the script waits for alice's exact text.
CHORD_JID=bob@chord.localhost CHORD_PASSWORD="$BOB_PASSWORD" "$cli" listen >"$bob_log" 2>&1 &
bob=$!
for _ in $(seq 1 150); do
  grep -q '^listening' "$bob_log" && break
  kill -0 "$bob" 2>/dev/null || break
  sleep 0.1
done
echo "[bob]   $(head -1 "$bob_log")"

# 2. Alice logs in and sends.
CHORD_JID=alice@chord.localhost CHORD_PASSWORD="$ALICE_PASSWORD" "$cli" login | sed 's/^/[alice] /'
CHORD_JID=alice@chord.localhost CHORD_PASSWORD="$ALICE_PASSWORD" "$cli" send bob@chord.localhost "$text" | sed 's/^/[alice] /'

# 3. Wait until bob prints the message. Stop after 15 seconds.
found=no
for _ in $(seq 1 150); do
  if grep -qF "$text" "$bob_log"; then found=yes; break; fi
  sleep 0.1
done
kill "$bob" 2>/dev/null || true
wait "$bob" 2>/dev/null || true
tail -n +2 "$bob_log" | sed 's/^/[bob]   /'
if [[ "$found" != yes ]]; then
  echo "FAIL: bob did not receive the message in 15 seconds" >&2
  exit 1
fi
echo "PASS: bob received the message"
