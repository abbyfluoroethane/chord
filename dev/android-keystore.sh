#!/usr/bin/env bash
# Make a tester keystore for the Android release build, outside the repository, and write
# chord-android/keystore.properties (ignored by the VCS) that points to it.
#
# Usage: dev/android-keystore.sh [--path <file.jks>] [--force]
# Default path: ~/.config/chord/android-release.jks
#
# This key signs test builds only. Keep the same key for all builds of a tester, because
# Android refuses an update that is signed with another key.
set -euo pipefail
cd "$(dirname "$0")/.."

path="$HOME/.config/chord/android-release.jks"
force=0
while (( $# )); do
  case "$1" in
    --path) path=${2:?--path needs a value}; shift 2 ;;
    --force) force=1; shift ;;
    *) echo "usage: $0 [--path <file.jks>] [--force]" >&2; exit 2 ;;
  esac
done

props=chord-android/keystore.properties
if [[ -e "$path" && $force -eq 0 ]]; then
  echo "The keystore exists: $path" >&2
  [[ -f $props ]] || echo "But $props is missing." >&2
  echo "Use --force to replace it. That changes the signature of new builds." >&2
  exit 1
fi
if ! command -v keytool >/dev/null; then
  echo "keytool not found. Install a JDK, or run this in the chord-android toolbox." >&2
  exit 1
fi

rand() { head -c 48 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' | head -c 24; }
store_pass=$(rand)
key_pass=$store_pass # PKCS12 uses one password for the store and the key.
key_alias=chord

mkdir -p "$(dirname "$path")"
rm -f "$path"
keytool -genkeypair -keystore "$path" -storetype PKCS12 -alias "$key_alias" \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=Chord tester, O=Chord" \
  -storepass "$store_pass" -keypass "$key_pass" >/dev/null
chmod 600 "$path"

umask 077
cat >"$props" <<PROPS
storeFile=$path
storePassword=$store_pass
keyAlias=$key_alias
keyPassword=$key_pass
PROPS
echo "OK: keystore $path"
echo "OK: wrote $props"
