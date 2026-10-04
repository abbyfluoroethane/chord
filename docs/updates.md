# Updates

The Android app and the desktop app on Windows and macOS update themselves from GitHub. Settings
lets the user pick a channel. On Linux the desktop app is a Flatpak only, and it updates through
Flatpak (see "Flatpak" below). iOS cannot update itself: there the App Store is the stable channel and TestFlight is
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
(see below). The platforms update on their own: a nightly that built only on Windows leaves the
macOS manifest as it was. The Linux app does not read these manifests (see "Flatpak").

The update signature uses the updater key: its public key is in `tauri.conf.json`, its private key
is the `TAURI_SIGNING_PRIVATE_KEY` secret, with a backup in `~/.config/chord/` on the release
machine. Without that key, installed apps can never update again.

## Flatpak

On Linux, Chord ships only as a Flatpak, app ID `space.foid.chord`. The Flatpak branch is the
channel:

| Channel | Ref | Remote |
| --- | --- | --- |
| Stable | `space.foid.chord//stable` | `flathub` |
| Beta | `space.foid.chord//beta` | `flathub-beta` |
| Nightly | `space.foid.chord//nightly` | `chord-nightly`, https://bigaouette.com/chord-nightly/flatpak/ |

Until Chord is on Flathub, the `chord-nightly` repository can also carry the `stable` and
`beta` branches.

The app reads its branch from `/.flatpak-info` (`[Instance] branch`; also `arch`,
`app-commit` and `flatpak-version`). That file has no origin: the app cannot know the remote.
A sandbox with `[Instance] build=true` (a `flatpak-builder --run`) has no updater. A Linux build
outside Flatpak (a dev build) has no updater either.

The updates go through the Flatpak portal, `org.freedesktop.portal.Flatpak` version 2 or newer
(`src-tauri/src/flatpak.rs`, with the `ashpd` crate). The Tauri updater is not used on Linux.

- `CreateUpdateMonitor` gives an update monitor. The portal looks for updates on its own, about
  twice an hour and not on a metered network, and sends `UpdateAvailable` with the running,
  installed and remote commits. "Check for updates" reports what the monitor said last. The
  portal gives no version text: the app says "A new version is available".
- When the installed commit is newer than the running one (the system already updated the app),
  the app offers only "Restart to update".
- `Update` installs the update. The `Progress` signal gives the step and its percent, then
  `done`, `empty` (nothing to do) or `failed`. The portal may ask the user once whether Chord can
  update itself.
- An update that adds permissions fails ("new version requires new permissions"). The app then
  says "This update needs new permissions. Update Chord in your software app." Other errors: no
  network, update not allowed in the privacy settings, or an install with no remote.
- "Restart to update" calls `Spawn` with the latest-version flag
  (`FLATPAK_SPAWN_FLAGS_LATEST_VERSION`), which starts the newest installed commit of the branch,
  then the app quits. `AppHandle::restart` would start the old deployment again.

**Change channel.** The app cannot switch its branch. Settings shows the one-time command:

```sh
# Stable
flatpak install flathub space.foid.chord//stable
# Beta
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo && flatpak install flathub-beta space.foid.chord//beta
# Nightly
flatpak remote-add --if-not-exists chord-nightly https://bigaouette.com/chord-nightly/flatpak/chord-nightly.flatpakrepo && flatpak install chord-nightly space.foid.chord//nightly
```

Several branches can be installed side by side. They share the data in
`~/.var/app/space.foid.chord`. `flatpak make-current space.foid.chord <branch>` picks the branch
that `flatpak run` and the desktop entry start. An older branch may not read a database that a
newer one has changed (see "No downgrades").

## In the apps

- The channel setting is in Settings → About, under "Updates". The default is the channel of the
  installed build. Stable, Beta, Nightly. In a Flatpak the setting shows the installed branch,
  and a pick shows the command that installs the other branch, with a Copy button.
- The app checks once a day and when the user taps "Check for updates". Automatic checks can be
  turned off. In a Flatpak the portal checks by itself and tells the app at once; with automatic
  checks off, the app shows that news only after "Check for updates".
- When an update is ready, the app says so: a notification on Android, a banner on the desktop.
  Nothing installs until the user says so. On the desktop "Restart to update" downloads the update
  and starts the new version.
- The About page shows the version, the commit, and the channel the build came from. In a
  Flatpak it shows the branch, for example "Beta (Flatpak)".
- The browser preview of the desktop app shows the Flatpak state with `?flatpak` in the URL.
