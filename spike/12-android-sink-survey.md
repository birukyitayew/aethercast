# Surveying an Android sink, read-only

The sink is an Android TV box (`android_b313`). Android gives us a way to
look inside its wireless stack that no amount of packet capture can: the
`wfd` service, the kernel log, and the package list.

Everything below is **read-only**. Nothing here changes a setting, starts
a service, or stops one.

## 0. Connect

```bash
adb devices                       # the box must show as "device"
adb shell getprop ro.product.model
```

If the box is not listed, it is usually because it has never been
authorised for this computer: accept the prompt on the TV, or run
`adb kill-server && adb start-server` and reconnect.

## 1. Which band is the box on?

```bash
adb shell cmd wifi status
adb shell cat /proc/net/wireless
adb shell dumpsys wifi | grep -iE 'frequency|MHz|band|5GHz|2\.4GHz'
```

`/proc/net/wireless` reports the negotiated frequency, and `cmd wifi
status` reports the band and the link speed. This answers whether the
box is associated on 2.4 GHz or 5 GHz, which is the difference between
"its Miracast listener is invisible to us" and "its Miracast should have
been visible".

## 2. Is the Wi-Fi Display stack actually running?

This is the decisive test for the hypothesis that the box's screen says
"waiting for connection" while its P2P stack never started.

```bash
adb shell service list | grep -iE 'wfd|miracast'
adb shell ps -A | grep -iE 'wfd|miracast'
adb shell cmd wfd status
adb shell dumpsys wfd 2>/dev/null | head -40
```

`service list` shows whether a Wi-Fi Display service is registered at
all. `cmd wfd status` prints the sink and source state, including the
negotiated channel and the session state.

## 3. What did the stack say about itself?

```bash
adb shell dmesg | grep -iE 'wfd|p2p|miracast' | tail -40
adb shell logcat -d | grep -iE 'wfd|miracast|WifiDisplay|WifiP2p' | tail -60
```

`logcat -d` dumps the existing buffer and does not clear it, so nothing
is lost. A sink that is up and advertising will show its WFD element and
the channel it chose.

## 4. What protocols does the box actually support?

```bash
adb shell pm list packages | grep -iE 'cast|airplay|dlna|mediashare|miracast|wfd|screen'
adb shell getprop | grep -iE 'wfd|miracast|chromecast|airplay|dlna|cast'
```

This distinguishes three very different boxes:

- one with the Google Cast receiver (`com.google.android.tv.cast`),
- one with an AirPlay receiver,
- one with a vendor "MediaShare"/DLNA app and nothing else,

and it tells us whether the "screen sharing" menu is wired to
Miracast at all.

## 5. Identity and the wireless chip

```bash
adb shell getprop ro.product.manufacturer
adb shell getprop ro.product.model
adb shell getprop ro.build.version.release
adb shell getprop ro.build.version.sdk
adb shell getprop ro.hardware
adb shell getprop ro.board.platform
adb shell dumpsys wifi | grep -iE 'hal|driver|chipset'
```

The chipset decides which Wi-Fi Direct roles it can play, and the
Android version decides which AOSP WFD implementation we are dealing
with.

## What not to run

Read-only means read-only. Avoid all of these:

```bash
adb shell svc wifi disable            # takes the box offline
adb shell cmd wifi set-wifi-enabled disabled
adb shell cmd wfd enable-sink         # changes the WFD state
adb shell settings put global ...     # writes settings
adb shell pm enable|disable|uninstall # changes what is installed
adb shell am start ...                # starts apps
adb root                              # not needed, and it is a change
```

## Afterwards: turn ADB off

ADB over the network is a serious exposure — anyone on the LAN gets a
shell on the box. When the survey is done:

1. `adb disconnect` and `adb kill-server` on this machine.
2. On the box: Settings → Developer options → **USB debugging** off, and
   **ADB over network** / **Wireless debugging** off.

## Why this matters to AetherCast

Whatever the survey shows, the product has to answer the user's question
instead of showing an empty list. The states we can now name are:

- the sink's Wi-Fi Direct stack is not running at all;
- the sink advertises on 5 GHz and the adapter is 2.4 GHz only;
- the sink speaks a different protocol entirely, and AetherCast does not
  implement it yet;
- the sink requires HDCP, which AetherCast cannot provide.
