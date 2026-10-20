# Spike 01 — The wpa_supplicant radio-work wedge

**Date:** 2026-10-20
**Machine:** Kali Linux 2026.2, XFCE/X11, Qualcomm Atheros QCA9565/AR9565 (`ath9k`), 2.4 GHz only, NetworkManager 1.56.1, wpa_supplicant 2.10
**Symptom under investigation:** GNOME Network Displays 0.99.0 discovers **no sinks** on this machine.

## Summary

The empty sink list is **not** a bug in the Miracast application layer. It is a **wedged scan queue inside `wpa_supplicant`**, and the wedge is permanent: once the first P2P discovery scan fails to complete, every later scan request is rejected instantly and never recovers on its own.

GNOME Network Displays has no way to observe or report this. It shows an empty list, exactly as a user would expect when a TV is switched off.

## Evidence

### 1. The queue is stuck

`wpa_cli -i wlan0 RADIO_WORK show` (a wpa_supplicant 2.10 control command that dumps the radio-work queue) after 3.5 hours of use:

```text
p2p-scan@wlan0:0:1:12737.225102     <- started=1, age 12,737 s (3h31m)
scan@wlan0:0:0:12736.815721         <- queued behind it
scan@wlan0:0:0:12622.368174
...                                  <- one every ~300 s (NetworkManager's periodic scan)
```

47 items total: **one** `p2p-scan` that started and never finished, and **45** queued `scan` items behind it. Full capture: [`evidence/radio-work-wedged.txt`](evidence/radio-work-wedged.txt).

Field format is `type@iface:freq:started:age.seconds` — `started=1` means the work is in flight and never completed.

### 2. Every request is rejected

```text
$ wpa_cli -i wlan0 p2p_find 15
FAIL

$ wpa_cli -i wlan0 scan
FAIL-BUSY
```

The supplicant journal shows the reason, once per attempt:

```text
wlan0: P2P: Reject scan trigger since one is already pending
```

That message comes from `wpa_supplicant/p2p_supplicant.c:423`:

```c
	if (wpa_s->p2p_scan_work) {
		wpa_dbg(wpa_s, MSG_INFO, "P2P: Reject scan trigger since one is already pending");
		return -1;
	}
```

### 3. The scan never returned results

Across the whole session there is **not one** `P2P: Scan results received` event, and no kernel-side ath9k or mac80211 error. The driver accepted the request and simply never completed it.

The wedge dates from **14:25**, which is the first GNOME Network Displays discovery attempt of the day. Every attempt after that — 4 rejected P2P finds in the last 2 hours alone — inherited the broken state.

### 4. The queue grows without bound

NetworkManager requests a Wi-Fi scan roughly every 5 minutes. Each one is queued behind the stuck `p2p-scan` and never runs, so the queue grows monotonically: 46 items at 17:52, 47 at 17:58. This is also a slow memory leak in the supplicant, and a second symptom users would eventually report as "the Wi-Fi is slow".

## Recovery

`wpa_cli -i wlan0 FLUSH` aborts the stuck scan and resets P2P state ([`spike/02-recover-radio-work.sh`](02-recover-radio-work.sh)):

| Metric | Before | After |
| --- | --- | --- |
| Queued radio work | 47 | 0 |
| Plain `wpa_cli scan` | `FAIL-BUSY` | `OK` |
| `p2p_find` | `FAIL` (rejected) | `OK` (completes) |
| Scan results | cached AP only | live scan, found a second AP on channel 6 |

The flush drops the station, so the connection is reactivated afterwards. Recovery takes about 10 seconds and needs no root: the `wpa_supplicant` control socket is group-writable for members of `netdev`.

## The second finding: how to get a WFD IE onto the wire

While the wedge was in place we also established how to set a **global WFD information element** in `wpa_supplicant` — something NetworkManager exposes no API for, and something GNOME Network Displays never does.

```bash
wpa_cli -i wlan0 WFD_SUBELEM_SET all 00000600901c4400c8
wpa_cli -i wlan0 WFD_SUBELEM_GET all
# -> 00000600901c4400c8
```

The chain, verified in `wpa_supplicant` 2.10 source:

| Step | Location |
| --- | --- |
| `WFD_SUBELEM_SET all <hex>` stores the WFD-IE body | `wpa_supplicant/wifi_display.c:255` `wifi_display_subelem_set` |
| Body is parsed as `id(1) | length(2, BE) | body` | `wpa_supplicant/wifi_display.c:315` `wifi_display_subelem_set_from_ies` |
| `wifi_display_update_wfd_ie()` builds the per-frame IEs | `wpa_supplicant/wifi_display.c:67` |
| Probe-request IE is set on the P2P core | `wpa_supplicant/wifi_display.c:199` `p2p_set_wfd_ie_probe_req` |
| Probe-request IE is added to every P2P find scan | `wpa_supplicant/p2p_supplicant.c:499` `p2p_scan_ie` |
| The scan is flagged so the driver uses these IEs | `wpa_supplicant/p2p_supplicant.c:501` `params->p2p_probe = 1` |

**Consequence:** after `WFD_SUBELEM_SET`, our P2P discovery probe requests carry a real WFD IE. Before it, they did not. A strict Miracast sink is entitled to ignore probes that do not carry one (WFD v2.1 §4.3.2: *"A WFD Device shall include the WFD Information Element (WFD IE) in all Beacon, Probe Request and Probe Response frames."*).

The IE used is the one GNOME Network Displays ships (`00 00 06 00 90 1c 44 00 c8` = Device Information + Capability Discovery + Application Information), so the spike is directly comparable to what the existing app sends.

The subelement ID space is `0..11` (`MAX_WFD_SUBELEMS == 12`); the mapping is in `src/common/ieee802_11_defs.h:1758`:

| ID | Subelement | ID | Subelement |
| --- | --- | --- | --- |
| 0 | Device Information | 6 | Coupled Sink |
| 1 | Associated BSSID | 7 | Extended Capability |
| 2 | Audio Formats | 8 | Local IP Address |
| 3 | Video Formats | 9 | Session Information |
| 4 | 3D Video Formats | 10 | MAC Information |
| 5 | Content Protection | 11 | R2 Device Information |

## What this means for Castrel

1. **A "no sinks found" screen must have a diagnosis.** The single most useful thing Castrel can do on this hardware is say *"the Wi-Fi driver stopped completing scans; recovered by flushing the radio-work queue"* — instead of showing an empty list.
2. **Detect the wedge, don't retry blindly.** A P2P find that is rejected instantly, or a scan that returns `FAIL-BUSY`, is a hard signal to run the recovery path, not to try again.
3. **Health-check the radio work queue.** `RADIO_WORK show` is a cheap, unprivileged probe. A queue that only grows, or a `p2p-scan` with `started=1` for more than a few seconds, is a fault condition.
4. **Set the global WFD IE ourselves.** It is unprivileged, it is the difference between a discoverable and an undiscoverable source, and NetworkManager cannot do it.
5. **Do not let the WFD IE get clobbered.** P2P state changes can clear it; re-arm it before every discovery round and verify with `WFD_SUBELEM_GET all`.

## Open questions

- Does the wedge reproduce with a plain `wpa_cli scan` on this radio, or only with the P2P scan path? The plain scan worked once the queue was flushed; it has not been tested as the *first* scan after boot.
- Is the 5 GHz band or a different channel a factor? The reference laptop is 2.4 GHz only.
- Does the wedge also occur on `ath10k`/`iwlwifi`, or is it specific to `ath9k`'s single-channel combination?
- Does the TV answer once it is in Miracast mode and our probe requests carry the WFD IE? — the open question this spike was built to answer.

## References

- `wpa_supplicant` 2.10 source, `wpa_supplicant/wifi_display.c`, `wpa_supplicant/p2p_supplicant.c`, `wpa_supplicant/ctrl_iface.c`, `src/p2p/p2p.c`, `src/common/ieee802_11_defs.h` (Debian `wpa_2.10.orig.tar.xz`)
- [`docs/research/01-miracast-wfd.md`](../docs/research/01-miracast-wfd.md) §2, §4
- [`docs/research/02-linux-wifi-p2p.md`](../docs/research/02-linux-wifi-p2p.md) §4–6
- Wi-Fi Display Technical Specification v2.1 (2017-07-31), §4.3.2
