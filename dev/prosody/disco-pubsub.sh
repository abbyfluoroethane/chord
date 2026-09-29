#!/usr/bin/env bash
# Send disco#info to pubsub.chord.localhost as alice, through prosodyctl shell.
# Prints the reply stanza.
set -euo pipefail
cd "$(dirname "$0")"
docker compose exec -T prosody prosodyctl shell <<'LUA' | sed -n '/Result:/,/<\/iq>/p'
> local st=require"prosody.util.stanza"; local out; local origin={type="c2s", username="alice", host="chord.localhost", full_jid="alice@chord.localhost/probe", send=function(s) out=s end}; local iq=st.iq({type="get",from="alice@chord.localhost/probe",to="pubsub.chord.localhost",id="d1"}):query("http://jabber.org/protocol/disco#info"); prosody.hosts["pubsub.chord.localhost"].events.fire_event("iq-get/host/http://jabber.org/protocol/disco#info:query", {origin=origin, stanza=iq}); return tostring(out and out:indent(1,"  "))
LUA
