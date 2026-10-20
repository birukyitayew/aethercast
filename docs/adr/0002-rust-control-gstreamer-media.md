# ADR 0002: Rust control plane, GStreamer media plane

- **Status:** Accepted (2026-10-20, maintainer sign-off after research synthesis)
- **Date:** 2026-10-20
- **Deciders:** Biruk Yitayew, AetherCast research team

## Context

AetherCast needs to implement the WFD state machine, negotiate with untrusted sinks, drive a media pipeline, expose a stable API to a GUI and a CLI, and run headless on kiosk hardware.

The research established two facts that shape this choice:

- The media path is not the differentiator. Every element the protocol needs is upstream-maintained and maintained by GStreamer: `ximagesrc`, `pipewiresrc` (portal), `vaapih264enc`/`nvh264enc`/`x264enc`/`openh264enc`, `avenc_aac`, `mpegtsmux`, `rtpmp2tpay`, `rtspclientsink`. Rewriting an encoder, a muxer, or a RTSP stack would be months of work with no user-visible gain.
- The control path is the differentiator. The WFD state machine, the capability contract, the error taxonomy, the channel-budget logic, and the diagnostics are exactly where GNOME Network Displays fails — and its tracker shows that discovery, P2P failure, freezing, and latency are the complaints people actually file ([05](05-product-architecture.md) §3.1).

A monolithic C/GTK4 application inherits GND's coarse error model and makes headless and cross-desktop use awkward. A pure-Rust media stack means building from scratch what GStreamer already maintains.

## Decision

**AetherCast is a Rust control plane around a GStreamer media plane, exposed as a daemon with a versioned D-Bus API, a GTK4/libadwaita client, and a first-class CLI.**

Concretely:

| Component | Choice |
| --- | --- |
| Core / session daemon | Rust (`aethercastd`), single owner of the session state machine |
| Media pipeline | GStreamer, in a per-session child process so a media crash cannot take down the daemon |
| Public API | Versioned D-Bus interface, method-per-operation, typed errors, stable bus name |
| GUI | GTK4 + libadwaita, thin client over D-Bus |
| CLI | First-class, same daemon, headless-friendly, scriptable |
| P2P control | NetworkManager for lifecycle and discovery, plus one narrowly scoped privileged helper for GO intent and operating frequency that NM cannot express ([02](02-linux-wifi-p2p.md) §6) |
| Capture | One `CaptureSource` interface, two backends: X11 (`ximagesrc`) first, ScreenCast portal for Wayland ([03](03-media-pipeline.md) §3) |
| Packaging | Flatpak first, distro packages second |

The daemon/CLI split is not optional: GND exposes D-Bus but ships no CLI binary, which is why "IT admin / developer" and "headless/kiosk integrator" are underserved personas ([05](05-product-architecture.md) §2.1).

## Consequences

- The protocol logic is testable without a GUI, a display, or a TV.
- Media crashes are isolated; the daemon can restart a pipeline without losing the app.
- Rust's type system and the absence of manual memory management reduce the risk of a parser bug in code that parses untrusted sink input.
- We take on a second language and a GStreamer packaging surface. The research shows this is the cheaper of the two burdens: GStreamer packaging gaps are known and bounded ([03](03-media-pipeline.md) §7), while re-implementing the media path is not.
- The daemon needs a real privilege model for the P2P helper. That is ADR-worthy on its own and will be recorded separately.

## Alternatives considered

- **C + GTK4 monolith, forked from GND.** Fastest route to a demo and reuses a working RTSP implementation, but it inherits GND's four high-severity negotiation defects ([01](01-miracast-wfd.md) §5), its coarse error model, and its 500 ms hard-coded latency ([03](03-media-pipeline.md) §7), and it cannot reuse GPL code under a permissive license.
- **Pure Rust, no GStreamer.** Rejected: the media path is exactly the part the ecosystem already solves well.
- **Python for the core.** Rejected: packaging, performance, and long-term maintenance for a daemon that must be robust in a kiosk setting.

## References

- [`docs/research/03-media-pipeline.md`](../research/03-media-pipeline.md) §7, §9
- [`docs/research/05-product-architecture.md`](../research/05-product-architecture.md) §5
- [`docs/research/01-miracast-wfd.md`](../research/01-miracast-wfd.md) §5
