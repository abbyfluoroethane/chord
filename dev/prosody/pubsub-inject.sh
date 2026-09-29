#!/usr/bin/env bash
# Inject one pubsub IQ into pubsub.chord.localhost as <from-full-jid>, and print the reply.
# Usage: dev/prosody/pubsub-inject.sh <from-full-jid> < request.xml
#
# Method and limit: see dev/prosody/notes/owner-subscriptions.md. The stanza goes to the
# pubsub event handlers through `prosodyctl shell`, not over a client connection.
set -euo pipefail
cd "$(dirname "$0")"
from=$1
user=${from%%@*}
rest=${from#*@}
host=${rest%%/*}
# One line, because prosodyctl shell runs each input line as one command.
xml=$(tr '\n' ' ')
docker compose exec -T prosody prosodyctl shell <<LUA | sed -n '/Result:/,$p' | sed '1s/^| Result: //; /^prosody>/d'
> local xml = require"prosody.util.xml"; local out = {}; local origin = { type = "c2s", username = "$user", host = "$host", full_jid = "$from", send = function(s) out[#out+1] = tostring(s) end }; local stanza = xml.parse([==[$xml]==]); local ns = stanza.tags[1].attr.xmlns; prosody.hosts["pubsub.chord.localhost"].events.fire_event("iq/host/"..ns..":pubsub", { origin = origin, stanza = stanza }); return table.concat(out, "\n")
LUA
