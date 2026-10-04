# Releasing

Each app has its own version and its own release repository. The source stays here. This
repository has tags but no GitHub releases.

| App | Tag here | Release repository |
| --- | --- | --- |
| Android | `android-v<version>` | [abbyfluoroethane/chord-android](https://github.com/abbyfluoroethane/chord-android/releases) |
| Desktop | `desktop-v<version>` | [abbyfluoroethane/chord-desktop](https://github.com/abbyfluoroethane/chord-desktop/releases) |
| iOS | `ios-v<version>` | [abbyfluoroethane/chord-iOS](https://github.com/abbyfluoroethane/chord-iOS/releases) (no app yet) |

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

### Android versionCode

Android updates an app only to a higher versionCode, so the code comes from the version:

```
major * 100,000,000 + minor * 1,000,000 + patch * 1,000 + (N for beta N, 999 for a release)
```

`0.3.0-beta.2` is 3,000,002 and `0.3.0` is 3,000,999. The limits are major 20, minor 99,
patch 999 and beta 998. `chord-android/app/build.gradle.kts` checks them.

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

## Setup

- The release repositories hold a README and their releases. Nothing else.
- `RELEASES_TOKEN`, a secret of this repository: a fine-grained personal access token with
  access to the three release repositories only, and the permission "Contents: read and write".
  The release workflow needs it to publish the desktop app.
- `CHORD_KLIPY_KEY`, a secret of this repository, for GIF search in desktop builds.

## Not done yet

- The macOS build is not signed or notarized, and the Windows installer is not signed. Users
  see a warning when they open them.
- The desktop app has its updater (`src-tauri/src/updates.rs`, see [updates.md](updates.md)),
  but the release workflow does not write the channel manifests yet. `tauri build` now makes
  the updater files, so it needs the `TAURI_SIGNING_PRIVATE_KEY` secret. Build with the plain
  version (`0.3.0-beta.2`): the signature names that version, and the app compares it with
  the manifest version without its `+b<build>`.
- The Android APKs are signed with the test key. A phone can only update to an APK with the
  same key, so keep the key safe and back it up. Changing it later means that every user has to
  uninstall first.
