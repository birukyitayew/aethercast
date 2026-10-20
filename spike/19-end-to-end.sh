#!/bin/bash
# Castrel end-to-end test: source -> reference sink.
#
# This exercises the whole product path with no hardware:
#
#   screen capture -> H.264 encode -> MPEG-TS with the WFD PID map
#   -> RTP (payload type 33) -> the reference sink -> a transport stream
#   that ffprobe validates.
#
# It is the test the project can run in CI, and the one that proves the
# media path independently of any sink's willingness to cooperate.
#
# Usage: ./spike/19-end-to-end.sh [seconds]

set -uo pipefail

SECS="${1:-12}"
DIR="$(dirname "$0")"
cd "$DIR/.." || exit 1

OUT="${OUT:-/tmp/opencode/sink}"
mkdir -p "$OUT"
TS="$OUT/received.ts"
LOG="$OUT/end-to-end.log"
rm -f "$TS" "$LOG"

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
check() { # description expected actual
	if [ "$2" = "$3" ]; then
		printf '  PASS  %-46s %s\n' "$1" "$3"
	else
		printf '  FAIL  %-46s expected %s, got %s\n' "$1" "$2" "$3"
		fail=1
	fi
}

if [ ! -x ./target/release/aethercastd ]; then
	echo "building aethercastd"
	cargo build --release || exit 1
fi

# libpulse aborts on default server discovery on this machine, so the
# PipeAudio socket has to be named explicitly.
export PULSE_SERVER="unix:${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/pulse/native"
AUDIO_DEVICE="${AUDIO_DEVICE:-alsa_output.pci-0000_00_1f.3.analog-stereo.monitor}"

# A build server has neither a display nor a sound card, so the test can be
# run with a generated pattern and without audio.
SOURCE_ARGS=()
if [ "${TEST_PATTERN:-0}" = "1" ]; then
	SOURCE_ARGS+=(--test-pattern)
else
	SOURCE_ARGS+=(--audio --audio-device "$AUDIO_DEVICE")
fi
if [ "${NO_AUDIO:-0}" = "1" ]; then
	SOURCE_ARGS=(--test-pattern)
fi

echo "== starting the source =="
timeout $((SECS + 40)) ./target/release/aethercastd cast --port 7236 --seconds "$SECS" \
	"${SOURCE_ARGS[@]}" > "$OUT/source.log" 2>&1 &
SOURCE=$!
sleep 2

echo "== starting the reference sink =="
python3 spike/18-reference-sink.py 127.0.0.1 7236 50000 "$TS" > "$OUT/sink.log" 2>&1 &
SINK=$!

wait $SOURCE 2>/dev/null
wait $SINK 2>/dev/null
echo "== what the source did =="
sed 's/^/  /' "$OUT/source.log" 2>/dev/null

echo
echo "== what the sink received =="
sed 's/^/  /' "$OUT/sink.log" 2>/dev/null
SIZE=$(stat -c %s "$TS" 2>/dev/null || echo 0)
echo "  transport stream: $SIZE bytes"

if [ "$SIZE" = "0" ]; then
	echo
	echo "== no media arrived =="
	exit 1
fi

echo
echo "== validating the transport stream =="
wait_for_streams "$TS" 10 || echo "  (the stream did not become readable)"
PROBE=$(ffprobe -v error -show_entries stream=index,codec_name,codec_type,id,width,height -of csv=p=0 "$TS" 2>/dev/null)
echo "$PROBE" | sed 's/^/  /'
VIDEO_PID=$(ffprobe -v error -select_streams v:0 -show_entries stream=id -of csv=p=0 "$TS" 2>/dev/null | head -1)
AUDIO_PID=$(ffprobe -v error -select_streams a:0 -show_entries stream=id -of csv=p=0 "$TS" 2>/dev/null | head -1)
VIDEO_CODEC=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 "$TS" 2>/dev/null | head -1)
AUDIO_CODEC=$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 "$TS" 2>/dev/null | head -1)

check "the video PID is the WFD one (0x1011)" "0x1011" "$VIDEO_PID"
check "the video codec" "h264" "$VIDEO_CODEC"
if [ "${NO_AUDIO:-0}" != "1" ]; then
	check "the audio PID is the WFD one (0x1100)" "0x1100" "$AUDIO_PID"
	check "the audio codec" "aac" "$AUDIO_CODEC"
fi

echo
if [ "$fail" = "0" ]; then
	echo "== the source cast a conformant stream to a sink =="
else
	echo "== the stream did not conform =="
fi
exit "$fail"
