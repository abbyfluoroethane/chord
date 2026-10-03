#!/usr/bin/env bash
# Start the chord-api36 emulator, wait for the boot, and print the adb serial. If an
# emulator already runs, use it. The emulator runs without a window, unless you pass
# --window.
#
# The flags -no-snapshot and -gpu swangle_indirect are required. A snapshot load made the
# emulator crash with a segmentation fault.
# Run this script on the host or in the chord-android toolbox. On the host it runs
# itself in the toolbox.
set -euo pipefail

avd=${CHORD_AVD:-chord-api36}
window=0
[[ "${1:-}" == "--window" ]] && window=1

if [[ ! -f /run/.containerenv ]] || ! grep -q 'name="chord-android"' /run/.containerenv; then
  exec toolbox run -c chord-android bash "$(readlink -f "$0")" "$@"
fi
# shellcheck disable=SC1091
. /etc/chord-android.sh

running_serial() { adb devices | awk '/^emulator-[0-9]+\tdevice$/ { print $1; exit }'; }

serial=$(running_serial || true)
if [[ -z "$serial" ]]; then
  flags=(-no-snapshot -gpu swangle_indirect -no-audio)
  (( window )) || flags+=(-no-window)
  log=${TMPDIR:-/tmp}/chord-emulator.log
  echo "Starting $avd (log: $log)" >&2
  nohup emulator -avd "$avd" "${flags[@]}" >"$log" 2>&1 &
  for _ in $(seq 1 60); do
    serial=$(running_serial || true)
    [[ -n "$serial" ]] && break
    sleep 2
  done
  [[ -n "$serial" ]] || { echo "FAIL: no emulator after 120 s. See $log." >&2; exit 1; }
fi

echo "Waiting for the boot of $serial" >&2
for _ in $(seq 1 150); do
  if [[ "$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; then
    echo "$serial"
    exit 0
  fi
  sleep 2
done
echo "FAIL: $serial did not finish the boot in 300 s." >&2
exit 1
