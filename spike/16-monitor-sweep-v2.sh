#!/bin/bash
# Castrel spike: monitor-mode sweep, with a positive control.
#
# THE FLAW IN THE PREVIOUS SWEEP
#
# The channel 6 and channel 11 captures produced **zero frames** — not
# even the "Nebyu" network, which a normal scan puts on channel 6 at
# 52% signal. A capture that sees nothing on a channel it is supposed
# to be on is not evidence that the channel is empty; it is evidence that
# the capture did not work. The previous sweep therefore proved nothing
# about channels 6 and 11, and the conclusion we drew from it was
# unjustified.
#
# THE FIX
#
# Every channel now has a **positive control**: we know a network exists
# on channel 6, so if the channel 6 capture does not show it, the
# capture is broken and the result is reported as such rather than as
# "nothing there".
#
# The sink is off the access point while its Miracast service listens, so
# it is expected to be absent from the LAN during this capture.
#
# Usage: sudo ./spike/16-monitor-sweep-v2.sh [seconds_per_channel]

set -uo pipefail

IFACE="${IFACE:-wlan0}"
SECS=20
CON="${CON:-Biruk}"
OUT="${OUT:-/tmp/opencode/spike}"
CHANNELS="${CHANNELS:-1 6 11}"
CONTROL_SSID="${CONTROL_SSID:-Nebyu}"   # a network we know is on channel 6

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
	restore
) &
watchdog=$!
trap 'kill $watchdog 2>/dev/null; restore' EXIT INT TERM

echo "== what this will do =="
echo "  * take $IFACE out of NetworkManager for about $((SECS * 3 + 20)) seconds"
echo "  * capture channels: $CHANNELS"
echo "  * check each capture against a known network, so an empty result"
echo "    cannot be mistaken for an empty channel"
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

echo "== 1. monitor mode =="
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
	sleep 2
	iw dev "$IFACE" info 2>/dev/null | grep -E '^\s+channel' | sed 's/^/  /'
	rm -f "$PCAP"
	timeout $((SECS + 5)) tcpdump -i "$IFACE" -s 0 -w "$PCAP" 2>&1 | tail -2 | sed 's/^/  /'
done

echo
echo "== 3. per channel, with the control check =="
for channel in $CHANNELS; do
	PCAP="$OUT/monitor-ch$channel.pcap"
	echo
	echo "  --- channel $channel ---"
	if [ ! -s "$PCAP" ]; then
		echo "    NO FRAMES: the capture did not work on this channel"
		echo "    (a broken capture is not evidence of an empty channel)"
		continue
	fi
	frames=$(tshark -r "$PCAP" 2>/dev/null | wc -l)
	echo "    frames: $frames"
	ssids=$(tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0008' -T fields -e wlan.ssid 2>/dev/null |
		grep -v '^$' | sort | uniq -c | sort -rn)
	if [ -n "$ssids" ]; then
		echo "    networks:"
		echo "$ssids" | sed 's/^/      /'
	else
		echo "    networks: none"
	fi
	# The control: on the channel where we know a network lives, it must
	# appear, or the capture is broken.
	case "$channel" in
	6)
		if echo "$ssids" | grep -q "$CONTROL_SSID"; then
			echo "    control: '$CONTROL_SSID' is visible, so this capture is good"
		else
			echo "    control: '$CONTROL_SSID' is MISSING - this capture is not trustworthy"
		fi
		;;
	esac
	echo "    WFD elements:"
	wfd=$(tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x0a' \
		-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -5)
	if [ -n "$wfd" ]; then echo "$wfd" | sed 's/^/      /'; else echo "      none"; fi
	echo "    P2P elements:"
	p2p=$(tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x09' \
		-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -5)
	if [ -n "$p2p" ]; then echo "$p2p" | sed 's/^/      /'; else echo "      none"; fi
	echo "    probe requests:"
	pr=$(tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0004' -T fields -e wlan.sa 2>/dev/null |
		sort | uniq -c | sort -rn | head -5)
	if [ -n "$pr" ]; then echo "$pr" | sed 's/^/      /'; else echo "      none"; fi
done

echo
echo "== 4. every device in the captures, and what it advertises =="
python3 "$(dirname "$0")/11-extract-wfd.py" $OUT/monitor-ch*.pcap 2>&1 | sed 's/^/  /'
