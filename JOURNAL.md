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
