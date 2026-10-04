# Releasing

Each app has its own version and its own release repository. The source stays here. This
repository has tags but no GitHub releases.

| App | Tag here | Release repository |
| --- | --- | --- |
| Android | `android-v<version>` | [abbyfluoroethane/chord-android](https://github.com/abbyfluoroethane/chord-android/releases) |
| Desktop | `desktop-v<version>` | [abbyfluoroethane/chord-desktop](https://github.com/abbyfluoroethane/chord-desktop/releases) |
| iOS | `ios-v<version>` | [abbyfluoroethane/chord-iOS](https://github.com/abbyfluoroethane/chord-iOS/releases) (no app yet) |
| Nightlies of all apps | none | [abbyfluoroethane/chord-nightly](https://github.com/abbyfluoroethane/chord-nightly/releases) |

One repository per app means that "latest release" of each repository is always that app. An
Android fix never pushes the desktop build down the list.

## Versions

Versions are SemVer. Each app moves on its own: a fix in the Android app is a new Android
version and nothing else.

- A release is `MAJOR.MINOR.PATCH`, for example `0.3.0`. Its tag in the release repository is
  `v0.3.0`, and it becomes "Latest".
- A beta is `MAJOR.MINOR.PATCH-beta.N`, for example `0.3.0-beta.2`: the version that it leads
  to, and a counter. Its tag in the release repository is `v0.3.0-beta.2+<commit>`, and it is a
  pre-release, so "Latest" skips it.
- Betas of a version sort below the version: `0.3.0-beta.1` < `0.3.0-beta.2` < `0.3.0`.

The core (`chord-core`, `chord-ffi`, `chord-cli`) has no version of its own. A core change
reaches users in the next release of each app. The About page of each app shows the commit it
was built from, for example `0.3.0-beta.2 (09c83fb)`.

A build that is not a release takes the last tag of its app and adds `+dev`, for example
`0.3.0-beta.2+dev`. Without a tag it is `0.0.0+dev`.

### Build number

Each build has a build number from its commit time and its channel. On Android it is the
versionCode. The apps compare build numbers to find updates. See [updates.md](updates.md).

## Make a release

Commit and push first. The script refuses a dirty tree or a commit that is not on GitHub.

```sh
dev/release.sh android 0.3.0-beta.2 --dry-run   # build and show what would be published
dev/release.sh android 0.3.0-beta.2             # tag, build, publish
dev/release.sh desktop 0.3.0                    # tag; CI builds and publishes
```

The notes are the commits for that app since its last release. Pass `--notes <file>` to write
them yourself.

- **Android** builds on your machine, because the signing key lives there
  (`dev/android-keystore.sh`). The script publishes the arm64-v8a, x86_64 and universal APKs and
  a `SHA256SUMS` file.
- **Desktop** builds in `.github/workflows/release.yml` on Linux (AppImage, .deb, .rpm),
  Windows (NSIS installer) and macOS (.dmg). "Run workflow" on that workflow builds a version
  without publishing it.

## Nightlies

`.github/workflows/nightly.yml` runs each night at 08:00 UTC. When `main` changed since the last
nightly and CI passed on it, it builds Android and Linux, plus Windows and macOS on Sundays, and
publishes them to chord-nightly as `android-v0.3.1-nightly.20261004+<commit>` and so on. The
version is the next version after the app's last release tag. "Run workflow" can force a build
and ask for all desktop platforms. The last 14 nightlies of each app are kept.

## Update manifests

Each publish also writes the channel manifests that the in-app updaters read
(`dev/releases.py publish`, [updates.md](updates.md)). A release goes to the stable, beta and
nightly channels, a beta to beta and nightly, a nightly to nightly, each only if it is newer than
what the channel has.

## Setup

The release repositories hold a README, their releases and, for the apps, the channel
manifests. Nothing else.

Secrets of this repository:

- `CHORD_RELEASES_TOKEN`: a fine-grained personal access token with access to the four release
  repositories only (chord-android, chord-desktop, chord-iOS, chord-nightly), and the permission
  "Contents: read and write". The workflows need it to publish.
- `ANDROID_KEYSTORE_BASE64`, `CHORD_KEYSTORE_PASSWORD`, `CHORD_KEY_ALIAS`, `CHORD_KEY_PASSWORD`:
  the Android signing key, for nightlies. It is the same key as for releases, because a phone
  only updates from an APK with the same key. The original is `~/.config/chord/android-release.jks`
  on the release machine.
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the desktop updater key.
  The original is `~/.config/chord/tauri-updater.key` and `.password`; the public key is in
  `tauri.conf.json`.
- `CHORD_KLIPY_KEY`, for GIF search in desktop builds.

Keep an offline backup of both keys. If the Android key is lost, every user has to uninstall
to move to a new one. If the updater key is lost, installed desktop apps can never update again.

## Not done yet

- The macOS build is not signed or notarized, and the Windows installer is not signed. Users
  see a warning when they open them.
- The macOS build is for Apple silicon only.
- The Android APKs are signed with the test key. A phone can only update to an APK with the
  same key, so keep the key safe and back it up. Changing it later means that every user has to
  uninstall first.
