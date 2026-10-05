<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/chord-lockup-reversed.svg">
  <img alt="Chord" src="docs/brand/chord-lockup.svg" width="218">
</picture>

Chat with spaces, channels, and DMs on XMPP.

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

## License

[MIT](LICENSE) <br>
[GIWTWM-PL](https://github.com/abbyfluoroethane/GIWTWMPL/blob/main/LICENSE.md)
