# Research Notes

Five parallel research tracks, completed 2026-10-20 by the research orchestra. Roughly 30,000 words with 393 cited URLs. Every claim is labelled **verified** (read in a primary source or reproduced locally), **reported** (third-party statement), or **speculative** (inference).

| Note | Track | Words |
| --- | --- | --- |
| [01 — Miracast / WFD](01-miracast-wfd.md) | Protocol deep dive, RTSP M1–M16, WFD IEs, codecs, HDCP boundary, GND 0.99.0 code audit, MiracleCast and peers | 5,999 |
| [02 — Linux Wi-Fi P2P](02-linux-wifi-p2p.md) | wpa_supplicant, NetworkManager P2P, nl80211, driver matrix, channel concurrency, permissions, test harness | 5,981 |
| [03 — Media pipeline](03-media-pipeline.md) | Capture, encode, mux, transport, audio, A/V sync, latency, GStreamer vs custom | 5,958 |
| [04 — Other protocols](04-other-protocols.md) | Google Cast/CASTV2, AirPlay/AirPlay 2, DLNA/UPnP, 21 prior-art projects, provider abstraction | 6,334 |
| [05 — Product & architecture](05-product-architecture.md) | Users, competitor teardown, architecture options, packaging, security, licensing, name clearance, roadmap | 5,999 |

## Read order

1. [05](05-product-architecture.md) — the strategic frame, the competitive gap, and the decisions.
2. [01](01-miracast-wfd.md) — the protocol we must implement correctly.
3. [02](02-linux-wifi-p2p.md) — the radio that decides whether anything works.
4. [03](03-media-pipeline.md) — the media path and its cost.
5. [04](04-other-protocols.md) — what comes after v1.

## The six findings that decide the project

1. **Discovery fails before RTSP ever starts.** The machine has no global WFD IE (`wpa_cli get WFDIEs` → `FAIL`), and NetworkManager only applies `wfd-ies` at connection activation, never while scanning. Miracast sinks may not answer probes that lack the WFD IE. This is the prime suspect for "discovers no sinks". — [01](01-miracast-wfd.md) §2, [02](02-linux-wifi-p2p.md) §5
2. **The radio, not the software, is the binding constraint.** `ath9k` on the QCA9565/AR9565 is `#channels <= 1`, so a P2P group on a different channel than the associated station fails with `-2` (channel unavailability). Discovery off-channel still works via ROC. — [02](02-linux-wifi-p2p.md) §4–5
3. **NetworkManager cannot express what P2P needs.** No GO intent, no operating frequency, no peer channel, `join` hard-coded false. A narrow helper that can reach wpa_supplicant is required. — [02](02-linux-wifi-p2p.md) §6
4. **Software encode is a 720p30-only path, and only with x264.** Measured on this box: OpenH264 0.32–0.81× realtime at 720p30, x264 2.64×. 1080p30 requires hardware. `vah264enc` is missing from stock GStreamer 1.28.3 here. — [03](03-media-pipeline.md) §4
5. **GND's negotiation core has four high-severity defects** — no global WFD IE during discovery, the second RTP port misread as RTCP, resolution selection compiled out behind `#if 0` (always 1080p30), and a 512 kbit/s bitrate clamp. Forking it inherits them. — [01](01-miracast-wfd.md) §5
6. **The codename collided.** "AetherCast" was the same name as `ubports/aethercast` (archived 2022) — same domain, same API concept — and Canonical ran an "Aethercast" Miracast project in 2015. The project was renamed to **Castrel** on 2026-10-20. — [05](05-product-architecture.md) §4.2, [ADR-0004](../adr/0004-codename-not-brand.md)

## What the research recommends

- **GStreamer for media, Rust for control.** Every media element the protocol needs is upstream-maintained; the WFD state machine, LPCM packaging, metrics, and supervision are ours.
- **Daemon + D-Bus + GTK4 + CLI.** Matches GND's process split, enables headless use, isolates media crashes, makes the core testable.
- **Build the diagnosis layer first.** A typed state machine, explainable discovery, and a redacted diagnostics bundle are what the GND issue tracker actually demands.
- **License GPL-3.0-or-later.** It permits GND reuse, carries an express patent grant, and fits the ecosystem. DCO, not CLA. SPDX/REUSE.
- **Ship Flatpak first**, distro packages second.
- **Gate on the radio.** Prove discovery and a first frame on the QCA9565/AltoBeam combination before investing in UI.

Full verdicts live in each note's "Bottom line for Castrel" section.
