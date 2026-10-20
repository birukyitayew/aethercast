# Documentation Index

AetherCast is in the research phase. This directory is the single source of truth for what we know, what we decided, and what we still need to prove.

## Structure

| Path | Purpose |
| --- | --- |
| `research/` | Cited deep-dives from the research orchestra |
| `protocols/` | Sequence diagrams, packet flows, and state machines |
| `adr/` | Architecture Decision Records (one decision per file) |
| `architecture.md` | System architecture, once the research is synthesized |
| `compatibility.md` | Hardware, TV, dongle, and driver compatibility matrix |
| `roadmap.md` | Milestones, acceptance criteria, and risks |

## Research tracks

1. `research/01-miracast-wfd.md` — Miracast/WFD, Wi-Fi Direct, GND/MiracleCast audit
2. `research/02-linux-wifi-p2p.md` — wpa_supplicant, NetworkManager, drivers, channels
3. `research/03-media-pipeline.md` — capture, encode, mux, audio, sync, latency
4. `research/04-other-protocols.md` — Google Cast, AirPlay, DLNA, prior art
5. `research/05-product-architecture.md` — UX, architecture, packaging, security, license, CI

Start with the [research index](research/README.md) for the six findings that decide the project.

## Decisions

- [ADR-0001](adr/0001-research-first-and-v1-scope.md) — research first, Miracast source first (Accepted)
- [ADR-0002](adr/0002-rust-control-gstreamer-media.md) — Rust control plane, GStreamer media plane (Proposed)
- [ADR-0003](adr/0003-license-gpl3.md) — GPL-3.0-or-later, DCO, no CLA (Proposed)
- [ADR-0004](adr/0004-name-aethercast.md) — the project is named AetherCast; the name collision is a known risk (Accepted)

## Research standards

- Every factual claim needs a source link or a reproducible local command.
- Separate **verified**, **reported**, and **speculative** claims.
- Include a "Bottom line for AetherCast" section with concrete recommendations.
- Record open questions and risks, not just conclusions.
- Prefer primary sources: specifications, source code, kernel/wpa_supplicant docs, and issue trackers.
