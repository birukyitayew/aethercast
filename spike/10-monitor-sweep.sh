#!/bin/bash
# Castrel spike: monitor-mode capture across the social channels.
#
# WHY A MULTI-CHANNEL SWEEP
#
# The first successful capture (30,792 frames) saw only channel 1, because
# a monitor-mode interface stays on the channel it is set to. A Miracast
# sink advertises on a Wi-Fi Direct *listen* channel, and the P2P social
# channels are 1, 6 and 11 - so a capture on channel 1 alone can easily
# miss the sink entirely.
#
# This version sweeps all three social channels. The channel is changed
# with `iw dev wlan0 set channel N` while the interface is in monitor
# mode; the capture tool cannot change channels itself.
#
# SAFETY
#
# Same rules as take three: it asks first, never stops the supplicant,
# gives the device back to NetworkManager in the restore, verifies the
# connection, and runs a watchdog.
#
# Usage: sudo ./spike/10-monitor-sweep.sh [seconds_per_channel]

set -uo pipefail

IFACE="${IFACE:-wlan0}"
SECS=15
CON="${CON:-Biruk}"
OUT="${OUT:-/tmp/opencode/spike}"
CHANNELS="${CHANNELS:-1 6 11}"

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

(
	sleep $((SECS * 3 + 60))
	echo
	echo "== watchdog: restoring the connection =="
	restore
) &
watchdog=$!
trap 'kill $watchdog 2>/dev/null; restore' EXIT INT TERM

echo "== what this will do =="
echo "  * take $IFACE out of NetworkManager's hands for about $((SECS * 3 + 20)) seconds"
echo "  * capture the air on channel(s): $CHANNELS"
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
sleep 2
ip link set "$IFACE" down
iw dev "$IFACE" set monitor otherbss 2>&1 | sed 's/^/  /'
ip link set "$IFACE" up
sleep 2
iw dev "$IFACE" info 2>/dev/null | grep -E 'type|channel' | sed 's/^/  /'

for channel in $CHANNELS; do
	PCAP="$OUT/monitor-ch$channel.pcap"
	echo
	echo "== 2. channel $channel =="
	iw dev "$IFACE" set channel "$channel" 2>&1 | sed 's/^/  /'
	sleep 1
	iw dev "$IFACE" info 2>/dev/null | grep -E 'channel' | sed 's/^/  /'
	rm -f "$PCAP"
	timeout $((SECS + 5)) tcpdump -i "$IFACE" -s 0 -w "$PCAP" 2>&1 | tail -3 | sed 's/^/  /'
	ls -l "$PCAP" 2>/dev/null | sed 's/^/  /'
done

echo
echo "== 3. what is on the air, per channel =="
for channel in $CHANNELS; do
	PCAP="$OUT/monitor-ch$channel.pcap"
	echo
	echo "  --- channel $channel ---"
	if [ ! -s "$PCAP" ]; then
		echo "    nothing captured"
		continue
	fi
	echo "    frames: $(tshark -r "$PCAP" 2>/dev/null | wc -l)"
	echo "    networks (SSID):"
	tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0008' -T fields -e wlan.ssid 2>/dev/null |
		grep -v '^$' | sort | uniq -c | sort -rn | head -6 | sed 's/^/      /'
	echo "    WFD information elements (OUI 50-6f-9a, type 0x0a):"
	wfd=$(tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x0a' \
		-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -6)
	if [ -n "$wfd" ]; then echo "$wfd" | sed 's/^/      /'; else echo "      none"; fi
	echo "    P2P information elements (type 0x09):"
	p2p=$(tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x09' \
		-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -6)
	if [ -n "$p2p" ]; then echo "$p2p" | sed 's/^/      /'; else echo "      none"; fi
	echo "    probe requests, by source:"
	tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0004' -T fields -e wlan.sa 2>/dev/null |
		sort | uniq -c | sort -rn | head -6 | sed 's/^/      /'
	echo "    AltoBeam (OUI dc:29:19) transmitters:"
	alto=$(tshark -r "$PCAP" -T fields -e wlan.sa 2>/dev/null | grep -i -E '^dc:29:19' | sort -u)
	if [ -n "$alto" ]; then echo "$alto" | sed 's/^/      /'; else echo "      none"; fi
done
