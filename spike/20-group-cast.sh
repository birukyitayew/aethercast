#!/bin/bash
# Castrel spike: a cast over the Wi-Fi Direct group interface.
#
# The reference sink in the end-to-end test connects over the loopback
# interface. A real Miracast sink instead joins our Wi-Fi Direct group and
# connects to the **group interface's** address, so that is the path this
# test exercises.
#
# It needs root for one thing only: the group interface has no address
# until a peer joins, and with no peer to join we have to give it one
# ourselves.
#
# Usage: sudo ./spike/20-group-cast.sh [seconds]

set -uo pipefail

SECS="${1:-12}"
DIR="$(dirname "$0")"
cd "$DIR/.." || exit 1

OUT="${OUT:-/tmp/opencode/sink}"
GROUP_IP="${GROUP_IP:-192.168.43.1}"
GROUP_CIDR="${GROUP_CIDR:-24}"
TS="$OUT/group-received.ts"
mkdir -p "$OUT"
rm -f "$TS"

# Wait for the transport stream to become readable.
#
# The sink writes the file as the packets arrive, so the file can have its
# final size before its tables are readable. Retrying here turns a race
# into a wait, instead of a false failure.
wait_for_streams() { # file timeout_seconds
	local file="$1" limit="$2" waited=0
	while [ "$waited" -lt "$limit" ]; do
		if ffprobe -v error -select_streams v:0 -show_entries stream=id -of csv=p=0 "$file" 2>/dev/null | grep -q .; then
			return 0
		fi
		sleep 1
		waited=$((waited + 1))
	done
	return 1
}

fail=0
check() {
	if [ "$2" = "$3" ]; then
		printf '  PASS  %-46s %s\n' "$1" "$3"
	else
		printf '  FAIL  %-46s expected %s, got %s\n' "$1" "$2" "$3"
		fail=1
	fi
}

if [ ! -x ./target/release/aethercastd ]; then
	cargo build --release || exit 1
fi

# Clean up the address we add to the group interface.
cleanup() {
	ip addr del "$GROUP_IP/$GROUP_CIDR" dev "$IFACE" 2>/dev/null
	wpa_cli -i wlan0 p2p_group_remove '*' >/dev/null 2>&1
}
trap cleanup EXIT

# libpulse aborts on default server discovery on this machine, so the
# PipeWire-Pulse socket has to be named explicitly.
export PULSE_SERVER="unix:${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/pulse/native"
AUDIO_DEVICE="${AUDIO_DEVICE:-alsa_output.pci-0000_00_1f.3.analog-stereo.monitor}"

echo "== starting the source, which forms the group =="
timeout $((SECS + 60)) ./target/release/aethercastd cast --group --port 7236 --seconds "$SECS" \
	--audio --audio-device "$AUDIO_DEVICE" > "$OUT/group-source.log" 2>&1 &
SOURCE=$!

# Wait for the group interface, then give it an address.
IFACE=""
for _ in $(seq 1 40); do
	IFACE=$(ls /sys/class/net 2>/dev/null | grep '^p2p-' | head -1)
	[ -n "$IFACE" ] && break
	sleep 1
done
if [ -z "$IFACE" ]; then
	echo "  no group interface appeared"
	cat "$OUT/group-source.log"
	wait $SOURCE 2>/dev/null
	exit 1
fi
echo "  group interface: $IFACE"
ip link set "$IFACE" up 2>/dev/null
ip addr add "$GROUP_IP/$GROUP_CIDR" dev "$IFACE" 2>/dev/null && echo "  gave it $GROUP_IP" || \
	echo "  (could not set the address; the session will be served on all addresses)"
sleep 1

echo
echo "== starting the reference sink, connecting to the group address =="
python3 spike/18-reference-sink.py "$GROUP_IP" 7236 50000 "$TS" > "$OUT/group-sink.log" 2>&1 &
SINK=$!

wait $SOURCE 2>/dev/null
wait $SINK 2>/dev/null

echo
echo "== what the source did =="
sed 's/^/  /' "$OUT/group-source.log" 2>/dev/null
echo "== what the sink received =="
sed 's/^/  /' "$OUT/group-sink.log" 2>/dev/null

SIZE=$(stat -c %s "$TS" 2>/dev/null || echo 0)
echo
echo "== validating the transport stream =="
if [ "$SIZE" = "0" ]; then
	echo "  no media arrived over the group interface"
	exit 1
fi
echo "  transport stream: $SIZE bytes"
wait_for_streams "$TS" 10 || echo "  (the stream did not become readable)"
VIDEO_PID=$(ffprobe -v error -select_streams v:0 -show_entries stream=id -of csv=p=0 "$TS" 2>/dev/null | head -1)
AUDIO_PID=$(ffprobe -v error -select_streams a:0 -show_entries stream=id -of csv=p=0 "$TS" 2>/dev/null | head -1)
check "the video PID is the WFD one (0x1011)" "0x1011" "$VIDEO_PID"
check "the audio PID is the WFD one (0x1100)" "0x1100" "$AUDIO_PID"
check "the stream came from the group interface" "yes" \
	"$(grep -q 'group' "$OUT/group-source.log" && echo yes || echo no)"

echo
if [ "$fail" = "0" ]; then
	echo "== the source served a conformant session on the group interface =="
else
	echo "== the session did not conform =="
fi
exit "$fail"
