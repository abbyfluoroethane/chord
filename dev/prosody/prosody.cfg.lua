-- Prosody 13 configuration for the Chord test server.
-- Local development only. Do not use this configuration in production.

admins = { }

modules_enabled = {
	"disco";
	"roster";
	"saslauth";
	"tls";
	"blocklist";
	"bookmarks";
	"carbons";      -- XEP-0280
	"mam";          -- XEP-0313
	"smacks";       -- XEP-0198
	"pep";
	"private";
	"vcard4";
	"vcard_legacy";
	"ping";
	"time";
	"uptime";
	"version";
	"admin_shell";  -- prosodyctl shell
}

authentication = "internal_hashed"
-- Store SCRAM-SHA-256 hashes, so the server offers SCRAM-SHA-256(-PLUS). Existing
-- accounts keep their old hash until the password is set again (setup.sh does that).
password_hash = "SHA-256"
storage = "internal"

-- Log to the console, so that "docker compose logs" shows the log.
log = { debug = "*console" }

-- TLS certificates from the local mkcert CA. setup.sh creates them in ./certs.
certificates = "certs"

-- Clients must use TLS (the Prosody default). Chord has plain TCP only behind its
-- dev-insecure feature, and that fails against this server.
c2s_require_encryption = true

archive_expires_after = "1w"

http_ports = { 5280 }
http_interfaces = { "*" }
https_ports = { }

VirtualHost "chord.localhost"

Component "rooms.chord.localhost" "muc"
	modules_enabled = { "muc_mam" }

Component "pubsub.chord.localhost" "pubsub"
	-- Users on chord.localhost get the prosody:registered role on this component.
	add_permissions = {
		["prosody:registered"] = { "pubsub:create-node" };
	}

Component "upload.chord.localhost" "http_file_share"
	http_host = "localhost"
	http_external_url = "http://localhost:5280/"
