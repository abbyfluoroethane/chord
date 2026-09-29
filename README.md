# Chord

Chat with circles, channels, and DMs, on a network that nobody owns.

Chord looks like the chat app you already use. Circles are on the left, channels are next to them, and messages are in the middle. The only new thing is the network underneath: Chord runs on XMPP, an open standard. Your account works with any XMPP server, and no single company sits in the middle of your conversations.

- **One address, any server.** Your address looks like an email address, for example `mika@chat.example`. Talk to people on any XMPP server.
- **Circles, channels, and DMs.** Group channels into circles, and keep DMs one click away. Anyone can make a circle.
- **Your server, or ours.** Sign up on a public server, or run your own. Chord works the same on both.

Website: [bigaouette.com/chord-site](https://bigaouette.com/chord-site/) · [Features](https://bigaouette.com/chord-site/features) · [Style guide](https://bigaouette.com/chord-site/style-guide)

> **Still in progress.** The core works against real servers. The desktop app is in testing.

## What is in this repository

| Folder | What it is |
|---|---|
| `chord-core` | The Rust library that does everything except pixels: the connection, the XMPP features, local storage, and the views that a UI shows. |
| `chord-desktop` | The desktop app: Tauri 2, with a Svelte front end. |
| `chord-cli` | A command-line client on the core. Use it to test and to script. |
| `chord-ffi` | UniFFI bindings for Kotlin, for the Android app later. |
| `dev` | Scripts for checks and live tests. |

## Run the desktop app

You need Rust (stable), Node 22, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```sh
cd chord-desktop
npm ci
npx tauri dev
```

To make an app bundle, run `npx tauri build`. To see the UI with sample data in a browser, run `npm run dev` and open http://localhost:1420.

## Use the command-line client

The password comes from an environment variable, never from an argument.

```sh
export CHORD_JID=mika@chat.example
export CHORD_PASSWORD='…'
cargo run -p chord-cli -- send rin@other.example "bay 2, top drawer."
cargo run -p chord-cli -- --json timeline rin@other.example
```

Run `cargo run -p chord-cli` with no command to see the full list.

## Check your changes

```sh
./dev/quick-check.sh
```

This script runs the format check, clippy, and the unit tests. It builds into a shared `target/` and stops when less than 10 GB of disk is free. GitHub CI runs the full set: the wasm build, the Kotlin bindings, the desktop app, and the live tests against a Prosody server.

## Words we use

Chord uses one name for one thing, in the app and in these docs.

| Chord says | It means | Never say |
|---|---|---|
| circle | a group of channels (an XMPP space) | server, guild |
| channel | a group chat room | room, group |
| DM | a one-to-one chat | private message |
| server | the computer that holds an account | instance, homeserver |
| address | a JID, like `mika@chat.example` | handle, username |

## Standards

Chord speaks XMPP (RFC 6120 and 6121). The core supports these extensions:

| Area | XEPs |
|---|---|
| Circles and channels | 0503 spaces, 0045 multi-user chat, 0402 bookmarks, 0060 pubsub, 0462 and 0499 |
| History and devices | 0313 message archive, 0280 carbons, 0198 stream management |
| Messages | 0308 corrections, 0424 and 0425 retraction, 0444 reactions, 0461 replies, 0333 read markers, 0085 typing |
| Files and people | 0363 file upload, 0084 and 0153 avatars, 0191 blocking, 0357 push |

## License

Chord is open source under the [MIT License](LICENSE). The app ships IBM Plex and Bricolage Grotesque (SIL Open Font License) and Lucide icons (ISC).
