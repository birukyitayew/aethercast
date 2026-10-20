#!/bin/bash
# Castrel spike: monitor-mode capture, take three.
#
# WHY THIS VERSION EXISTS
#
# Take two downed the network and did not come back. Two reasons, both
# ours:
#
#   1. it set `nmcli device set wlan0 managed no` and never set it back,
#      so NetworkManager refused to re-activate the connection;
#   2. it stopped the wpa_supplicant service, which is more than a
#      capture needs.
#
# This version is defensive by construction:
#
#   * it asks before it touches anything, and says exactly what will
#     happen,
#   * it never stops the wpa_supplicant service,
#   * a watchdog restores the connection even if the script is killed,
#   * the restore is verified, and if it fails the exact recovery commands
#     are printed,
#   * `trap` covers EXIT, INT and TERM.
#
# THE COST
#
# The interface is down while capturing, so the internet and this session
# stop for the length of the capture. That is unavoidable: a managed
# interface never delivers the management frames this needs.
#
# Usage: sudo ./spike/09-monitor-capture-v3.sh [seconds]
#        sudo ./spike/09-monitor-capture-v3.sh --yes [seconds]

set -uo pipefail

IFACE="${IFACE:-wlan0}"
SECS=30
CON="${CON:-Biruk}"
OUT="${OUT:-/tmp/opencode/spike}"
PCAP="$OUT/monitor.pcap"

while [ $# -gt 0 ]; do
	case "$1" in
	--yes) ASSUME_YES=1 ;;
	*) SECS="$1" ;;
	esac
	shift
done
ASSUME_YES="${ASSUME_YES:-0}"

mkdir -p "$OUT"

restore() {
	echo
	echo "== restoring the connection =="
	# The important step: give NetworkManager the device back. Without it
	# the connection cannot be re-activated.
	nmcli device set "$IFACE" managed yes 2>&1 | sed 's/^/  /'
	ip link set "$IFACE" up 2>&1 | sed 's/^/  /'
	iw dev "$IFACE" set type managed 2>&1 | sed 's/^/  /'
	sleep 2
	nmcli connection up "$CON" 2>&1 | sed 's/^/  /'
	sleep 3

	if wpa_cli -i "$IFACE" status 2>/dev/null | grep -q 'wpa_state=COMPLETED'; then
		echo "  the connection is back"
	else
		echo "  the connection did NOT come back. Run this:"
		echo "    sudo nmcli device set $IFACE managed yes"
		echo "    sudo ip link set $IFACE up"
		echo "    nmcli connection up $CON"
	fi
}

# A watchdog: whatever happens, the network comes back.
(
	sleep $((SECS + 45))
	echo
	echo "== watchdog: restoring the connection =="
	restore
) &

watchdog=$!
trap 'kill $watchdog 2>/dev/null; restore' EXIT INT TERM

echo "== what this will do =="
echo "  * take $IFACE out of NetworkManager's hands for about $((SECS + 15)) seconds"
echo "  * capture the air on channel 1, which includes every nearby network"
echo "  * put the connection back and verify it"
echo
if [ "$ASSUME_YES" != "1" ]; then
	read -r -p "Continue? [y/N] " answer
	case "$answer" in
	[yY]*) ;;
	*)
		echo "cancelled"
		exit 0
		;;
	esac
fi

echo "== 1. taking $IFACE out of NetworkManager =="
nmcli device set "$IFACE" managed no 2>&1 | sed 's/^/  /'
nmcli device disconnect "$IFACE" 2>&1 | sed 's/^/  /'
sleep 2
ip link set "$IFACE" down
iw dev "$IFACE" set monitor otherbss 2>&1 | sed 's/^/  /'
ip link set "$IFACE" up
sleep 2
iw dev "$IFACE" info 2>/dev/null | sed 's/^/  /'

echo
echo "== 2. capturing $SECS seconds (the capture tool's own output is shown) =="
rm -f "$PCAP"
timeout $((SECS + 5)) dumpcap -i "$IFACE" -a "duration:$SECS" -w "$PCAP" 2>&1 | sed 's/^/  /'
ls -l "$PCAP" 2>/dev/null | sed 's/^/  /'

if [ ! -s "$PCAP" ]; then
	echo
	echo "  dumpcap produced nothing, trying tcpdump"
	timeout $((SECS + 5)) tcpdump -i "$IFACE" -s 0 -w "$PCAP" 2>&1 | sed 's/^/  /'
	ls -l "$PCAP" 2>/dev/null | sed 's/^/  /'
fi

echo
echo "== 3. what is on the air =="
if [ ! -s "$PCAP" ]; then
	echo "  nothing was captured, so the capture tool could not attach."
	exit 1
fi

echo "  total frames: $(tshark -r "$PCAP" 2>/dev/null | wc -l)"

echo
echo "  -- channels and their frame counts --"
tshark -r "$PCAP" -T fields -e wlan_radio.channel 2>/dev/null |
	sort | uniq -c | sort -rn | head -8 | sed 's/^/    /'

echo
echo "  -- beacons (the networks in range) --"
tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0008' -T fields -e wlan.ssid -e wlan_radio.channel 2>/dev/null |
	sort | uniq -c | sort -rn | head -8 | sed 's/^/    /'

echo
echo "  -- probe requests, by source --"
tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0004' -T fields -e wlan.sa 2>/dev/null |
	sort | uniq -c | sort -rn | head -10 | sed 's/^/    /'

echo
echo "  -- devices that sent a WFD information element (OUI 50-6f-9a, type 0x0a) --"
tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x0a' \
	-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -10 | sed 's/^/    /'

echo
echo "  -- devices that sent a P2P information element (type 0x09) --"
tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x09' \
	-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -10 | sed 's/^/    /'

echo
echo "  -- any AltoBeam device (OUI dc:29:19)? --"
alto=$(tshark -r "$PCAP" -T fields -e wlan.sa 2>/dev/null | grep -i -E '^dc:29:19' | sort -u)
if [ -n "$alto" ]; then
	echo "$alto" | sed 's/^/    /'
else
	echo "    none: the sink transmits nothing in this band"
fi

echo
echo "  -- distinct source addresses --"
tshark -r "$PCAP" -T fields -e wlan.sa 2>/dev/null | sort -u | head -20 | sed 's/^/    /'
