# Monitor-mode capture — channel 1

**Date:** 2026-10-20
**Tool:** tcpdump (the dumpcap fallback, because the capture directory is not writable by root's dropped privileges)
**Interface:** wlan0 in monitor mode, channel 1 (2412 MHz)
**Frames:** 30,792

## What was on the air

| Frame type | Count |
| --- | --- |
| 0x0028 | 13,128 |
| 0x0019 | 9,021 |
| 0x001c | 4,863 |
| 0x001b | 3,233 |
| 0x0008 (beacon) | 339 |
| 0x0018 | 123 |

| Observation | Result |
| --- | --- |
| Beacon senders | `2c:b6:c2:ed:c7:d3` only — the `Biruk` access point, 339 beacons |
| Beacon SSID | `426972756b` (hex for `Biruk`) |
| Probe requests | 2, both from `e2:88:4c:36:21:22` — a phone with a randomised MAC |
| **WFD information elements** | **none** |
| **P2P information elements** | **none** |
| **AltoBeam (OUI `dc:29:19`) transmitters** | **none** |
| Distinct source addresses | `2c:b6:c2:ed:c7:d3` (the AP), `e2:88:4c:36:21:22` (a phone), `16:a0:d5:2f:3a:b8` (the other LAN client) |

## What this does and does not prove

It proves the capture works: the access point's beacons are there, in the
expected count, and the radio was genuinely in monitor mode.

It does **not** prove the sink is absent from the band, because the
capture only covered **channel 1**. A monitor-mode interface stays on the
channel it is set to, and a Miracast sink advertises on a Wi-Fi Direct
*listen* channel — the P2P social channels are 1, 6 and 11. The `Nebyu`
network in this building is on channel 6 and was not visible either, which
is the same effect and confirms the sweep is needed.

The AltoBeam device was not transmitting on channel 1, which is consistent
with it being in Miracast mode: it leaves the access point to form a
Wi-Fi Direct group, so it is no longer on our network and no longer on
our channel.

Next: `spike/10-monitor-sweep.sh` captures channels 1, 6 and 11 in turn.
