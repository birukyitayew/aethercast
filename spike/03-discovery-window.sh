#!/bin/bash
# Castrel Phase 0 spike — supervised P2P discovery window.
#
# Prerequisite: the global WFD IE must be set, otherwise our probe requests
# carry no WFD IE and a strict Miracast sink may ignore them:
#     wpa_cli -i wlan0 WFD_SUBELEM_SET all 00000600901c4400c8
#
# The window:
#   * re-arms the WFD IE before every round (P2P state changes can drop it),
#   * runs a real p2p_find and waits for it to COMPLETE (a rejected find
#     leaves the radio-work queue wedged - see 02-recover-radio-work.sh),
#   * records peers, the radio-work queue depth, and the supplicant's
#     P2P_EVENT_DEVICE_FOUND lines, which carry the peer's WFD device info.
#
# Usage: ./spike/03-discovery-window.sh [rounds] [seconds_per_find]

set -uo pipefail

IFACE="${IFACE:-wlan0}"
ROUNDS="${1:-8}"
SECS="${2:-30}"
CTL=(wpa_cli -i "$IFACE")
OUT="${OUT:-/tmp/opencode/spike}"

mkdir -p "$OUT"
LOG="$OUT/discovery-window.log"
: > "$LOG"

# Follow the supplicant for P2P events (P2P_EVENT_DEVICE_FOUND carries
# wfd_dev_info=0x... for peers that advertise a WFD IE).
journalctl -f -u wpa_supplicant -o short-precise 2>/dev/null > "$OUT/supplicant-events.log" &
FOLLOW=$!
trap 'kill $FOLLOW 2>/dev/null' EXIT

say() { printf '%s %s\n' "$(date +%T)" "$*" | tee -a "$LOG"; }

say "=== Castrel discovery window: $ROUNDS rounds x ${SECS}s ==="
say "station: $("${CTL[@]}" status | grep -E 'ssid=|freq=' | tr '\n' ' ')"

for round in $(seq 1 "$ROUNDS"); do
	"${CTL[@]}" WFD_SUBELEM_SET all 00000600901c4400c8 >/dev/null 2>&1
	find_out=$("${CTL[@]}" p2p_find "$SECS" 2>&1)
	say "round $round: p2p_find -> $find_out"

	# Let the find run its full course (a social-channel scan needs time to
	# visit 1/6/11); do not restart it early.
	sleep "$((SECS + 2))"

	peers=$("${CTL[@]}" p2p_peers 2>/dev/null | tr '\n' ' ')
	work=$("${CTL[@]}" RADIO_WORK show 2>/dev/null | wc -l)
	say "round $round: peers=[${peers}] queued_work=$work"
	[ -n "${peers// /}" ] && say "round $round: PEER FOUND: $peers"

	"${CTL[@]}" p2p_stop_find >/dev/null 2>&1
	sleep 3
done

say "=== window closed ==="
say "device-found events:"
grep -c 'P2P_EVENT_DEVICE_FOUND' "$OUT/supplicant-events.log" 2>/dev/null
grep 'P2P_EVENT_DEVICE_FOUND' "$OUT/supplicant-events.log" 2>/dev/null | tail -5
