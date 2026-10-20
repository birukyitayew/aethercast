#!/bin/bash
# Run the read-only Android sink survey over ADB.
#
# Nothing here changes a setting, starts a service, or stops one. The
# commands are the ones listed in spike/12-android-sink-survey.md.
#
# Usage: ./spike/13-android-survey.sh [serial-or-ip]

set -uo pipefail

TARGET="${1:-}"
SERIAL=""
if [ -n "$TARGET" ]; then
	SERIAL="$TARGET"
fi

run() {
	if [ -n "$SERIAL" ]; then
		adb -s "$SERIAL" "$@" 2>&1
	else
		adb "$@" 2>&1
	fi
}

echo "== devices =="
adb devices -l 2>&1 | tr -d '\r'
if [ -n "$SERIAL" ]; then
	adb connect "$SERIAL" 2>&1 | tr -d '\r'
	sleep 2
fi

echo
echo "== 1. identity =="
for prop in ro.product.manufacturer ro.product.model ro.build.version.release \
	ro.build.version.sdk ro.hardware ro.board.platform; do
	printf '  %-28s %s\n' "$prop" "$(run shell getprop "$prop" | tr -d '\r')"
done

echo
echo "== 2. which band is the box on? =="
run shell cmd wifi status 2>&1 | sed 's/^/  /'
echo "  -- /proc/net/wireless --"
run shell cat /proc/net/wireless 2>&1 | sed 's/^/  /'
echo "  -- frequency and band in dumpsys wifi --"
run shell dumpsys wifi 2>&1 | grep -iE 'frequency|MHz|band|5GHz|2\.4GHz|WifiInfo' | head -20 | sed 's/^/  /'

echo
echo "== 3. is the Wi-Fi Display stack running? =="
echo "  -- registered services --"
run shell service list 2>&1 | grep -iE 'wfd|miracast' | sed 's/^/  /'
echo "  -- processes --"
run shell ps -A 2>&1 | grep -iE 'wfd|miracast|wifid' | sed 's/^/  /'
echo "  -- cmd wfd status --"
run shell cmd wfd status 2>&1 | sed 's/^/  /'
echo "  -- dumpsys wfd --"
run shell dumpsys wfd 2>&1 | head -30 | sed 's/^/  /'

echo
echo "== 4. what did the stack say about itself? =="
echo "  -- kernel log --"
run shell dmesg 2>&1 | grep -iE 'wfd|p2p|miracast' | tail -30 | sed 's/^/  /'
echo "  -- logcat --"
run shell logcat -d 2>&1 | grep -iE 'wfd|miracast|WifiDisplay|WifiP2p' | tail -40 | sed 's/^/  /'

echo
echo "== 5. which protocols does the box speak? =="
run shell pm list packages 2>&1 | grep -iE 'cast|airplay|dlna|mediashare|miracast|wfd|screen' | sed 's/^/  /'
echo "  -- properties --"
run shell getprop 2>&1 | grep -iE 'wfd|miracast|chromecast|airplay|dlna|cast' | head -20 | sed 's/^/  /'

echo
echo "== done =="
