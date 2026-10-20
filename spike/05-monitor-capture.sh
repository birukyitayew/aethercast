#!/bin/bash
# Castrel spike: monitor-mode capture of the 2.4 GHz air.
#
# WHY THIS EXISTS
#
# Discovery is provably healthy: the radio-work queue is empty, every
# `p2p_find` runs to completion, our probe requests carry a WFD
# information element, and the sink answers neither our probes nor ours
# when we listen. The sink is also gone from the LAN.
#
# The remaining explanations are:
#
#   1. The sink is in Miracast mode but on 5 GHz. The reference adapter
#      (QCA9565/AR9565) is 2.4 GHz only, so such a sink is invisible to
#      us no matter what the software does.
#   2. The sink is on 2.4 GHz but on a channel or in a mode we do not
#      reach.
#   3. The screen the user opened is not a Wi-Fi Display sink at all.
#
# A monitor-mode capture answers all three: it shows every frame in the
# band, including the ones a managed interface never delivers.
#
# WHAT IT DOES
#
#   * takes wlan0 out of NetworkManager's hands and puts it in monitor
#     mode,
#   * records 30 seconds of the air,
#   * restores managed mode and the connection, whatever happens,
#   * reports the channels in use, every P2P and WFD information element
#     it saw, and who sent them.
#
# COST
#
# The interface is down while capturing, so the internet and this session
# stop for about 40 seconds.
#
# Usage: sudo ./spike/05-monitor-capture.sh [seconds]

set -uo pipefail

IFACE="${IFACE:-wlan0}"
SECS="${1:-30}"
CON="${CON:-Biruk}"
OUT="${OUT:-/tmp/opencode/spike}"
PCAP="$OUT/monitor.pcap"

mkdir -p "$OUT"

restore() {
	echo
	echo "== restoring managed mode =="
	ip link set "$IFACE" down 2>/dev/null
	iw dev "$IFACE" set type managed 2>/dev/null
	ip link set "$IFACE" up 2>/dev/null
	nmcli device set "$IFACE" managed yes 2>/dev/null
	sleep 2
	nmcli connection up "$CON" 2>&1 | sed 's/^/  /'
	sleep 3
	wpa_cli -i "$IFACE" status 2>/dev/null | grep -E 'wpa_state|ssid=' | sed 's/^/  /'
}
trap restore EXIT

echo "== 1. taking $IFACE out of NetworkManager =="
nmcli device set "$IFACE" managed no 2>&1 | sed 's/^/  /'
sleep 2
ip link set "$IFACE" down
iw dev "$IFACE" set monitor otherbss 2>&1 | sed 's/^/  /'
ip link set "$IFACE" up
sleep 1
iw dev "$IFACE" info 2>/dev/null | grep -E 'type|channel' | sed 's/^/  /'

echo
echo "== 2. capturing $SECS seconds of the air =="
rm -f "$PCAP"
dumpcap -i "$IFACE" -a "duration:$SECS" -w "$PCAP" >/dev/null 2>&1 &
CAP=$!
sleep $((SECS + 4))
kill "$CAP" 2>/dev/null
wait "$CAP" 2>/dev/null
ls -l "$PCAP" 2>/dev/null | sed 's/^/  /'

echo
echo "== 3. what is on the air =="
[ -s "$PCAP" ] || { echo "  no packets captured"; exit 1; }

echo "  total frames: $(tshark -r "$PCAP" 2>/dev/null | wc -l)"

echo
echo "  -- channels and their frame counts --"
tshark -r "$PCAP" -T fields -e wlan_radio.channel 2>/dev/null |
	sort | uniq -c | sort -rn | head -8 | sed 's/^/    /'

echo
echo "  -- probe requests, by source --"
tshark -r "$PCAP" -Y 'wlan.fc.type_subtype == 0x0004' -T fields -e wlan.sa 2>/dev/null |
	sort | uniq -c | sort -rn | head -10 | sed 's/^/    /'

echo
echo "  -- devices that sent a WFD information element (OUI 50-6f-9a, type 0x0a) --"
tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a' \
	-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -10 | sed 's/^/    /'

echo
echo "  -- devices that sent a P2P information element --"
tshark -r "$PCAP" -Y 'wlan.tag.number == 221 && wlan.vendor.oui == 50:6f:9a && wlan.vendor.type == 0x09' \
	-T fields -e wlan.sa 2>/dev/null | sort | uniq -c | sort -rn | head -10 | sed 's/^/    /'

echo
echo "  -- any AltoBeam device (OUI dc:29:19)? --"
tshark -r "$PCAP" -T fields -e wlan.sa 2>/dev/null | grep -i -E '^dc:29:19' | sort -u | sed 's/^/    /' || true
tshark -r "$PCAP" -T fields -e wlan.sa 2>/dev/null | grep -i -E '^dc:29:19' | sort -u | grep -q . ||
	echo "    none: the sink transmits nothing in this band"

echo
echo "  -- distinct source addresses seen --"
tshark -r "$PCAP" -T fields -e wlan.sa 2>/dev/null | sort -u | head -20 | sed 's/^/    /'
