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

## 2026-10-20 — The sink is an Android box, and AOSP's WFD sink is talkative

The TV's screen says **"waiting for connection"** and names the device
**`android_b313`**. That name is the AOSP convention, so the sink is an
**Android TV or an Android TV box** — which fits the AltoBeam Wi-Fi chip
perfectly (AltoBeam is common in low-cost Android boxes and dongles).

That is not a cosmetic detail; it changes what we should expect:

- Android's Wi-Fi Display sink is **AOSP's** `WifiDisplaySinkController`
  and `libwfd`.
- **AOSP's sink sends periodic probe requests carrying its own WFD
  information element** while it is waiting for a source. It is one of
  the chattier WFD implementations.
- Our capture saw **zero** probe requests from any device across three
  channels and ~45 seconds — and only two from a phone in the earlier
  30-second capture.

So a working AOSP sink on 2.4 GHz should have been impossible to miss.
Three readings follow:

1. **The sink's P2P stack is on 5 GHz.** Our adapter is Band 1 only, so
   we cannot see it. Fits the observation exactly.
2. **The box's screen says "waiting for connection" but its Wi-Fi Direct
   stack never started.** Cheap Android boxes ship broken or vendor-
   mangled Wi-Fi drivers, and the screen is served by an app that does
   not necessarily depend on the P2P stack working. If this is the case,
   *no* source could ever connect to it, and that is a sink defect
   rather than a source defect.
3. **The screen is not a Wi-Fi Display sink at all.** "Screen sharing" on
   an Android box may be a vendor app, a DLNA/MediaShare app, or the
   Google Cast path — and the box's device name is what a *Cast* or
   *DLNA* screen would show too.

Reading 2 and 3 are cheap to exclude, and both change the plan, so the
next step is to fingerprint the box on the LAN rather than to keep
poking at the radio: when it leaves the screen-share screen it rejoins
the network, and then its ports and mDNS advertisements tell us exactly
which protocol it speaks.

**What we will build either way.** The provider architecture means the
answer does not change the design, only the order:

| What the box speaks | Our provider | Status |
| --- | --- | --- |
| Miracast over P2P (needs 5 GHz) | WFD P2P | built, waiting on a 5 GHz radio |
| Miracast over infrastructure | WFD MICE | not started |
| Google Cast | Cast control-plane | researched, not started |
| DLNA/UPnP | DLNA | researched, not started |
| AirPlay | AirPlay sender | researched, not started |

---

## 2026-10-20 — The sweep: the software side is now exhausted

The multi-channel capture finished. It is the most informative result of
the project so far, and it is mostly negative — which is progress.

**What the sweep found**

| | Channel 1 | Channel 6 | Channel 11 |
| --- | --- | --- | --- |
| Frames | 3,222 | 0 | 0 |
| Beacons | 185 (the `Biruk` AP) | 0 | 0 |
| WFD information elements | none | none | none |
| P2P information elements | none | none | none |
| Probe requests | none | none | none |
| AltoBeam transmitters | none | none | none |

**The sink was in Miracast mode the whole time.** It is absent from the
neighbour table, and a sink in Miracast mode leaves the access point to
form a Wi-Fi Direct group. So the TV was advertising — just not where we
could hear it.

**What that rules out**

- Our radio is healthy: the radio-work queue is empty, searches run to
  completion, and the access point's beacons are captured in the
  expected count.
- Our element is now correct under every reading of the specification
  (`0x1010`, symmetric), and discovery was re-run with it.
- Both directions were exercised: scanning for sinks, and staying
  discoverable for a sink-initiated connection. Neither produced a peer.
- The sink is not advertising a P2P information element on 2.4 GHz at
  all, which rules out a plain "it is on channel 6" explanation.

**What is left**

1. **The sink's Miracast listener is on 5 GHz.** Our adapter is Band 1
   only (`iw phy phy0 info`), and a monitor-mode capture cannot see a
   band the radio does not have. This fits every observation.
2. **The "screen share" screen is not a Wi-Fi Display sink.** Some sets
   label a vendor app or a Chromecast/AirPlay menu "screen share".
3. The sink answers only probes that carry a WFD element *and* the
   element it wants — the `arm --device-info` escape hatch exists for
   that, but we need the sink's own bytes to know what it wants.

**The product answer, whatever the cause**

AetherCast must be able to say *why* it found nothing. The states we
can already detect:

- the adapter is 2.4 GHz only, and sinks advertise on 5 GHz;
- the sink requires HDCP, which we cannot provide;
- the sink's element says the session is unavailable;
- the radio-work queue is wedged.

GNOME Network Displays shows an empty list for all of them.

---

## 2026-10-20 — The sink may be filtering us: four mechanisms, and a bug in our own element

The user asked the right question: maybe the TV only accepts a phone or a
PC, and rejects everything else. That is a real possibility, so we
enumerated the ways a sink can ignore a source.

**1. The WFD device information field.** Two bytes carrying the device
type (source, primary sink, secondary sink, dual), session availability,
the WFD protocol version, MAC address availability, information element
availability, WSD availability, the TDLS preference, HDCP support, time
synchronisation, and the audio flags.

Here we found a bug — **in our element and in GNOME Network Displays'**.
The two readings of the specification disagree about which octet holds
the low bits, and the readings disagree about what the *same bytes*
mean:

```
$ aethercastd wfd decode 000600901c4400c8      # GNOME Network Displays
  bits 0-7 in the low octet
    device type      : source
    session available: true
    TDLS preferred   : true
  bits 0-7 in the high octet
    device type      : source
    session available: false        <- a sink may ignore this
```

The element AetherCast advertised had the same problem: under the
specification's own drawing it claimed **no session availability**.

The fix is to stop guessing: advertise a **symmetric** value, `0x1010` in
both octets, so every reading sees exactly the same thing — a source,
session available, no TDLS preference, no HDCP claim. Both readings now
agree, and a unit test enforces it.

**2. The P2P primary device type.** The P2P device type attribute is
`0050F204-<category>-<subcategory>`: `1-1` computer/PC, `1-6`
computer/desktop, `10-3` phone/smartphone, `7-1` display/TV. A sink may
accept only one category — "a phone" or "a PC" — and ignore the rest.
`wpa_supplicant` takes this from its configuration file, and
**NetworkManager exposes no property for it**, so changing it means
AetherCast has to own the supplicant configuration. That is an
architectural decision for v1, not a flag.

**3. Content protection.** If the sink requires HDCP and the source
advertises none, it may refuse. We advertise no HDCP, which is honest:
we cannot offer it.

**4. The channel and the listen channel.** Covered by the capture sweep.

**Tooling built for this**

- `aethercastd wfd decode <hex>` decodes an element under **both**
  layouts, so a captured sink element immediately shows which reading is
  right — a sink must say "primary sink" and "session available", so only
  one reading can be.
- `aethercastd arm --device-info <hex>` advertises an exact device
  information field, so once we have the sink's own bytes we can copy
  them instead of reasoning about them.

---

## 2026-10-20 — Milestone 4: Wi-Fi Direct, and the wrong model of it

**What we learned by trying to form a group on this hardware**

`aethercastd p2p group` asks `wpa_supplicant` to become a Group Owner on its own. It works — the interface `p2p-wlan0-0` appears with `type P2P-GO` — and then the group is **removed again within milliseconds**, because an autonomous group with no client attached has no reason to exist. The supplicant's log says it plainly:

```
P2P-GROUP-STARTED p2p-wlan0-3 GO ssid="DIRECT-I1" freq=2412 go_dev_addr=f8:da:0c:0c:c0:11
p2p-wlan0-3: interface state ENABLED->DISABLED
AP-DISABLED
CTRL-EVENT-DISCONNECTED reason=3 locally_generated=1
nl80211: deinit ifname=p2p-wlan0-3
```

So the autonomous Group Owner is the wrong model for Miracast. **The sink initiates**: the source stays discoverable, the sink finds it, and the group forms with the sink attached — which is why the group then has a client, an address, and a reason to live. `aethercastd p2p listen` implements that: arm the WFD element, `P2P_LISTEN`, and wait for a group interface that has an IPv4 address, which is the signal that a sink has joined.

**Three things that cost time and are worth writing down**

1. **The group-start event does not reach the control socket.** It is written to the supplicant's log instead, because the P2P group interface is an internal device with no control socket of its own. The daemon therefore watches `/sys/class/net` for a new `p2p-*` interface, and reads its address with `getifaddrs`. That is unprivileged and does not depend on how the supplicant chooses to publish its events.
2. **The snapshot has to be taken before the group is created.** Taking the list of interfaces afterwards means the new one is already in it, and the group is never noticed. (Our first implementation made exactly this mistake.)
3. **The group interface has no address until a client attaches**, so "the interface exists" is not enough — the signal is "the interface exists *and* has an IPv4 address". That address is the address the sink will use to reach our RTSP server.

**Where the project stands**

| Piece | State |
| --- | --- |
| Radio health, wedge recovery, WFD element | done |
| P2P discovery, listen mode, group detection | done |
| WFD RTSP handshake M1-M7 | done, tested against a mock sink |
| Media pipeline, WFD conformance | done, all checks pass |
| **A sink that answers** | **the one missing thing** |

---

## 2026-10-20 — Milestone 3: the WFD RTSP handshake

**What we built**

`crates/aethercastd/src/rtsp.rs` implements the source side of the Wi-Fi Display RTSP profile: M1 `OPTIONS`, M2, M3 `GET_PARAMETER` with the sink's capabilities parsed, M4 `SET_PARAMETER` with our format selection, M5, M6 `SETUP` with the `Session` header, and M7.

**The protocol no longer needs a TV to be developed against.** `spike/06-mock-sink.py` plays the sink's role exactly, and `spike/07-rtsp-handshake-test.sh` checks the whole sequence:

```
PASS  the server logged '-> M1 OPTIONS'      PASS  '<- M6 SETUP'
PASS  the server logged '<- M2 OPTIONS'      PASS  '<- M7 PLAY'
PASS  the server logged '-> M3 GET_PARAMETER' PASS  the RTP port was parsed
PASS  the server logged '-> M4 SET_PARAMETER' PASS  the handshake reached the playing state
```

The second RTP port is read the way the specification defines it — a second RTP stream for a coupled secondary sink, **not** RTCP — so a sink reporting `rtp-port1=0` means "one MPEG-TS stream" rather than a reserved combination. This is the detail the code audit found GNOME Network Displays getting wrong.

**The topology, established by experiment**

| Question | Answer | How |
| --- | --- | --- |
| Is this machine on Ethernet? | **No** — it has no Ethernet interface at all, only `wlan0` | `ip -br link` |
| Where is the sink? | `192.168.1.2` and `.8`, AltoBeam MAC `dc:29:19:7f:46:5e`, **REACHABLE** | neighbour table sweep |
| Is the network 5 GHz? | **No** — the `Biruk` AP is channel 1 (2412 MHz) and there is no 5 GHz network anywhere | `nmcli dev wifi list` |
| Can the adapter see 5 GHz? | **No** — `iw phy phy0 info` shows Band 1 only | `iw phy` |
| Does the sink advertise mDNS services? | **No** — `_display._tcp`, `_googlecast._tcp`, `_airplay._tcp` are all empty, and no casting port is open | `avahi-browse -rt` |
| Did the sink leave the AP earlier? | **Yes** — the AltoBeam device disappeared entirely while the sink was in Miracast mode, which is exactly what entering Wi-Fi Direct does | neighbour table, before and after |

So the sink's Miracast listener is on the same 2.4 GHz band we can hear, which rules out the "5 GHz only" explanation. It is a Wi-Fi Direct question, not a hardware-blindness question — and the remaining test is a monitor-mode capture of the air, which is the only thing a managed interface cannot do.

**Next**
- The sink enters Miracast mode, and the monitor-mode capture (`spike/05-monitor-capture.sh`) shows whether it transmits on 2.4 GHz and what it advertises.
- P2P group formation: `P2P_GROUP_ADD`, the sink joining our group, and the RTSP server bound to the group interface.

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
