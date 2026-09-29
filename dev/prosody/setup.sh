#!/usr/bin/env bash
# Start the Chord test server and create the test accounts.
# Run it again at any time. It does not change accounts that exist.
set -euo pipefail
cd "$(dirname "$0")"

if [[ ! -f .env ]]; then
  echo "Missing dev/prosody/.env. Copy .env.example to .env and set the passwords." >&2
  exit 1
fi
# shellcheck disable=SC1091
source .env

# Certificates from the local mkcert CA. See README.md.
if ! command -v mkcert >/dev/null; then
  echo "mkcert is not installed. See dev/prosody/README.md." >&2
  exit 1
fi
mkdir -p certs
if [[ ! -f certs/chord.localhost.crt ]]; then
  mkcert -cert-file certs/chord.localhost.crt -key-file certs/chord.localhost.key \
    chord.localhost rooms.chord.localhost pubsub.chord.localhost upload.chord.localhost
fi
# Prosody looks for <host>.crt and <host>.key. One certificate covers all four hosts.
for host in rooms pubsub upload; do
  cp certs/chord.localhost.crt "certs/$host.chord.localhost.crt"
  cp certs/chord.localhost.key "certs/$host.chord.localhost.key"
done
chmod 644 certs/*

docker compose up -d

# Wait until prosodyctl can talk to the running server.
for _ in $(seq 1 30); do
  if docker compose exec -T prosody prosodyctl status >/dev/null 2>&1; then break; fi
  sleep 1
done

# Create each account, or set its password again. Setting it again also re-hashes it
# with the current password_hash (SHA-256).
register() {
  local user=$1 pass=$2
  if docker compose exec -T prosody prosodyctl shell user list chord.localhost 2>/dev/null | grep -q "^$user@"; then
    docker compose exec -T prosody prosodyctl shell user password "$user@chord.localhost" "$pass" >/dev/null
    echo "$user@chord.localhost exists, password set again"
  else
    docker compose exec -T prosody prosodyctl register "$user" chord.localhost "$pass"
    echo "$user@chord.localhost created"
  fi
}
register alice "$ALICE_PASSWORD"
register bob "$BOB_PASSWORD"
