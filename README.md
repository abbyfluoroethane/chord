<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/chord-lockup-reversed.svg">
  <img alt="Chord" src="docs/brand/chord-lockup.svg" width="218">
</picture>

Chat with spaces, channels, and DMs, on XMPP. Your account works with any server, and no company owns the network.

[Website](https://bigaouette.com/chord-site/) · [Features](https://bigaouette.com/chord-site/features) · [Style guide](https://bigaouette.com/chord-site/style-guide)

Still in progress.

## Run

```sh
cd chord-desktop
npm ci
npx tauri dev
```

You need Rust, Node 22, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

## Repository

| Folder | Contents |
|---|---|
| `chord-core` | Connection, XMPP features, storage, views |
| `chord-desktop` | Desktop app (Tauri and Svelte) |
| `chord-cli` | Command-line client |
| `chord-ffi` | Kotlin bindings |
| `chord-android` | Android app (Kotlin and Compose) |

Check changes with `./dev/quick-check.sh`.

## License

[MIT](LICENSE)
