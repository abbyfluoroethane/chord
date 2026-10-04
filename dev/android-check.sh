#!/usr/bin/env bash
# The Android check: unit tests, the Roborazzi screenshot check, lint, and a debug APK.
# Use --prebuilt to skip the Rust build. The script then takes the core from
# CHORD_PREBUILT (default: target/android-prebuilt in the main checkout). Make that
# directory with: ./gradlew exportPrebuilt
#
# Run it on the host or in the chord-android toolbox. On the host it runs itself in the
# toolbox. CHORD_GRADLE_ARGS adds extra Gradle arguments.
set -euo pipefail
cd "$(dirname "$0")/.."

prebuilt=0
for arg in "$@"; do
  case "$arg" in
    --prebuilt) prebuilt=1 ;;
    *) echo "usage: $0 [--prebuilt]" >&2; exit 2 ;;
  esac
done

if [[ ! -f /run/.containerenv ]] || ! grep -q 'name="chord-android"' /run/.containerenv; then
  exec toolbox run -c chord-android bash "$PWD/dev/android-check.sh" "$@"
fi
# shellcheck disable=SC1091
. /etc/chord-android.sh
export PATH="$HOME/.cargo/bin:$PATH"

args=(--console=plain)
if (( prebuilt )); then
  main=$(git worktree list --porcelain | awk '/^worktree / && !done { print $2; done = 1 }')
  dir=${CHORD_PREBUILT:-$main/target/android-prebuilt}
  [[ -d "$dir/jniLibs" && -d "$dir/uniffi" ]] ||
    { echo "FAIL: no prebuilt core in $dir. Run: cd chord-android && ./gradlew exportPrebuilt" >&2; exit 1; }
  args+=("-Pchord.prebuilt=$dir")
fi

cd chord-android
# shellcheck disable=SC2086
./gradlew "${args[@]}" ${CHORD_GRADLE_ARGS:-} \
  testDebugUnitTest verifyRoborazziDebug lintDebug assembleDebug
echo "OK: Android check passed"
