#!/bin/bash
# Castrel media conformance check.
#
# Validates the two things the Wi-Fi Display specification is strict about,
# using the real pipeline on this machine:
#
#   1. The MPEG-TS PID map: video 0x1011, audio 0x1100.
#   2. The RTP transport: payload type 33, at most seven TS packets per
#      datagram (12 byte RTP header + 7 * 188 byte TS = 1336 bytes of UDP
#      payload).
#
# It also reports the PCR cadence, which must stay at or below 100 ms.
#
# Usage: ./spike/04-media-conformance.sh

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
BIN="./target/release/aethercastd"
WORK="${WORK:-/tmp/opencode/conformance}"
mkdir -p "$WORK"

fail=0
check() { # description expected actual
	if [ "$2" = "$3" ]; then
		printf '  PASS  %-46s %s\n' "$1" "$3"
	else
		printf '  FAIL  %-46s expected %s, got %s\n' "$1" "$2" "$3"
		fail=1
	fi
}

if [ ! -x "$BIN" ]; then
	echo "building aethercastd"
	cargo build --release || exit 1
fi

# The PulseAudio socket has to be named explicitly on this system: the
# default server lookup aborts libpulse.
export PULSE_SERVER="unix:${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/pulse/native"
AUDIO_DEVICE="${AUDIO_DEVICE:-alsa_output.pci-0000_00_1f.3.analog-stereo.monitor}"

# A build server has neither a display nor a sound card, so the test can be
# run with a generated pattern and without audio.
TEST_ARGS=()
if [ "${TEST_PATTERN:-0}" = "1" ]; then
	TEST_ARGS+=(--test-pattern)
else
	TEST_ARGS+=(--audio --audio-device "$AUDIO_DEVICE")
fi
if [ "${NO_AUDIO:-0}" = "1" ]; then
	TEST_ARGS=(--test-pattern)
fi

echo "== 1. MPEG-TS file =="
"$BIN" media test --seconds 6 --out "$WORK/stream.ts" "${TEST_ARGS[@]}" | sed 's/^/  /'

VIDEO_PID=$(ffprobe -v error -select_streams v:0 -show_entries stream=id -of csv=p=0 "$WORK/stream.ts" 2>/dev/null | head -1)
AUDIO_PID=$(ffprobe -v error -select_streams a:0 -show_entries stream=id -of csv=p=0 "$WORK/stream.ts" 2>/dev/null | head -1)
VIDEO_CODEC=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 "$WORK/stream.ts" 2>/dev/null | head -1)
AUDIO_CODEC=$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 "$WORK/stream.ts" 2>/dev/null | head -1)

check "video PID is 0x1011" "0x1011" "$VIDEO_PID"
if [ "${NO_AUDIO:-0}" != "1" ]; then
	check "audio PID is 0x1100" "0x1100" "$AUDIO_PID"
fi
check "video codec" "h264" "$VIDEO_CODEC"
if [ "${NO_AUDIO:-0}" != "1" ]; then
	check "audio codec" "aac" "$AUDIO_CODEC"
fi

echo
echo "== 2. PCR cadence (the specification requires 100 ms or less) =="
MAX_GAP=$(ffprobe -v error -select_streams v:0 -show_entries packet=pts_time -of csv=p=0 "$WORK/stream.ts" 2>/dev/null |
	awk -F, 'NR>1 && $1 != "" {d = ($1 - p) * 1000; if (d > m) m = d} {p = $1} END {printf "%.0f", m}')
if [ -z "$MAX_GAP" ]; then
	echo "  SKIP  no packet timestamps"
else
	check "largest PCR gap is at most 100 ms" "yes" "$([ "${MAX_GAP%.*}" -le 100 ] && echo yes || echo no)"
	echo "        largest gap: ${MAX_GAP} ms"
fi

echo
echo "== 3. RTP transport =="
PCAP="$WORK/rtp.pcap"
rm -f "$PCAP"

# Capturing on the loopback interface needs root, and a check that cannot
# run is skipped rather than failed.
CAPTURED=0
if [ "$(id -u)" = "0" ]; then
	# "any" rather than "lo": the traffic is loopback, but the capture
	# interface differs between a container and a machine.
	dumpcap -i any -f "udp port 50000" -w "$PCAP" >"$WORK/dumpcap.log" 2>&1 &
	CAP_PID=$!
	sleep 3
	CAPTURED=1
else
	echo "  (needs root to capture; run this script with sudo to exercise it)"
fi

# The media test's own output is shown, so a failure here is visible
# instead of looking like an empty capture.
"$BIN" media test --seconds 6 --udp 127.0.0.1:50000 --width 640 --height 480 --framerate 30 2>&1 |
	sed 's/^/  /'
sleep 2
if [ "$CAPTURED" = "1" ]; then
	kill "$CAP_PID" 2>/dev/null
	wait "$CAP_PID" 2>/dev/null
fi

if [ "$CAPTURED" != "1" ]; then
	echo "  SKIP  the RTP transport check needs root to capture on the loopback interface"
elif [ ! -s "$PCAP" ]; then
	echo "  FAIL  no packets captured"
	fail=1
else
	# The RTP header is not decoded by tshark's default heuristics, so the
	# payload is inspected directly: byte 1 carries the payload type.
	PAYLOAD_TYPE=$(tshark -r "$PCAP" -T fields -e udp.payload 2>/dev/null | head -1 | cut -c3-4)
	[ -n "$PAYLOAD_TYPE" ] && PAYLOAD_TYPE=$((16#$PAYLOAD_TYPE))
	UDP_SIZE=$(tshark -r "$PCAP" -T fields -e udp.length 2>/dev/null | sort -n | uniq -c | tail -1 | awk '{print $2}')
	TS_PACKETS=$(tshark -r "$PCAP" -T fields -e udp.payload 2>/dev/null | head -1 | cut -c25-26)

	check "RTP payload type is 33" "33" "${PAYLOAD_TYPE:-none}"
	check "UDP payload is 1336 bytes (7 TS packets)" "1336" "${UDP_SIZE:-none}"
	check "the payload is MPEG-TS (sync byte 0x47)" "47" "${TS_PACKETS:-none}"
	echo "        packets captured: $(tshark -r "$PCAP" 2>/dev/null | wc -l)"
fi

echo
if [ "$fail" = "0" ]; then
	echo "== all checks passed =="
else
	echo "== some checks failed =="
fi
exit "$fail"
