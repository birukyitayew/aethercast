# ADR 0001: Research first, Miracast source first

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Biruk Yitayew, AetherCast research team

## Context

GNOME Network Displays proves that Linux screen casting is possible, but it is fragile on real hardware: discovery can silently fail, audio initialization can crash, error messages are hard to act on, and protocol support is narrow. We want to build a better open-source alternative, not a clone.

Casting is a stack of hard, interdependent problems: Wi-Fi Direct and channel concurrency, WFD/RTSP session negotiation, HDCP boundaries, screen capture on X11 and Wayland, hardware encoding, A/V sync, and desktop UX. Committing to a language, framework, or repository layout before understanding those constraints would create expensive rework.

The user's test hardware is also a constraint: a single-radio 2.4 GHz Atheros adapter, a NetworkManager-managed station connection, and a Miracast sink that the existing app cannot discover.

## Decision

1. **Research and architecture before product code.** We will produce cited research notes, protocol diagrams, an architecture document, ADRs, a compatibility matrix, and a roadmap before implementing the application.
2. **v1 is Linux Miracast source.** The first release mirrors a Linux laptop to a Miracast sink. Audio, X11/Wayland capture, discovery, pairing, diagnostics, and packaging are in scope. Miracast sink, Google Cast, AirPlay, and DLNA come later through a provider architecture.
3. **Stack and license are evidence-driven.** We will decide between C/GTK4, Rust/GTK4, and a hybrid after the research is synthesized. The license decision will account for code reuse (GNOME Network Displays is GPL-3.0), patent risk, and ecosystem adoption.
4. **The repository is public from the start.** Research notes and decisions are documented in the open, including dead ends and unresolved questions.

## Consequences

- We delay the first line of product code, but we reduce the risk of building on a broken abstraction.
- The research notes become onboarding material and a public record of why the architecture looks the way it does.
- We must keep the documentation alive as the implementation teaches us new things; the journal and ADRs are not write-once artifacts.
- The v1 scope is deliberately narrow. Features that do not improve Miracast source reliability are deferred.

## Alternatives considered

- **Start with a UI prototype and discover the stack later.** Faster dopamine, but high risk of rebuilding the core.
- **Fork GNOME Network Displays and patch it.** Fastest path to a working demo, but inherits architectural and UX limitations and makes a clean license/stack choice harder.
- **Build a universal caster first.** Maximum scope, maximum risk, and no proven core.

## References

- [`docs/research/`](../research/) — the five research tracks
- [`JOURNAL.md`](../../JOURNAL.md) — day-by-day record
- [GNOME Network Displays](https://gitlab.gnome.org/GNOME/gnome-network-displays)
