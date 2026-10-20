# Castrel Build Journal

A running record of decisions, experiments, dead ends, and lessons. Newest entries at the top.

---

## 2026-10-20 — Day 0: Research phase begins

**Goal:** build a better open-source Linux casting app, starting with Miracast source support.

**Decisions**
- v1 scope: Linux Miracast source (laptop → TV).
- Research and architecture pack before product code.
- Public GitHub repository.
- Tech stack and license: decided after the research pack, not by preference.
- Working name: **Castrel** (provisional; trademark/name check pending).

**Environment**
- Kali Linux 2026.2, XFCE, X11.
- Qualcomm Atheros QCA9565 / AR9565 (`ath9k`), 2.4 GHz, single radio.
- Connected to `Biruk` on channel 1; P2P device `p2p-dev-wlan0`.
- TV: AltoBeam-based Miracast sink, likely `192.168.1.2` / `.8`, currently not discovered by GNOME Network Displays.
- PipeWire 1.6.6; GStreamer has `ximagesrc`, `openh264enc`, `avenc_aac`, `mpegtsmux`, `rtpmp2tpay`; missing `x264enc` and `pipewiresrc`.
- GNOME Network Displays 0.99.0 source checked out at `/tmp/opencode/gnd-src` (tag `0.99.0`).

**GitHub**
- Authenticated `gh` as `birukyitayew`.
- Public repository: [`birukyitayew/castrel`](https://github.com/birukyitayew/castrel).

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
6. The codename "AetherCast" collides with `ubports/aethercast` and Canonical's 2015 Ubuntu Miracast project. Codename only; rename before any brand.

**Decisions taken (maintainer sign-off)**
- **License:** GPL-3.0-or-later, DCO, no CLA (ADR-0003 → Accepted).
- **Architecture:** Rust control plane + GStreamer media plane, daemon + versioned D-Bus + GTK4 + CLI (ADR-0002 → Accepted).
- **Name:** the codename "AetherCast" is retired; the project is **Castrel**. Repository renamed to `birukyitayew/castrel`; the D-Bus namespace, Flathub ID, and package names follow the same change.

**Decisions recorded**
- ADR-0002 (Proposed): Rust control plane, GStreamer media plane, daemon + versioned D-Bus + GTK4 + CLI.
- ADR-0003 (Proposed): GPL-3.0-or-later, DCO, no CLA, no DRM circumvention, no proprietary codec bundling.
- ADR-0004 (Accepted): the name is a codename; clearance checklist and a registry-swept shortlist recorded.

**Next**
- Milestone 1: a headless discovery spike that sets the global WFD IE and captures a scan, to settle whether the AltoBeam TV answers WFD probes at all.
- Then a minimal WFD M1–M3 exchange to read the sink's real capability bitmap.
- Then the first frame, then the UI.
