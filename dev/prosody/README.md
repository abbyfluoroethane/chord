# Chord test server

Prosody 13 (`prosodyim/prosody:13.0`) in Docker, with TLS certificates from a local
`mkcert` CA.

| Host | Role |
|------|------|
| `chord.localhost` | VirtualHost. Accounts `alice` and `bob`. |
| `rooms.chord.localhost` | MUC component |
| `pubsub.chord.localhost` | Pubsub component |
| `upload.chord.localhost` | HTTP file share component |

## Start on a fresh Mac

The commands use Homebrew. Run them from the repository root.

1. Install the tools:

   ```sh
   brew install colima docker docker-compose mkcert
   ```

2. Make the Docker CLI find the Compose plugin. Do this step only if
   `~/.docker/config.json` does not exist. Otherwise, add the same key to that file.

   ```sh
   mkdir -p ~/.docker
   echo '{"cliPluginsExtraDirs":["/opt/homebrew/lib/docker/cli-plugins"]}' > ~/.docker/config.json
   ```

3. Start the Docker VM:

   ```sh
   colima start
   ```

4. Install the local CA into the system trust store. macOS asks for your password.

   ```sh
   mkcert -install
   ```

5. Set the passwords for the test accounts:

   ```sh
   cp dev/prosody/.env.example dev/prosody/.env
   ```

   Then edit `dev/prosody/.env` and put a password in each line. Git ignores this file.

6. Start the server. The script creates the certificates and the accounts. You can run
   it again at any time.

   ```sh
   ./dev/prosody/setup.sh
   ```

7. Make sure that TLS works. Alice sends a message to bob:

   ```sh
   ./dev/send-demo.sh
   ```

   The last line must be `PASS: bob received the message`.

## Without the system trust store

If you cannot run `mkcert -install`, point Chord at the CA file instead. This
replaces the system trust store for that process only:

```sh
export SSL_CERT_FILE="$(mkcert -CAROOT)/rootCA.pem"
```

## Plain TCP

The server requires TLS (`c2s_require_encryption = true`). Chord supports plain TCP only
in a build with the `dev-insecure` feature:

```sh
cargo build -p chord-cli --features dev-insecure
```

Against this server, a plain TCP login fails with `no common SASL mechanism`, because
Prosody offers no SASL mechanism without TLS. Do not use `dev-insecure` in `chord-ffi` or
in a release build.

## Other commands

| Task | Command |
|------|---------|
| Show the server log | `docker compose -f dev/prosody/docker-compose.yml logs -f` |
| Stop the server | `docker compose -f dev/prosody/docker-compose.yml down` |
| Delete all server data | `docker compose -f dev/prosody/docker-compose.yml down -v` |
| Check the pubsub features | `./dev/prosody/disco-pubsub.sh` |
| Stop the Docker VM | `colima stop` |

## SASL

`password_hash = "SHA-256"` makes Prosody offer SCRAM-SHA-256 and SCRAM-SHA-256-PLUS.
With TLS 1.3, Chord logs in with `SCRAM-SHA-256-PLUS`. To see the mechanism, set
`CHORD_LOG=info`.
