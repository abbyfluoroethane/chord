# Updates

The Android and desktop apps update themselves from GitHub. Settings lets the user pick a
channel. iOS cannot update itself: there the App Store is the stable channel and TestFlight is
the beta, and the app has no setting for it.

## Channels

| Channel | What it gets | Built by |
| --- | --- | --- |
| Stable | Releases, `0.3.0` | `dev/release.sh` (Android), `release.yml` (desktop) |
| Beta | Betas and releases, `0.3.0-beta.2` | the same |
| Nightly | Everything, plus a build of `main` each night that CI passed, `0.3.0-nightly.20261004` | `nightly.yml` |

A channel gets the newest build of its own kind or of any slower channel: a beta user gets
`0.3.0` when it is newer than the last beta. The publish step does this. It writes the new build
to its own channel and to every faster channel whose build is older.

**No downgrades.** The app installs an update only if its build number is higher than the
installed one. A user who moves from Nightly to Stable keeps the nightly build until Stable has
a newer one. Android cannot install an older versionCode without wiping the app's data, and an
older build may not read a database that a newer one has changed.

## Build number

```
build = (minutes from 2026-01-01T00:00Z to the commit time) * 4 + slot
slot  = 2 for a release, 1 for a beta, 0 for a nightly or a dev build
```

A newer commit always has a higher number, in any channel. A release made from the same commit
as a beta or a nightly is still an update. On Android the build number is the versionCode. The
rule is in `chord-android/app/build.gradle.kts` (`BuildConfig.VERSION_CODE`) and in
`chord-desktop/src-tauri/build.rs` (`env!("CHORD_BUILD")`). Each build also knows its channel:
`BuildConfig.CHANNEL` and `env!("CHORD_CHANNEL")`, one of `stable`, `beta`, `nightly`, `dev`.

A `dev` build (any build that is not a release, such as a debug build) does not update itself.
An Android build made with `-Pchord.updater=false` (`BuildConfig.UPDATER`), for a store that
updates apps itself, has no updater either.

## Manifests

Each channel has a manifest in the default branch (`main`) of the app's release repository, read
from `https://raw.githubusercontent.com/abbyfluoroethane/<repository>/main/<path>`. GitHub
caches these files for about 5 minutes. A missing manifest (HTTP 404) means that the channel has
no build yet: no update.

### Android: `chord-android`, `channels/<channel>.json`

```json
{
  "version": "0.3.0-beta.2",
  "build": 1612345,
  "channel": "beta",
  "commit": "09c83fb",
  "pub_date": "2026-10-04T06:00:00Z",
  "release_url": "https://github.com/abbyfluoroethane/chord-android/releases/tag/v0.3.0-beta.2%2B09c83fb",
  "notes": "Markdown",
  "apks": {
    "arm64-v8a": { "url": "https://…/chord-android-0.3.0-beta.2-arm64-v8a.apk", "sha256": "…", "size": 17756806 },
    "x86_64": { "url": "…", "sha256": "…", "size": 19919549 },
    "universal": { "url": "…", "sha256": "…", "size": 35075532 }
  }
}
```

The app takes the APK of the first entry of `Build.SUPPORTED_ABIS` that the manifest has, else
`universal`. It checks the SHA-256 before it installs. Android itself refuses an APK that is not
signed with the same key.

### Desktop: `chord-desktop`, `channels/<channel>/<target>-<arch>.json`

`<target>` is `linux`, `windows` or `darwin`, and `<arch>` is `x86_64` or `aarch64`: the
`{{target}}` and `{{arch}}` of the Tauri updater. The file is a Tauri updater response with a few
more fields:

```json
{
  "version": "0.3.0-beta.2+b1612345",
  "build": 1612345,
  "channel": "beta",
  "commit": "09c83fb",
  "pub_date": "2026-10-04T06:00:00Z",
  "release_url": "https://github.com/abbyfluoroethane/chord-desktop/releases/tag/…",
  "notes": "Markdown",
  "url": "https://…/Chord_0.3.0-beta.2_amd64.AppImage",
  "signature": "the text of the .sig file"
}
```

`version` carries the build number as `+b<build>`, so the updater can compare build numbers
(see below). On Linux the update is the AppImage. A user who installed the .deb or .rpm gets a
link to the release page instead of an install button. The platforms update on their own: a
nightly that built only on Linux leaves the macOS manifest as it was.

The update signature uses the updater key: its public key is in `tauri.conf.json`, its private key
is the `TAURI_SIGNING_PRIVATE_KEY` secret, with a backup in `~/.config/chord/` on the release
machine. Without that key, installed apps can never update again.

## In the apps

- The channel setting is in Settings → About, under "Updates". The default is the channel of the
  installed build. Stable, Beta, Nightly.
- The app checks once a day and when the user taps "Check for updates". Automatic checks can be
  turned off.
- When an update is ready, the app says so: a notification on Android, a banner on the desktop.
  Nothing installs until the user says so.
- The About page shows the version, the commit, and the channel the build came from.
