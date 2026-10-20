#!/bin/bash
# Castrel spike: scan for the sink while its Miracast service is listening.
#
# WHY THIS VERSION IS DIFFERENT
#
# The previous run hung on `adb logcat -c`, which never returns on this
# vendor Android 9 build. And the ordering was wrong: the sink leaves
# the access point as soon as its Miracast service starts, so it is only
# reachable over ADB *before* the scan and unreachable *during* it.
#
# So this version:
#   * never calls `logcat -c`,
#   * puts a timeout on every adb invocation,
#   * starts the log tail in the background without waiting for it,
#   * and does the scan first, which is the thing that matters.
#
# The sink does not need to be on the LAN for the scan to work.
#
# Usage: ./spike/15-scan-while-listening.sh [serial] [seconds]

set -uo pipefail

SERIAL="${1:-192.168.1.2:5555}"
SECS="${2:-60}"
OUT="${OUT:-/tmp/opencode/spike}"
mkdir -p "$OUT"

adb_cmd() { timeout 15s adb -s "$SERIAL" "$@" 2>&1 | tr -d '\r'; }

echo "sink: $SERIAL"
echo "scanning for $SECS seconds — leave the TV on its screen-share screen"
echo

# Best effort: start tailing the sink's logs. Never block on it, and never
# clear its log buffer (`logcat -c` hangs on this build).
(
	timeout $((SECS + 20)) adb -s "$SERIAL" logcat -v time 2>/dev/null |
		grep -i -E 'miracast|wfd|p2p|atbm' > "$OUT/sink-logcat.log"
) >/dev/null 2>&1 &

# The scan: this is the part that matters, and it needs nothing from the
# sink.
cd "$(dirname "$0")/.." || exit 1
./target/release/aethercastd arm >/dev/null 2>&1
timeout $((SECS + 20)) ./target/release/aethercastd discover --duration "$SECS" --verbose 2>&1 | sed 's/^/  /'

echo
echo "== the sink's own view (best effort, it may be off the LAN) =="
state=$(adb_cmd shell dumpsys activity services com.hisilicon.miracast | head -4 | tr '\n' ' ')
if [ -n "$state" ]; then
	echo "  service: $state"
else
	echo "  the sink is off the LAN - which means its Miracast service is running"
	echo "  (that is what we want; it is listening as a Wi-Fi Direct device)"
fi

echo
echo "== summary of what we know =="
cat <<'EOF'
  The sink is a CVTE/Hisilicon Android TV with a 2.4 GHz-only ATBM chip.
  Its Miracast is the vendor app com.hisilicon.miracast, which stops
  searching and stops listening whenever the Wi-Fi association changes -
  and it leaves the access point while it waits for a source.

  So: our scan has to happen while it is listening, and our interface has
  to be left alone. This script does exactly that.
EOF
