#!/usr/bin/env bash
# Build chord-ffi, generate the Kotlin bindings into a temp dir, and check that they exist.
# Then compile the bindings with a small smoke program (chord-ffi/kotlin-test/Smoke.kt) and run it.
#
# The Kotlin step needs java and kotlinc. It is skipped with a message when one is missing.
# Set CHORD_KOTLIN_FETCH=1 to download a pinned kotlinc when it is not on PATH (CI does this).
# Set CHORD_KOTLIN_REQUIRE=1 to fail, in place of skip, when the Kotlin step cannot run.
# The jars are cached in target/kotlin-deps, or in $CHORD_KOTLIN_CACHE.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

# Pinned downloads. Update the version and the sha256 together.
JNA_VERSION=5.14.0
JNA_SHA256=34ed1e1f27fa896bca50dbc4e99cf3732967cec387a7a0d5e3486c09673fe8c6
COROUTINES_VERSION=1.8.1
COROUTINES_SHA256=f3d4f5de1c391bbcc20f3b3435ccbac013521e76b6902d7d59635ec15c1f797e
KOTLIN_VERSION=2.0.21
KOTLIN_SHA256=0352c0a45bd22f80f6b26e485cd04da8047baa5de54865281fb9f89a4a7bcf2a

cargo build -p chord-ffi
case "$(uname -s)" in
  Darwin) lib=libchord_ffi.dylib ;;
  *) lib=libchord_ffi.so ;;
esac
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
cargo run -q -p chord-ffi --bin uniffi-bindgen -- generate \
  --library "target/debug/$lib" --language kotlin --no-format --out-dir "$out"
kt=$(find "$out" -name 'chord_ffi.kt' -print -quit)
if [[ -z "$kt" ]]; then
  echo "FAIL: the bindgen made no chord_ffi.kt file." >&2
  exit 1
fi
echo "OK: Kotlin bindings generated: $kt"

skip_kotlin() {
  if [[ "${CHORD_KOTLIN_REQUIRE:-0}" == 1 ]]; then
    echo "FAIL: $1" >&2
    exit 1
  fi
  echo "SKIP: Kotlin compile and smoke run. $1"
  exit 0
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# fetch URL DEST SHA256: download once, then check the hash on every run.
fetch() {
  local url=$1 dest=$2 want=$3
  if [[ ! -f "$dest" ]]; then
    curl -fsSL --retry 3 -o "$dest.part" "$url"
    mv "$dest.part" "$dest"
  fi
  local got
  got=$(sha256_of "$dest")
  if [[ "$got" != "$want" ]]; then
    rm -f "$dest"
    echo "FAIL: bad sha256 for $dest: got $got, want $want" >&2
    exit 1
  fi
}

if ! command -v java >/dev/null 2>&1 || ! java -version >/dev/null 2>&1; then
  skip_kotlin "java is not installed."
fi

cache=${CHORD_KOTLIN_CACHE:-$PWD/target/kotlin-deps}
mkdir -p "$cache"

kotlinc_bin=$(command -v kotlinc || true)
if [[ -z "$kotlinc_bin" ]]; then
  if [[ "${CHORD_KOTLIN_FETCH:-0}" != 1 ]]; then
    skip_kotlin "kotlinc is not on PATH. Install it, or set CHORD_KOTLIN_FETCH=1 to download version $KOTLIN_VERSION."
  fi
  zip="$cache/kotlin-compiler-$KOTLIN_VERSION.zip"
  fetch "https://github.com/JetBrains/kotlin/releases/download/v$KOTLIN_VERSION/kotlin-compiler-$KOTLIN_VERSION.zip" \
    "$zip" "$KOTLIN_SHA256"
  if [[ ! -x "$cache/kotlinc/bin/kotlinc" ]]; then
    unzip -q -o "$zip" -d "$cache"
  fi
  kotlinc_bin="$cache/kotlinc/bin/kotlinc"
fi

maven=https://repo1.maven.org/maven2
jna="$cache/jna-$JNA_VERSION.jar"
coroutines="$cache/kotlinx-coroutines-core-jvm-$COROUTINES_VERSION.jar"
fetch "$maven/net/java/dev/jna/jna/$JNA_VERSION/jna-$JNA_VERSION.jar" "$jna" "$JNA_SHA256"
fetch "$maven/org/jetbrains/kotlinx/kotlinx-coroutines-core-jvm/$COROUTINES_VERSION/kotlinx-coroutines-core-jvm-$COROUTINES_VERSION.jar" \
  "$coroutines" "$COROUTINES_SHA256"

# -include-runtime puts the Kotlin stdlib in the jar.
"$kotlinc_bin" -nowarn -cp "$jna:$coroutines" -include-runtime \
  -d "$out/smoke.jar" "$kt" chord-ffi/kotlin-test/Smoke.kt
echo "OK: Kotlin bindings compiled."

result=$(java -Djna.library.path="$PWD/target/debug" \
  -cp "$out/smoke.jar:$jna:$coroutines" SmokeKt "$out/smoke.db")
echo "$result"
if [[ "$result" != *OK* ]]; then
  echo "FAIL: the Kotlin smoke run did not print OK." >&2
  exit 1
fi
echo "OK: Kotlin smoke run passed."
