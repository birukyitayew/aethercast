#!/bin/bash
# Regression test: the WFD RTSP handshake against a mock sink.
#
# The real sink is a TV. This test uses `spike/06-mock-sink.py` so the
# handshake in `crates/aethercastd/src/rtsp.rs` can be verified on any
# machine, without hardware, and without a sink in Miracast mode.
#
# Usage: ./spike/07-rtsp-handshake-test.sh

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
BIN="./target/release/aethercastd"
PORT="${PORT:-7236}"
SERVER_LOG="${SERVER_LOG:-/tmp/opencode/rtsp-server.log}"
MOCK_LOG="${MOCK_LOG:-/tmp/opencode/rtsp-mock.log}"
DIR="$(dirname "$SERVER_LOG")"

mkdir -p "$DIR"

if [ ! -x "$BIN" ]; then
	echo "building aethercastd"
	cargo build --release || exit 1
fi

fail=0
check() { # description expected actual
	if [ "$2" = "$3" ]; then
		printf '  PASS  %-44s %s\n' "$1" "$3"
	else
		printf '  FAIL  %-44s expected %s, got %s\n' "$1" "$2" "$3"
		fail=1
	fi
}

rm -f "$SERVER_LOG" "$MOCK_LOG"
"$BIN" rtsp serve --port "$PORT" --seconds 30 >"$SERVER_LOG" 2>&1 &
SERVER=$!
sleep 2

python3 spike/06-mock-sink.py 127.0.0.1 "$PORT" >"$MOCK_LOG" 2>&1
sleep 2
kill "$SERVER" 2>/dev/null
wait "$SERVER" 2>/dev/null

echo "== the message sequence =="
# The direction matters: the source sends M1, M3, M4 and M5, and the sink
# sends M2, M6 and M7.
for message in "-> M1 OPTIONS" "<- M2 OPTIONS" "-> M2 200 OK" "-> M3 GET_PARAMETER" \
	"-> M4 SET_PARAMETER" "-> M5 SET_PARAMETER" "<- M6 SETUP" "<- M7 PLAY"; do
	check "the server logged '$message'" "yes" \
		"$(grep -q -- "$message" "$SERVER_LOG" && echo yes || echo no)"
done

echo
echo "== the sink's capabilities were read =="
check "the RTP port was parsed" "50000" \
	"$(grep -o 'ports=Some([0-9]*)' "$SERVER_LOG" | head -1 | grep -o '[0-9]\+')"
check "the video formats were parsed" "yes" \
	"$(grep -q 'video=Some' "$SERVER_LOG" && echo yes || echo no)"
check "the handshake reached the playing state" "yes" \
	"$(grep -q 'handshake reached Playing' "$SERVER_LOG" && echo yes || echo no)"

echo
if [ "$fail" = "0" ]; then
	echo "== the handshake is complete =="
else
	echo "== the handshake is incomplete =="
	cat "$SERVER_LOG"
fi
exit "$fail"
