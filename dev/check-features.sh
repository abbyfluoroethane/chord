#!/usr/bin/env bash
# Make sure that chord-ffi and the default chord-cli build do not enable dev-insecure.
#
# It reads the resolved features of each package ({f} in cargo tree --format). The plain
# `cargo tree -e features` output does not show a feature that another feature turns on,
# so a grep on it finds nothing even in a bad build.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

# Print the chord-core and tokio-xmpp lines with their resolved features. Call it in an
# assignment, not in a condition, so that a cargo failure stops the script (set -e).
features() {
  cargo tree "$@" -e normal --prefix none --format '{p} [{f}]' |
    grep -E '^(chord-core|tokio-xmpp) ' | sort -u
}

insecure() {
  grep -qE '[[,](dev-insecure|insecure-tcp)[],]'
}

status=0
for pkg in chord-ffi chord-cli; do
  resolved=$(features -p "$pkg")
  if insecure <<<"$resolved"; then
    echo "FAIL: $pkg enables dev-insecure or insecure-tcp in its default build." >&2
    echo "$resolved" >&2
    status=1
  else
    echo "OK: $pkg does not enable dev-insecure or insecure-tcp."
  fi
done

# Self-test: the check must catch a build that enables the feature.
resolved=$(features -p chord-cli --features dev-insecure)
if ! insecure <<<"$resolved"; then
  echo "FAIL: the check does not detect --features dev-insecure. Fix this script." >&2
  status=1
fi
exit "$status"
