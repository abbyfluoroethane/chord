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

# Self-signed certificates for chord.localhost and its components.
mkdir -p certs
for host in chord.localhost rooms.chord.localhost pubsub.chord.localhost upload.chord.localhost; do
  if [[ ! -f "certs/$host.crt" ]]; then
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
      -subj "/CN=$host" \
      -keyout "certs/$host.key" -out "certs/$host.crt" 2>/dev/null
  fi
done
chmod 644 certs/*

docker compose up -d

# Wait until prosodyctl can talk to the running server.
for _ in $(seq 1 30); do
  if docker compose exec -T prosody prosodyctl status >/dev/null 2>&1; then break; fi
  sleep 1
done

register() {
  local user=$1 pass=$2
  if docker compose exec -T prosody prosodyctl shell user list chord.localhost 2>/dev/null | grep -q "^$user@"; then
    echo "$user@chord.localhost exists"
  else
    docker compose exec -T prosody prosodyctl register "$user" chord.localhost "$pass"
    echo "$user@chord.localhost created"
  fi
}
register alice "$ALICE_PASSWORD"
register bob "$BOB_PASSWORD"
