# ADR 0004: "AetherCast" is a codename, not a brand

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Biruk Yitayew, AetherCast research team

## Context

The project started under the working name **AetherCast**. The name-clearance research found exact prior art in the same technical niche:

| Prior art | Evidence | Collision |
| --- | --- | --- |
| `ubports/aethercast` | Archived 2022-11-24, LGPL-3.0, 604 commits — "a generic abstraction layer … common DBus API" for Miracast-like technologies ([repo](https://github.com/ubports/aethercast)) | Same name, same domain, same API concept |
| Canonical "Aethercast" | Miracast for Ubuntu, Launchpad project, 25 Dec 2015 ([Phoronix](https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi)) | Same name and technology, prior use in this ecosystem |
| Our own repository | [`birukyitayew/aethercast`](https://github.com/birukyitayew/aethercast) | Already public |

A name that collides in the same domain is a legal and adoption liability: it is confusing to users, hard to clear for trademark, and impossible to use cleanly for package names, D-Bus namespaces, and app IDs.

## Decision

**"AetherCast" is a codename only. The project must be renamed before it takes on a public brand, a Flathub submission, a domain, or a package namespace.**

- The repository, D-Bus namespace, Flathub ID, package names, and website are renamed **together**, never piecemeal.
- Renaming now is cheap: GitHub redirects old URLs, the project has no users and no releases, and no product code depends on the name.
- Renaming later is expensive: a Flathub ID, a published D-Bus API, package names in five distros, and a domain are all effectively permanent.
- The repository is not renamed as part of this ADR; the final name is an open question until the clearance checklist passes.

### Clearance checklist for a candidate name

1. Trademark search — registered and common-law — in the US, EU, and UK.
2. Code-host, package registry, and app-ID search (GitHub, GitLab, npm, PyPI, crates.io, Flathub).
3. Domain, social handle, and app-store availability; spelling and read-aloud test.
4. Brand-usage review: avoid "Miracast" and "Wi-Fi CERTIFIED" in the name and marketing, since both are licensed marks with usage rules.
5. Search for unrelated but confusingly similar products in the same category (lighting, broadcast, streaming).

### Candidates checked so far

Registry sweep on 2026-10-20 (GitHub repo search, npm, PyPI):

| Candidate | GitHub | npm | PyPI | Note |
| --- | --- | --- | --- | --- |
| `lumenbeam` | none | free | free | **Rejected** — Lumenpulse sells a "Lumenbeam" architectural lighting product family; confusing in the same "beam/light" metaphor |
| `castrel` | none | free | free | Strong candidate: short, invented, pronounceable, no product collision found yet |
| `mirrorling` | none | free | free | Unusual, harder to say and to spell |
| `waveline` | none | taken | taken | Package conflicts |
| `beamcast` | none | free | free | Viable; less distinctive than `castrel` |
| `lumenplay` | none | free | free | Viable; reads as a media player rather than a caster |
| `signalcast` | none | free | free | Viable; slightly generic |
| `castwave` | none | free | free | Viable |
| `screenbeam` | none | taken | free | Package conflict |
| `prismatic` | none | taken | taken | Package conflicts, and generic |

Trademark and common-law clearance is still outstanding for every candidate. **Do not treat any of these as cleared.**

## Consequences

- The repository keeps the codename until the checklist passes, but must not be published under a brand, packaged, or promoted under it.
- Every downstream name (D-Bus namespace, Flathub ID, binary names, config directory) is decided after the name is fixed, so they are chosen once.
- The research is unaffected: the notes, the code audit, and the roadmap do not depend on the name.

## Alternatives considered

- **Keep the name and rely on "AetherCast" being a new coinage.** Rejected: the coinage is not new in this niche.
- **Rename after the first release.** Rejected: a published Flathub ID, D-Bus API, and package names are the most expensive things to change.
- **Use a descriptive name** ("Linux Screen Mirror"). Rejected: it cannot be trademarked, cannot build a brand, and reads like a utility.

## References

- [`docs/research/05-product-architecture.md`](../research/05-product-architecture.md) §4.2
- <https://github.com/ubports/aethercast>
- <https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi>
