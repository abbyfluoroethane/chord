#!/usr/bin/env bash
# A light local check: format, clippy, and the unit tests. No wasm build, no Kotlin, and
# no live server. GitHub CI runs the full set (dev/check.sh). The full local run filled the
# disk and crashed the Mac, so use this script on a laptop.
#
# CHORD_TARGET_DIR: a shared target directory for all worktrees (default: target/ of the
# main checkout). CHORD_JOBS: parallel rustc jobs (default: 4).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

min_free_gb=10
free_gb=$(df -g . | awk 'NR == 2 { print $4 }')
if (( free_gb < min_free_gb )); then
  echo "FAIL: only ${free_gb} GB free. Free ${min_free_gb} GB before a build." >&2
  exit 1
fi

main=$(git worktree list --porcelain | awk '/^worktree / { print $2; exit }')
export CARGO_TARGET_DIR="${CHORD_TARGET_DIR:-$main/target}"
export CARGO_BUILD_JOBS="${CHORD_JOBS:-4}"
export CARGO_INCREMENTAL=0

cargo fmt --all --check
cargo clippy -q -p chord-core -p chord-cli -p chord-ffi --all-targets -- -D warnings
cargo test -q -p chord-core --lib
cargo test -q -p chord-cli
echo "OK: quick check passed (target: $CARGO_TARGET_DIR)"
