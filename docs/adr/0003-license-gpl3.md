# ADR 0003: GPL-3.0-or-later, DCO, no CLA

- **Status:** Accepted (2026-10-20, maintainer sign-off after research synthesis)
- **Date:** 2026-10-20
- **Deciders:** Biruk Yitayew, AetherCast research team

## Context

The license decision was deferred until the research was done, because it depends on three facts we did not have at the start: what code we intend to reuse, what patent risk the protocols carry, and which prior art is even reusable.

The research established:

1. **The reference implementation is GPL-3.0-or-later.** GNOME Network Displays is GPL-3.0-or-later ([header](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-sink.h)). Any direct reuse of its code forces the same license.
2. **The AirPlay prior art is unusable under a permissive license.** UxPlay, RPiPlay, and gmrender-resurrect are GPL, and `openairplay/airplay2-receiver` ships **no LICENSE file at all**, which means default copyright and no reuse rights ([04](../research/04-other-protocols.md) §12).
3. **The codec path is license-sensitive.** x264 is GPL. OpenH264's royalty cover applies to Cisco-distributed binaries, not self-built ones ([01](../research/01-miracast-wfd.md) §13). A permissive project can still ship hardware encoding, but a software-encode fallback drags GPL in ([03](../research/03-media-pipeline.md) §9).
4. **The protocol space carries patent risk.** Miracast/Wi-Fi Display and HDCP are licensed programs; HDCP requires a DCP license for a "Licensed Product", and the royalty rate is not published — the note explicitly marks it *unknown, not established* ([01](../research/01-miracast-wfd.md) §13, [05](../research/05-product-architecture.md) §7.1).

## Decision

**AetherCast is licensed GPL-3.0-or-later.** Contributions are made under a **Developer Certificate of Origin**. We do **not** use a CLA.

Concretely:

- SPDX headers on every source file, plus REUSE compliance for non-code assets.
- DCO sign-off in every commit (`git config commit.gpgsign` is optional; the sign-off line is not).
- No CLA: the DCO gives us the copyright transfer-and-license right we need, without the legal friction that stalls contributions.
- **No DRM circumvention, ever.** If a sink requires HDCP-protected content, we detect it and refuse cleanly and visibly.
- **No Wi-Fi CERTIFIED claims.** "Miracast" and "Wi-Fi CERTIFIED" carry brand usage rules; we are not a Wi-Fi Alliance member.
- **No proprietary codec bundling.** Ship only codecs with clear redistribution terms, and document the patent position per codec.

## Consequences

- We may reuse GNOME Network Displays code, and we may ship x264 as the software-encode fallback, which is required for 720p30 on machines without hardware encoding ([03](../research/03-media-pipeline.md) §4).
- GPL-3.0 §11 gives an express patent grant from contributors, which is a meaningful mitigation for the protocol patent question — not a complete answer, and not legal advice.
- Commercial adoption is harder than under MIT/Apache. For a desktop app aimed at users and distro packagers, that is an acceptable trade; for embedded OEM products it would need a commercial licence, which is a different business model and should not block v1.
- A future relicensing to a permissive license is possible in principle (GPL-3.0 §5(b) allows it if all contributors agree) but is impractical in practice. Treat the license as permanent.

## Alternatives considered

- **MIT or Apache-2.0.** Attractive for corporate adoption and for reusing BSD/Apache prior art for Cast and DLNA. Rejected because it forfeits GND reuse, complicates the x264 fallback, and does not change the patent position materially.
- **LGPL-3.0.** Would permit dynamic linking against GPL prior art, but the reuse we actually want is at the source level in the same project, where LGPL buys nothing.
- **A CLA.** Rejected as unnecessary friction; the DCO covers the copyright and licensing grant.

## References

- [`docs/research/05-product-architecture.md`](../research/05-product-architecture.md) §8
- [`docs/research/04-other-protocols.md`](../research/04-other-protocols.md) §12
- [`docs/research/01-miracast-wfd.md`](../research/01-miracast-wfd.md) §13
- [GPL-3.0 text](https://www.gnu.org/licenses/gpl-3.0.html)
