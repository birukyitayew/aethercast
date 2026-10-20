# AetherCast

> A best-in-class open-source casting app for Linux — starting with Miracast.

**Status: Research phase.** No product code yet. We are documenting the protocols, the Linux stack, the hardware limits, and the UX before committing to an architecture.

## Why

GNOME Network Displays proved that Linux screen casting is possible, but it is still fragile: discovery depends on the Wi-Fi radio and desktop portal, error messages are cryptic, audio setup can crash, and protocol support is narrow. AetherCast aims to be the reliable, transparent, cross-desktop alternative:

- **Discovery that explains itself** — show why a TV is not visible instead of an empty list.
- **One-click casting** — sane defaults, quality/latency presets, and fast recovery.
- **Protocol-honest** — Miracast first, then Google Cast, AirPlay, and DLNA through a modular provider architecture.
- **Works on real hardware** — including single-radio laptops, X11 and Wayland, and common Wi-Fi chipsets.
- **Diagnostics built in** — exportable logs, compatibility reports, and a CLI/headless mode.
- **Open source, upstream-first** — reuse and contribute to GStreamer, wpa_supplicant, NetworkManager, and PipeWire rather than forking the world.

## Scope

**v1:** Linux Miracast source (laptop → TV), X11 and Wayland, video + audio, discovery, pairing, and a usable GTK4 interface.

**Later:** Miracast sink, Google Cast, AirPlay/AirPlay 2, DLNA/UPnP, headless/kiosk mode, and a plugin API.

## Principles

1. **Evidence before code.** Every protocol claim and design decision is documented and cited.
2. **No DRM circumvention.** HDCP and protected-content limits are documented, not broken.
3. **The radio is the hard part.** Wi-Fi Direct, channel concurrency, and driver support drive the architecture.
4. **Fail visibly.** A user should always know what failed, why, and what to try next.
5. **Accessible by default.** Keyboard navigation, screen-reader labels, high contrast, and translated strings.

## Status

**Research phase complete. Milestone 1 in code.**

- Five cited research notes, ~30,000 words, 393 sources — [`docs/research/`](docs/research/)
- Four ADRs: research-first scope, Rust + GStreamer architecture, GPL-3.0-or-later, the name decision — [`docs/adr/`](docs/adr/)
- A reproducible root cause for "GNOME Network Displays finds no sinks" — [`spike/01-radio-work-wedge-report.md`](spike/01-radio-work-wedge-report.md)
- `aethercastd`, the session daemon: radio health, the global WFD information element, and Wi-Fi Direct discovery — [`crates/aethercastd/`](crates/aethercastd/)

```console
$ cargo run -p aethercastd -- doctor
interface        : wlan0
station          : Biruk (2c:b6:c2:ed:c7:d3)  [connected]
channel          : 2412 MHz
radio-work queue : 0 item(s)
radio health     : ok
wfd element      : armed
  - id=0 body=00101c4400c8
  device type     : source  session available: true  TDLS preferred: false
```

Not yet implemented: P2P group formation, the WFD RTSP handshake, the media pipeline, and the GUI.

## Documentation

- [`docs/README.md`](docs/README.md) — documentation index
- [`docs/research/`](docs/research/) — cited protocol and platform research
- [`docs/adr/`](docs/adr/) — architecture decision records
- [`docs/protocols/`](docs/protocols/) — sequence diagrams and packet flows
- [`JOURNAL.md`](JOURNAL.md) — the day-by-day build journal

## License

To be decided after the licensing and code-reuse research. If we reuse GPL-3.0 code from GNOME Network Displays, the project must be GPL-3.0.

## Not affiliated with GNOME

AetherCast is an independent project. GNOME Network Displays is a separate project and a source of inspiration and prior art.
