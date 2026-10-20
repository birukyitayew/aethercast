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
