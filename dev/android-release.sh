#!/usr/bin/env bash
# Build the signed release APKs (universal and one per ABI), print their paths and SHA-256.
#
# Usage: dev/android-release.sh [--install] [--prebuilt <dir>] [--abis <list>]
#   --install          Install the right APK on the connected device with adb.
#   --prebuilt <dir>   Use a prebuilt core (see chord-android/README.md) and skip cargo.
#   --abis <list>      ABIs to build, for example x86_64. Default: arm64-v8a,x86_64.
#
# Needs chord-android/keystore.properties (run dev/android-keystore.sh) or the CHORD_KEYSTORE_*
# variables. The script runs itself in the chord-android toolbox when it starts on the host.
set -euo pipefail
cd "$(dirname "$0")/.."

install=0
gradle_args=()
while (( $# )); do
  case "$1" in
    --install) install=1; shift ;;
    --prebuilt) gradle_args+=("-Pchord.prebuilt=$(cd "${2:?--prebuilt needs a directory}" && pwd)"); shift 2 ;;
    --abis) gradle_args+=("-Pchord.abis=${2:?--abis needs a value}"); shift 2 ;;
    *) echo "usage: $0 [--install] [--prebuilt <dir>] [--abis <list>]" >&2; exit 2 ;;
  esac
done

if [[ ! -f /run/.containerenv ]] || ! grep -q 'name="chord-android"' /run/.containerenv; then
  exec toolbox run -c chord-android bash "$PWD/dev/android-release.sh" "$@"
fi
# shellcheck disable=SC1091
. /etc/chord-android.sh
export PATH="$HOME/.cargo/bin:$PATH"

if [[ ! -f chord-android/keystore.properties && -z "${CHORD_KEYSTORE_FILE:-}" ]]; then
  echo "FAIL: no signing key. Run dev/android-keystore.sh first." >&2
  exit 1
fi

(cd chord-android && ./gradlew --console=plain assembleRelease "${gradle_args[@]}")

out=chord-android/app/build/outputs/apk/release
mapfile -t apks < <(find "$out" -name '*.apk' | sort)
((${#apks[@]})) || { echo "FAIL: no APK in $out" >&2; exit 1; }
if compgen -G "$out/*unsigned*" >/dev/null; then
  echo "FAIL: the APKs are unsigned. Check chord-android/keystore.properties." >&2
  exit 1
fi

echo "sha256  size  path"
for apk in "${apks[@]}"; do
  printf '%s  %s  %s\n' "$(sha256sum "$apk" | cut -d' ' -f1)" "$(du -h "$apk" | cut -f1)" "$apk"
done

if (( install )); then
  abi=$(adb shell getprop ro.product.cpu.abi | tr -d '\r')
  [[ -n "$abi" ]] || { echo "FAIL: no device. Connect one with adb." >&2; exit 1; }
  apk=$(printf '%s\n' "${apks[@]}" | grep -- "-$abi-" | head -1 || true)
  [[ -n "$apk" ]] || apk=$(printf '%s\n' "${apks[@]}" | grep universal | head -1)
  echo "Install $apk ($abi)"
  adb install -r "$apk"
fi
