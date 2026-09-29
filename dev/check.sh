#!/usr/bin/env bash
# Run the checks that CI runs. Stops at the first failure.
# With --live, also run the tests against the dev Prosody server (see dev/prosody/README.md).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

cargo fmt --all --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p chord-cli --all-targets --features dev-insecure -- -D warnings
cargo test --workspace
cargo build -p chord-core --target wasm32-unknown-unknown --no-default-features
./dev/check-features.sh

if [[ "${1:-}" == "--live" ]]; then
  cargo test --workspace -- --ignored
  ./dev/send-demo.sh
fi
echo "All checks passed."
