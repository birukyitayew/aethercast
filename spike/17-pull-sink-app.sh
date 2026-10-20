#!/bin/bash
# Wait for the sink to rejoin the LAN, then pull its Miracast application.
#
# The sink is off the LAN while its screen-share screen is up, and its
# service stops when the screen closes. So the application can only be
# pulled while the box is back on the network - this waits for that.
#
# Nothing on the device is changed: the pull is a read.
#
# Usage: ./spike/17-pull-sink-app.sh [minutes]

set -uo pipefail

MINUTES="${1:-10}"
OUT="${OUT:-/tmp/opencode/sink}"
mkdir -p "$OUT"

echo "waiting up to $MINUTES minutes for the sink to rejoin the LAN"
deadline=$((SECONDS + MINUTES * 60))
found=""

while [ "$SECONDS" -lt "$deadline" ]; do
	for ip in 192.168.1.2 192.168.1.8; do
		if ping -c1 -W1 "$ip" >/dev/null 2>&1; then
			found="$ip"
			break
		fi
	done
	[ -n "$found" ] && break
	sleep 5
done

if [ -z "$found" ]; then
	echo "the sink did not rejoin within $MINUTES minutes"
	exit 1
fi

echo "the sink is back at $found"
timeout 20s adb connect "$found:5555" 2>&1 | tr -d '\r'
sleep 2

echo
echo "== the application on the device =="
timeout 20s adb -s "$found:5555" shell 'pm list packages -f | grep -i miracast' 2>&1 | tr -d '\r' | sed 's/^/  /'

APK=$(timeout 20s adb -s "$found:5555" shell 'pm path com.hisilicon.miracast' 2>/dev/null |
	tr -d '\r' | sed 's/^package://')
APK="${APK:-/system/app/Miracast/Miracast.apk}"

echo
echo "== pulling $APK =="
timeout 120s adb -s "$found:5555" pull "$APK" "$OUT/Miracast.apk" 2>&1 | tr -d '\r' | sed 's/^/  /'
ls -lh "$OUT/Miracast.apk" 2>/dev/null | sed 's/^/  /'

echo
echo "== the other vendor applications, for context =="
timeout 20s adb -s "$found:5555" shell 'ls /system/app' 2>&1 | tr -d '\r' | grep -i -E 'miracast|dlna|media|cast|share' | sed 's/^/  /'

echo
echo "== the service's own configuration, if it has any =="
timeout 20s adb -s "$found:5555" shell 'ls /data/data/com.hisilicon.miracast 2>/dev/null; ls /sdcard/ 2>/dev/null | head' 2>&1 | tr -d '\r' | sed 's/^/  /'

echo
echo "the application is at $OUT/Miracast.apk"
