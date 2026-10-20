#!/bin/bash
# AetherCast Phase 0 spike — recover a wedged wpa_supplicant radio-work queue.
#
# Symptom (reproduced on QCA9565/AR9565 + ath9k, Kali 2026.2):
#   * The first P2P discovery scan never returns results.
#   * wpa_supplicant keeps that "p2p-scan" radio work in the queue forever
#     (started=1), so every later request is rejected instantly:
#       P2P: Reject scan trigger since one is already pending
#   * Even a plain scan fails: FAIL-BUSY
#   * NetworkManager keeps queueing its periodic scan every 5 minutes, so the
#     queue grows without bound.
#
# Recovery: FLUSH aborts the stuck scan and resets P2P state, after which the
# station is reactivated and the global WFD IE is re-armed.
#
# Usage: sudo -n true 2>/dev/null; ./spike/02-recover-radio-work.sh
# No root is required: the wpa_supplicant control socket is group-writable
# for members of the "netdev" group.

set -uo pipefail

IFACE="${IFACE:-wlan0}"
CON="${CON:-Biruk}"
CTL=(wpa_cli -i "$IFACE")

hr() { printf '\n=== %s ===\n' "$1"; }

hr "BEFORE — radio work queue"
"${CTL[@]}" RADIO_WORK show | head -5
printf 'queued work items: %s\n' "$("${CTL[@]}" RADIO_WORK show | wc -l)"
printf 'plain scan result: %s\n' "$("${CTL[@]}" scan 2>&1)"

hr "FLUSH — abort the stuck scan and reset P2P state"
"${CTL[@]}" FLUSH 2>&1

sleep 3
if ! "${CTL[@]}" status | grep -q 'wpa_state=COMPLETED'; then
	echo "station dropped by FLUSH; reactivating connection '$CON'"
	nmcli connection up "$CON" >/dev/null 2>&1 || nmcli device connect "$IFACE" >/dev/null 2>&1
	for _ in $(seq 1 25); do
		"${CTL[@]}" status | grep -q 'wpa_state=COMPLETED' && break
		sleep 1
	done
fi

hr "AFTER — radio work queue"
printf 'queued work items: %s\n' "$("${CTL[@]}" RADIO_WORK show | wc -l)"
"${CTL[@]}" status | grep -E 'wpa_state|ssid|freq'
printf 'plain scan result: %s\n' "$("${CTL[@]}" scan 2>&1)"
sleep 6
"${CTL[@]}" scan_results | head -6

hr "RE-ARM — global WFD IE (Device Info | Capability Discovery | App Info)"
"${CTL[@]}" WFD_SUBELEM_SET all 00000600901c4400c8 2>&1
printf 'WFD IE now: %s\n' "$("${CTL[@]}" WFD_SUBELEM_GET all 2>&1)"

hr "P2P DISCOVERY — does the scan complete this time?"
"${CTL[@]}" p2p_find 25 2>&1
sleep 14
printf 'peers: [%s]\n' "$("${CTL[@]}" p2p_peers 2>/dev/null | tr '\n' ' ')"
"${CTL[@]}" p2p_stop_find >/dev/null 2>&1

hr "RADIO WORK AFTER DISCOVERY"
"${CTL[@]}" RADIO_WORK show | head -6
printf 'queued work items: %s\n' "$("${CTL[@]}" RADIO_WORK show | wc -l)"
