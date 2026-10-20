#!/bin/bash
# Castrel spike: monitor-mode capture, take two.
#
# The first attempt captured nothing at all — not even the access point's
# beacons, which are always visible on the channel we are on. That means
# the capture never started rather than the air being empty. Two things
# were wrong with it:
#
#   * the error output of the capture tool was thrown away, so the reason
#     was invisible,
#   * `wpa_supplicant` was still running and holding the radio, so the
#     interface never really went off the access point.
#
# This version stops the supplicant, shows every message, and falls back
# to tcpdump if dumpcap produces nothing.
#
# COST
#
# The interface is down while capturing, so the internet and this session
# stop for about 45 seconds. The connection is restored at the end, even
# if the capture fails.
#
# Usage: sudo ./spike/08-monitor-capture-v2.sh [seconds]

set -uo pipefail

IFACE="${IFACE:-wlan0}"
SECS="${1:-30}"
CON="${CON:-Biruk}"
OUT="${OUT:-/tmp/opencode/spike}"
PCAP="$OUT/monitor.pcap"
LOG="$OUT/monitor.log"

mkdir -p "$OUT"

restore() {
	echo
	echo "== restoring =="
	ip link set "$IFACE" down 2>/dev/null
	iw dev "$IFACE" set type managed 2>/dev/null
	ip link set "$IFACE" up 2>/dev/null
	systemctl start wpa_supplicant 2>&1 | sed 's/^/  /'
	sleep 3
	nmcli connection up "$CON" 2>&1 | sed 's/^/  /'
	sleep 3
	wpa_cli -i "$IFACE" status 2>/dev/null | grep -E 'wpa_state|ssid=' | sed 's/^/  /'
}
trap restore EXIT

echo "== 1. freeing the radio =="
nmcli device set "$IFACE" managed no 2>&1 | sed 's/^/  /'
nmcli device disconnect "$IFACE" 2>&1 | sed 's/^/  /'
systemctl stop wpa_supplicant 2>&1 | sed 's/^/  /'
sleep 2
ip link set "$IFACE" down
iw dev "$IFACE" set monitor otherbss 2>&1 | sed 's/^/  /'
ip link set "$IFACE" up
sleep 2
iw dev "$IFACE" info 2>/dev/null | sed 's/^/  /'
iw dev "$IFACE" link 2>&1 | sed 's/^/  /'

echo
echo "== 2. capturing $SECS seconds (dumpcap output is shown) =="
rm -f "$PCAP" "$LOG"
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
	echo "  STILL nothing captured. The capture tool cannot attach to this interface."
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
