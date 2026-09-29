#!/usr/bin/env bash
# Build chord-ffi, generate the Kotlin bindings into a temp dir, and check that they exist.
# It does not compile the Kotlin.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

cargo build -p chord-ffi
case "$(uname -s)" in
  Darwin) lib=libchord_ffi.dylib ;;
  *) lib=libchord_ffi.so ;;
esac
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
cargo run -q -p chord-ffi --bin uniffi-bindgen -- generate \
  --library "target/debug/$lib" --language kotlin --no-format --out-dir "$out"
if [[ -z "$(find "$out" -name '*.kt' -print -quit)" ]]; then
  echo "FAIL: the bindgen made no .kt file." >&2
  exit 1
fi
echo "OK: Kotlin bindings generated: $(find "$out" -name '*.kt')"
