# Spike 18 — What the sink actually is, and why it never advertises

**Date:** 2026-10-21
**Device:** CVTE / Hisilicon `Hi3751V350_DMO`, model `SMART_TV`, Android 9 (SDK 28), hardware `bigfish`
**Wireless:** ATBM chip, 2.4 GHz only, associated at 2412 MHz, 65 Mbps
**Artifacts pulled (read-only):** `Miracast.apk`, `libhiwfd.so`, `libwfds.so`, `libhihal_miracast.so`, `ext.jar`, `tvservice.jar`, `cvte.media.jar`

## The short version

The sink speaks **standard** Wi-Fi Display. It is not a proprietary
protocol, and it is not on 5 GHz. It is a **vendor implementation whose
advertising never happens** — so no standard source can find it, which
includes Windows, Android, GNOME Network Displays, and AetherCast.

## What the vendor stack is

| Artifact | What it is |
| --- | --- |
| `com.hisilicon.miracast` | The application. **Resource-only APK** — no `classes.dex`; the code lives in a framework jar |
| `libhiwfd.so` (114 KB, ARM32) | **The Wi-Fi Display sink implementation**, Hisilicon's own AOSP-derived code: `WifiDisplaySink.cpp`, `RTPSink.cpp`, `ANetworkSession.cpp`, `ParsedMessage.cpp` |
| `libhihal_miracast.so` (24 KB) | The **EsPlay** media HAL — the standard Android Miracast media path |
| `libwfds.so` (253 KB) | **A red herring**: the Android Wi-Fi Direct *Services* framework, i.e. the IPP printing backend, not WFD |

None of the three libraries contains the WFA OUI `50:6F:9A`. **The
vendor does not build the information element itself** — the platform
supplicant does, so the element is the standard one.

## The protocol it speaks

Straight out of `libhiwfd.so`:

```text
Public: org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER
Require: org.wfa.wfd1.0
OPTIONS * RTSP/1.0
GET_PARAMETER / SET_PARAMETER
SETUP %s RTSP/1.0
PLAY %s RTSP/1.0
PAUSE %s RTSP/1.0
Session: %s
Content-Type: text/parameters
client-port
```

That is the standard Wi-Fi Display RTSP profile. Port constants found in
the binaries: **7236** (the WFD default RTSP port) in all three
libraries, **7250** (the MS-MICE port) in two, and 7237.

It also implements the HDCP path (`startHDCPListen`, `interruptHDCPListen`)
and publishes a "Sink Info" block:

```text
=============================Sink Info========================
Source IpAddress                 : %s
RTSP Port                        : %d
Sink Rtp Port                    : %d
Source Rtp Port                  : %d
------------------------------wfd capability-------------------
Wfd_content_protection           : %d
Wfd_coupled_sink                 : %d
Wfd_standby_resume_capability    : %d
```

That block is the **MS-MICE** discovery payload: the sink telling a
source where to connect. So the vendor supports both transports, and
there is no mDNS or NSD code in the vendor libraries, so the MS-MICE
advertisement would have to come from the platform.

## Why nothing is advertised

Two independent pieces of evidence, both from the device itself.

**1. The driver refuses to change channel.**

```
atbm_hw_scan:scan, delay suspend v12
atbm_scan_work:end(0)
atbm_config:oper_channel action
atbm_config:channel not chage(be9c2270)(3)
```

Every scan ends with the ATBM driver reporting that the operational
channel did not change. On the sink side that means a Wi-Fi Direct listen
cannot be moved onto a listen channel of its choosing.

**2. The application has no configuration.**

```
com.hisilicon.miracast.business.WifiBusiness: connect L180 ---- no cfg saved ----
```

`no cfg saved` is the application failing to find the configuration its
WFD session needs. The APK carries a **setup wizard** — `WelcomeActivity`
and `step2.png`, `step3.png`, `step4.png` — so the configuration is
presumably created by a first-run wizard that has never been completed.

**3. The state is exclusive.** While the screen-share screen is up, the
box is **off the LAN** and unreachable, and it sends ordinary Wi-Fi probe
requests with **no P2P element, no WFD element and no vendor element at
all** — only SSID, supported rates, extended rates and HT capabilities. A
device that advertises nothing cannot be found by any source.

## What this rules out

| Hypothesis | Verdict |
| --- | --- |
| The sink is on 5 GHz | **Dead** — the chip is 2.4 GHz only, and it is on 2.4 GHz |
| The sink speaks a proprietary protocol | **Dead** — it is standard `org.wfa.wfd1.0` |
| Our WFD element is malformed | **Dead** — it now reads identically under every layout |
| Our radio is broken | **Dead** — the queue is clean, searches complete, and the capture works |
| The sink is a good Miracast sink | **Dead** — it advertises nothing, so no source can find it |

## What is left

The sink is a standard sink that never advertises, most likely because
its setup wizard was never completed, and possibly because the ATBM
driver cannot host a Wi-Fi Direct listen at all.

The next action is on the device: open the Miracast application and
complete its first-run wizard, then look for an mDNS service and for
the sink's RTSP port.

## What this means for AetherCast

1. **Our implementation is standards-correct.** The WFD handshake, the
   media pipeline and the PID map are all validated independently. This
   sink is not a reference implementation to copy from, and not a
   specification to follow differently.
2. **A sink that advertises nothing is a real interoperability class**,
   and AetherCast should say so rather than showing an empty list. The
   detectable states now include: the adapter is 2.4 GHz only; the radio
   work queue is wedged; the sink requires HDCP; the sink speaks another
   protocol; and **the sink is on the network but advertises no Wi-Fi
   Display element**.
3. **The compatibility matrix needs an honest entry** for this model,
   with the evidence, rather than "not supported".

## Reproducing

```bash
# identity and wireless
adb shell getprop ro.product.model
adb shell dumpsys wifi | grep -i 'mWifiInfo'

# the application and its libraries
adb shell pm list packages -f | grep -i miracast
adb pull /system/app/Miracast/Miracast.apk
adb pull /system/lib/libhiwfd.so

# the protocol it speaks
strings -a libhiwfd.so | grep -i -E 'org\.wfa|RTSP|OPTIONS|SETUP'

# the port constants
python3 - <<'EOF'
import struct, pathlib
data = pathlib.Path("libhiwfd.so").read_bytes()
for port in (7236, 7250, 7237):
    for endian in ("<H", ">H"):
        if struct.pack(endian, port) in data:
            print(port, endian)
EOF

# what it says about itself
adb shell logcat -d | grep -i hisilicon.miracast
adb shell dmesg | grep -i -E 'atbm|p2p|wfd'
```
