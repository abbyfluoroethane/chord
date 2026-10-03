# Chord for Android

The Android app for Chord. It uses Kotlin and Compose. The Rust core (`chord-core`) runs in the app through the `chord-ffi` bindings.

## Set up

You need a Fedora host with Toolbox. The script makes a toolbox with all tools.

1. Run `./dev/toolbox-android.sh` from the repository root.
2. Wait for `OK: the chord-android toolbox is ready`.

The script installs Temurin JDK 21, the C toolchain, Rust with the Android targets, cargo-ndk, the Android SDK, the NDK 29.0.14206865, and the emulator image. It makes the AVD `chord-api36`.

You can run the script again. It skips the steps that are done.

To work in the toolbox, run `toolbox enter chord-android`. The shell loads `/etc/chord-android.sh` there.

## Build

1. Enter the toolbox.
2. Go to `chord-android`.
3. Run `./gradlew assembleDebug`.

The build runs cargo-ndk for the Rust core. The APK is in `app/build/outputs/apk/debug/`.

To build for the emulator only, add `-Pchord.abis=x86_64`. The build is then faster.

### The prebuilt shortcut

The Rust build is slow. For UI work, build the core one time and reuse it.

1. Run `./gradlew exportPrebuilt`. It writes `../target/android-prebuilt`.
2. Add `-Pchord.prebuilt=<directory>` to each Gradle command.

With `-Pchord.prebuilt`, Gradle does not run cargo. Run `exportPrebuilt` again after each change to `chord-core` or `chord-ffi`.

## Run on the emulator

1. Run `./dev/android-emulator.sh`. Add `--window` to see the screen.
2. Wait for the serial, for example `emulator-5554`.
3. Run `./gradlew installDebug` in `chord-android`.

The script reuses an emulator that already runs.

## Tests

Run all checks with `./dev/android-check.sh`. Add `--prebuilt` to skip the Rust build. The script runs these Gradle tasks:

- `testDebugUnitTest`: unit tests.
- `verifyRoborazziDebug`: screenshot tests.
- `lintDebug`: Android lint.
- `assembleDebug`: the debug APK.

Core behavior is tested in the core and in the CLI. The Android tests cover the app.

### Screenshots

Roborazzi makes the screenshots. They are in `app/src/test/screenshots/`. They are the visual reference for the iOS port.

- To check the screenshots, run `./gradlew verifyRoborazziDebug`.
- To update them after a planned UI change, run `./gradlew recordRoborazziDebug`.
- Commit the changed images with the code.

If a check fails, Roborazzi writes `_compare.png` and `_actual.png` files next to the references. CI uploads them as the artifact `roborazzi-diffs`.

### Smoke test

`./dev/android-smoke.sh` installs the debug APK on the emulator. It signs in, opens a room, and sends a message. Then a second account sends a message with `chord-cli`, and the script checks that the app shows it.

- The default server is `chat.foid.space`. It has a real certificate. Put the passwords in `dev/foid/.env`.
- Use `--server prosody` for the local Docker server. Android does not trust the mkcert CA, so use it only after you install the CA in the emulator.
- The top of the script lists the test tags that the UI must set.

## CI

The `android` job in `.github/workflows/ci.yml` runs the Gradle checks. It uploads the debug APK. After a failure, it uploads the screenshot differences.

## Install on a phone

Testers get sideloaded APKs. Send the debug APK from `app/build/outputs/apk/debug/`. The tester must allow installs from unknown sources.

## Troubleshooting

**The emulator crashes with a segmentation fault.** Do not load a snapshot. Start the emulator with `-no-snapshot -gpu swangle_indirect`. `dev/android-emulator.sh` does this. Use the same flags if you start the emulator by hand.

**The emulator does not start.** Read `/tmp/chord-emulator.log`.

**`adb devices` shows no device.** Run `./dev/android-emulator.sh` again.

**Gradle cannot find the NDK.** Make sure that `ANDROID_NDK_HOME` points to `~/android-sdk/ndk/29.0.14206865`. Enter the toolbox to get this value.

**`cargo ndk` is not found.** Run the toolbox script again.

**`--prebuilt` says that no prebuilt core exists.** Run `./gradlew exportPrebuilt`.

**The screenshot check fails after a UI change.** Look at the `_compare.png` files. If the change is planned, run `recordRoborazziDebug`.
