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

## Research standards

- Every factual claim needs a source link or a reproducible local command.
- Separate **verified**, **reported**, and **speculative** claims.
- Include a "Bottom line for AetherCast" section with concrete recommendations.
- Record open questions and risks, not just conclusions.
- Prefer primary sources: specifications, source code, kernel/wpa_supplicant docs, and issue trackers.
