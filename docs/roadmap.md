# Roadmap

**Status:** research complete; milestones 1–8 are built. The whole cast path
is proven against the reference sink, and every hardware check that remains
is listed in [`pc-test-checklist.md`](pc-test-checklist.md).

## Phase 0 — Research and architecture

- [x] Five cited research tracks
- [x] Protocol sequence diagrams
- [x] Compatibility matrix
- [x] Architecture and ADRs
- [x] Stack and license decisions
- [ ] Threat model and privacy review (the data-handling rules exist in
  ADR-0003; a full review is still owed)

## Phase 1 — MVP: Linux Miracast source

| Criterion | Status |
| --- | --- |
| Discover a Miracast sink on X11 and Wayland | Implemented; proven against the reference sink, not against a physical sink |
| Complete WFD capability negotiation and session setup | Implemented; M1–M7 proven against the reference sink |
| Stream video with acceptable latency | Implemented and measured locally; latency on a real link not measured |
| Stream audio with A/V sync | Implemented; not verified on a sink |
| Recover from a failed connection without restarting the app | `recover` implemented for the radio-work queue; session-level recovery not implemented |
| Produce an exportable diagnostic report | `doctor --json` and `GetDiagnostics` exist; a bundled report is not implemented |
| Package for at least one distribution and Flatpak | Not started |

The hardware run book is
[`pc-test-checklist.md`](pc-test-checklist.md).

## Phase 2 — Audio, UX, and packaging hardening

- Audio routing and device selection
- Quality/latency presets
- Multi-monitor selection
- Better error messages and recovery flows
- Accessibility and translations
- Stable packaging and release process

## Phase 3 — More protocols

- Google Cast
- AirPlay/AirPlay 2
- DLNA/UPnP
- Provider plugin API

## Phase 4 — Sink and advanced features

- Miracast sink mode
- Headless/kiosk mode
- Input backchannel (UIBC)
- Advanced diagnostics and compatibility reporting

## Non-goals

- DRM/HDCP circumvention
- Supporting hardware whose drivers cannot do P2P, without documenting the limitation
- Replacing GStreamer, PipeWire, wpa_supplicant, or NetworkManager
