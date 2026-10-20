#!/bin/bash
# Castrel spike: watch the sink's own Miracast service while we scan for it.
#
# WHAT THE SURVEY FOUND
#
#   * The sink is a CVTE/Hisilicon Android TV (Hi3751V350, Android 9),
#     with an ATBM 2.4 GHz-only Wi-Fi chip, associated at 2412 MHz.
#   * Its Miracast is a *vendor* implementation - com.hisilicon.miracast
#     - not AOSP's WifiDisplaySinkController.
#   * That service **stops searching and stops listening the moment the
#     Wi-Fi association changes**:
#
#       WfdBusiness: connect L180 ---- no cfg saved ----
#       WfdBusiness: stopSearch L157
#       WfdBusiness: stopListen L212
#
#   * Our monitor-mode capture, and the ethernet to Wi-Fi switch, both
#     disturbed the TV's Wi-Fi state - so its Wi-Fi Direct listen was torn
#     down before we ever scanned. The silence we measured was the sink
#     switching off, not the sink being unreachable.
#
# WHAT THIS DOES
#
#   1. Tails the sink's logcat and kernel log, so we see exactly what its
#      Miracast service does while it waits: does it listen, on which
#      channel, and what does it advertise?
#   2. Scans for it from our side, without touching the interface.
#
# It changes nothing on either machine.
#
# Usage: ./spike/14-sink-live-scan.sh [serial] [seconds]

set -uo pipefail

SERIAL="${1:-192.168.1.2:5555}"
SECS="${2:-60}"
OUT="${OUT:-/tmp/opencode/spike}"
mkdir -p "$OUT"

echo "sink: $SERIAL"
echo
echo "== 1. tailing the sink's Miracast service =="
adb -s "$SERIAL" logcat -c >/dev/null 2>&1
( adb -s "$SERIAL" logcat -v time 2>/dev/null |
	grep -i -E 'miracast|wfd|p2p|atbm' > "$OUT/sink-logcat.log" ) &
LOG_PID=$!
( adb -s "$SERIAL" shell dmesg 2>/dev/null |
	grep -i -E 'p2p|wfd|atbm|miracast' > "$OUT/sink-dmesg.log" ) &
DMESG_PID=$!

echo "   the sink's service is running:"
adb -s "$SERIAL" shell dumpsys activity services com.hisilicon.miracast 2>/dev/null |
	head -6 | sed 's/^/   /'

echo
echo "== 2. scanning for the sink (${SECS}s, our interface is untouched) =="
cd "$(dirname "$0")/.." || exit 1
./target/release/aethercastd arm >/dev/null 2>&1
timeout $((SECS + 15)) ./target/release/aethercastd discover --duration "$SECS" --verbose 2>&1 | sed 's/^/   /'

kill $LOG_PID $DMESG_PID 2>/dev/null

echo
echo "== 3. what the sink's own service did =="
echo "  -- logcat (hisilicon.miracast, wfd, p2p) --"
grep -i -E 'start|listen|search|channel|freq|ie|advertis|discover|stop' "$OUT/sink-logcat.log" 2>/dev/null |
	tail -25 | sed 's/^/   /'
echo "  -- kernel (p2p, atbm) --"
tail -12 "$OUT/sink-dmesg.log" 2>/dev/null | sed 's/^/   /'
echo
echo "  full logs: $OUT/sink-logcat.log  $OUT/sink-dmesg.log"
