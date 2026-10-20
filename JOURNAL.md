# AetherCast Build Journal

A running record of decisions, experiments, dead ends, and lessons. Newest entries at the top.

---

## 2026-10-20 — Day 0: Research phase begins

**Goal:** build a better open-source Linux casting app, starting with Miracast source support.

**Decisions**
- v1 scope: Linux Miracast source (laptop → TV).
- Research and architecture pack before product code.
- Public GitHub repository.
- Tech stack and license: decided after the research pack, not by preference.
- Working name: **AetherCast** (provisional; trademark/name check pending).

**Environment**
- Kali Linux 2026.2, XFCE, X11.
- Qualcomm Atheros QCA9565 / AR9565 (`ath9k`), 2.4 GHz, single radio.
- Connected to `Biruk` on channel 1; P2P device `p2p-dev-wlan0`.
- TV: AltoBeam-based Miracast sink, likely `192.168.1.2` / `.8`, currently not discovered by GNOME Network Displays.
- PipeWire 1.6.6; GStreamer has `ximagesrc`, `openh264enc`, `avenc_aac`, `mpegtsmux`, `rtpmp2tpay`; missing `x264enc` and `pipewiresrc`.
- GNOME Network Displays 0.99.0 source checked out at `/tmp/opencode/gnd-src` (tag `0.99.0`).

**GitHub**
- Authenticated `gh` as `birukyitayew`.
- Public repository: [`birukyitayew/aethercast`](https://github.com/birukyitayew/aethercast).

**Work started**
- Five parallel research tracks launched:
  1. Miracast/WFD and Wi-Fi Direct protocol, plus GND/MiracleCast code audit.
  2. Linux Wi-Fi P2P, wpa_supplicant, NetworkManager, driver/channel constraints.
  3. Screen capture, encoding, muxing, audio, A/V sync, GStreamer pipeline.
  4. Google Cast, AirPlay, DLNA, and open-source prior art.
  5. Product, UX, architecture, packaging, security, licensing, CI, and roadmap.
- Project skeleton and documentation structure created.
- Added ADR-0001 (research first, Miracast source first), issue/PR templates, contributing/security/conduct docs, and roadmap/compatibility placeholders.

**Known hard limits to design around**
- HDCP/DRM cannot be bypassed.
- Wi-Fi P2P support varies by driver and chipset.
- Single-radio laptops cannot be on two channels at once.
- Wayland capture requires the ScreenCast portal; X11 needs a fallback.

**Next**
- Connect the public GitHub repository and push the research pack.
- Synthesize the five research notes into an architecture and roadmap.
- Decide the stack and license with evidence.
- Build a minimal discovery + RTSP handshake spike before a full UI.

---

## 2026-10-20 — Milestone 2: the media pipeline conforms

**What we built**

`crates/aethercastd/src/media.rs` builds the real capture → encode → mux → transport pipeline, and `spike/04-media-conformance.sh` proves it against the specification.

```
$ ./spike/04-media-conformance.sh
== 1. MPEG-TS file ==
  PASS  video PID is 0x1011                            0x1011
  PASS  audio PID is 0x1100                            0x1100
  PASS  video codec                                    h264
  PASS  audio codec                                    aac
== 2. PCR cadence (the specification requires 100 ms or less) ==
  PASS  largest PCR gap is at most 100 ms              yes
        largest gap: 33 ms
== 3. RTP transport ==
  PASS  RTP payload type is 33                         33
  PASS  UDP payload is 1336 bytes (7 TS packets)       1336
  PASS  the payload is MPEG-TS (sync byte 0x47)        47
== all checks passed ==
```

**The GStreamer API cost us more than the protocol did**

| Finding | Detail |
| --- | --- |
| GEnum properties | `queue leaky`, `openh264enc rate-control` and `videorate framerate` are enums, and the Rust bindings do not generate them. `set_property_from_str` resolves them by name; the launch parser does too. |
| `videoconvert` will not scale | The reference display is **1821x1024 at 25 fps**, an odd geometry. `videoconvert` refuses the size negotiation, so `videoscale` has to be explicit. |
| `videorate` has no `framerate` | GStreamer 1.28 replaced it with `max-rate` (frames per second). A fixed rate is imposed by a `capsfilter` *after* `videorate`, so it can duplicate as well as drop. |
| The element is `pulsesrc` | `pulseaudio` is the *plugin* name. The GStreamer PulseAudio device provider also enumerates nothing on PipeWire, so the source has to be named explicitly — the system mix is `alsa_output.pci-0000_00_1f.3.analog-stereo.monitor`. |
| The plugin cache can blacklist a plugin | After the distribution upgrade, `libgstpulseaudio.so` stayed at 1.28.3 while the core moved to 1.28.7, and the plugin ended up blacklisted in the registry cache. Clearing `~/.cache/gstreamer-1.0/registry.*.bin` restores it. |
| `mpegtsmux` has no PAT/PMT PID properties | The WFD PIDs come from the **request pad names** — `sink_4113` is 0x1011, `sink_4352` is 0x1100. The PCR lands on the first video PID rather than the 0x1000 the specification asks for: a known gap, tracked. |
| `libpulse` still aborts on default server discovery | The same crash that broke GNOME Network Displays. The PulseAudio socket has to be named explicitly. |

**The finding that changed the default profile**

The first conformance run failed: a **117 ms PCR gap**, against a 100 ms limit. The cause is the encoder, not the muxer — exactly what the research predicted.

| Profile | Largest PCR gap | Verdict |
| --- | --- | --- |
| 1280x720p30 | 133 ms | **fails** — OpenH264 cannot sustain 720p30 on this CPU |
| 640x480p30 | 67 ms, then 33 ms | **passes** |

640x480 is also the format WFD mandates as a minimum, so nothing is given up. **The default profile is now 640x480p30**, and 720p30 is only offered with a hardware encoder, which needs `gstreamer1.0-vaapi` and a driver that supports it.

**The remaining piece**

The pipeline is proven; what is missing is the sink. The next milestone is P2P group formation and the RTSP handshake, so that a real TV can be asked for its capabilities.

---

## 2026-10-20 — Milestone 1: `aethercastd` exists

**What we built**

The first real AetherCast code: a Rust daemon with a CLI, 14 unit tests, and a clean clippy run. It is not a caster yet — it is the layer that decides whether a cast can start at all.

| Module | What it does |
| --- | --- |
| `supplicant.rs` | Client for the `wpa_supplicant` control interface |
| `wfd.rs` | WFD information element: build, parse, and decode device information |
| `radio.rs` | Radio health, wedge detection and recovery, Wi-Fi Direct control |
| `discovery.rs` | Peer model and event parsing |
| `main.rs` | CLI: `doctor`, `arm`, `recover`, `discover` |

**The control interface is not what the documentation suggests.** Four things cost us real time and are worth recording:

1. It is a **datagram** socket (`SOCK_DGRAM`), not a stream. `UnixStream` fails with `Protocol wrong type for socket`.
2. The client must **bind a local address** first. Without it the request is delivered but the reply is lost and the caller blocks forever — `wpa_ctrl_open()` does the same thing.
3. Commands must be sent **without a trailing newline**. The control interface parses the rest of the line as arguments, and the stray newline makes `WFD_SUBELEM_SET` fail on the hex payload.
4. Most commands reply `OK`/`FAIL`, but `STATUS`, `WFD_SUBELEM_GET`, `RADIO_WORK show` and `P2P_PEER` return a **raw document**. And there is no `P2P_PEERS` command: peers are walked with `P2P_PEER FIRST` then `P2P_PEER NEXT-<address>`.

**Two protocol bugs our own tests caught**

- The WFD device information field is a big-endian 16-bit value with **bits numbered from the LSB of the low byte**, not from the first octet. The first implementation had the two bytes swapped, which made GNOME Network Displays' own element decode as "session unavailable".
- `P2P_EVENT_DEVICE_FOUND` carries **two** addresses: the bare address the frame came from, and `p2p_dev_addr`, the peer's Wi-Fi Direct device address. They are not the same value.

**What it does on this machine, right now**

```
$ aethercastd doctor
interface        : wlan0
station          : Biruk (2c:b6:c2:ed:c7:d3)  [connected]
channel          : 2412 MHz
radio-work queue : 0 item(s)
radio health     : ok
wfd element      : armed
  - id=0 body=00101c4400c8
  device type     : source  session available: true  TDLS preferred: false

$ aethercastd discover --duration 25
no peers found in 25s
  radio: healthy, the search ran to completion
  hint: make sure the sink is in Miracast / Screen Share mode
```

The WFD element we advertise is AetherCast's own: source, session available, **TDLS preference cleared** — the bit GNOME Network Displays sets incorrectly.

**Still missing, in order**

1. The sink is still not discovered. The radio is healthy, the search completes, the probe requests carry a WFD IE. The remaining variable is the TV itself.
2. P2P group formation (GO negotiation) — `P2P_GROUP_ADD`, invitation, the sink joining our group.
3. The WFD RTSP server: M1 `OPTIONS`, M2 public device information, M3 session description, M4 accept, M5 establish.
4. The media pipeline, which needs the GStreamer development headers.
5. The GUI, last.

**Next**
- Group formation and the RTSP handshake, so a sink can be *asked* for its capabilities.
- The media pipeline, once `gstreamer1.0-dev` and friends are installed.

---

## 2026-10-20 — Phase 0 spike: the radio-work wedge

**Goal:** answer one question with evidence — does the AltoBeam TV respond to Wi-Fi Display probes? That question turned out to be downstream of a much bigger finding.

**What we found**

1. **GNOME Network Displays' empty sink list is not an application bug.** `wpa_supplicant`'s radio-work queue was wedged: one `p2p-scan` work item stuck at `started=1` for 12,737 seconds (3h31m), with 45 NetworkManager scans queued behind it and growing by one every 5 minutes. Every P2P find was rejected instantly (`P2P: Reject scan trigger since one is already pending`) and even a plain `wpa_cli scan` returned `FAIL-BUSY`. There was not a single `P2P: Scan results received` event in the whole session, and no kernel-side error: the driver accepted the request and never completed it.
2. **Recovery exists and needs no root.** `wpa_cli -i wlan0 FLUSH` aborted the stuck scan: queue 47 → 0, plain scan `FAIL-BUSY` → `OK`, `p2p_find` `FAIL` → `OK` and completing without re-wedging. The station is reactivated automatically afterwards (~10 s).
3. **We can set a global WFD IE without root, and NetworkManager cannot.** `wpa_cli WFD_SUBELEM_SET all <hex>` stores a WFD-IE body in wpa_supplicant; `wifi_display_update_wfd_ie()` propagates it into `p2p_set_wfd_ie_probe_req()`, and `p2p_scan_ie()` adds it to every P2P find scan's probe requests (`params->p2p_probe = 1`). **Our discovery probes now carry a real WFD IE** — something the existing app can never do, because NM exposes no API for it. The subelement ID space is 0–11 (`MAX_WFD_SUBELEMS`).
4. **The TV has still not been seen.** Discovery now runs cleanly, but zero peers, zero `P2P_EVENT_DEVICE_FOUND`. The remaining suspects are the TV not being in Miracast mode, and the single-radio channel constraint.

**Delivered**
- `spike/01-radio-work-wedge-report.md` — full technical report with source-level references
- `spike/02-recover-radio-work.sh` — automated wedge recovery
- `spike/03-discovery-window.sh` — supervised discovery window
- `spike/evidence/radio-work-wedged.txt` — the captured queue

**Next**
- Put the TV in Miracast mode and re-run the discovery window with the WFD IE armed.
- If the sink still does not appear, capture the air in monitor mode to see whether it answers a WFD-bearing probe at all.
- Design implication for v1: the "no sinks" screen must diagnose the radio, not just list devices.

---

## 2026-10-20 — Research phase complete

**What landed**
Five cited research notes, ~30,000 words, 393 URLs, 393+ line-level claims labelled verified / reported / speculative:

| Note | Words |
| --- | --- |
| 01 — Miracast / WFD protocol + GND code audit | 5,999 |
| 02 — Linux Wi-Fi P2P, drivers, channel constraints | 5,981 |
| 03 — Media pipeline, codecs, latency | 5,958 |
| 04 — Google Cast, AirPlay, DLNA, prior art | 6,334 |
| 05 — Product, architecture, packaging, license, name | 5,999 |

**The findings that matter**
1. Discovery fails *before* RTSP. The machine has no global WFD IE (`wpa_cli get WFDIEs` → `FAIL`); NetworkManager only applies `wfd-ies` at activation, never while scanning. Prime suspect for "discovers no sinks".
2. The radio is the binding constraint. `ath9k` on the QCA9565/AR9565 is `#channels <= 1`; a P2P group on another channel fails with `-2`. Discovery off-channel still works via ROC.
3. NetworkManager cannot express GO intent, operating frequency, or peer channel — a narrow helper is required.
4. Software encode is 720p30-only and only with x264. Measured here: OpenH264 0.32–0.81× realtime at 720p30, x264 2.64×. `vah264enc` is missing from stock GStreamer 1.28.3.
5. GND's negotiation core has four high-severity defects (no global WFD IE in discovery, second RTP port misread as RTCP, resolution selection compiled out, 512 kbit/s clamp). Forking inherits them.
6. "AetherCast" collides with `ubports/aethercast` (archived 2022) and Canonical's 2015 Ubuntu Miracast project. Dormant, not live. Name kept, risk recorded in ADR-0004.

**Decisions taken (maintainer sign-off)**
- **License:** GPL-3.0-or-later, DCO, no CLA (ADR-0003 → Accepted).
- **Architecture:** Rust control plane + GStreamer media plane, daemon + versioned D-Bus + GTK4 + CLI (ADR-0002 → Accepted).
- **Name:** the project is **AetherCast** and the repository is `birukyitayew/aethercast`. The prior-art collision (`ubports/aethercast`, Canonical 2015) is dormant and is recorded as an accepted risk with a clearance gate and an explicit rename trigger in ADR-0004.

**Decisions recorded**
- ADR-0002 (Proposed): Rust control plane, GStreamer media plane, daemon + versioned D-Bus + GTK4 + CLI.
- ADR-0003 (Proposed): GPL-3.0-or-later, DCO, no CLA, no DRM circumvention, no proprietary codec bundling.
- ADR-0004 (Accepted): the project keeps the name AetherCast; the prior-art collision is an accepted risk with a clearance gate before the first release and an explicit rename trigger.

**Next**
- Milestone 1: a headless discovery spike that sets the global WFD IE and captures a scan, to settle whether the AltoBeam TV answers WFD probes at all.
- Then a minimal WFD M1–M3 exchange to read the sink's real capability bitmap.
- Then the first frame, then the UI.
