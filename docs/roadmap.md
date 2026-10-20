# Roadmap

**Status: Research phase.** The roadmap will be refined after the research synthesis and architecture review.

## Phase 0 — Research and architecture

- Five cited research tracks
- Protocol sequence diagrams
- Compatibility matrix
- Architecture and ADRs
- Stack and license decisions
- Threat model and privacy review

## Phase 1 — MVP: Linux Miracast source

Acceptance criteria:

- Discover a Miracast sink on X11 and Wayland
- Complete WFD capability negotiation and session setup
- Stream video with acceptable latency on the test hardware
- Stream audio with A/V sync
- Recover from a failed connection without restarting the app
- Produce an exportable diagnostic report
- Package for at least one distribution and Flatpak

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
