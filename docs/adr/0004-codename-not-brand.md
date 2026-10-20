# ADR 0004: "AetherCast" retired as a codename; the project is Castrel

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Biruk Yitayew, AetherCast research team

## Context

The project started under the working name **AetherCast**. The name-clearance research found exact prior art in the same technical niche:

| Prior art | Evidence | Collision |
| --- | --- | --- |
| `ubports/aethercast` | Archived 2022-11-24, LGPL-3.0, 604 commits — "a generic abstraction layer … common DBus API" for Miracast-like technologies ([repo](https://github.com/ubports/aethercast)) | Same name, same domain, same API concept |
| Canonical "Aethercast" | Miracast for Ubuntu, Launchpad project, 25 Dec 2015 ([Phoronix](https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi)) | Same name and technology, prior use in this ecosystem |
| Our own repository | formerly `birukyitayew/aethercast` | Public at the time of writing |

A name that collides in the same domain is a legal and adoption liability: confusing to users, hard to clear for trademark, and unusable for package names, D-Bus namespaces, and app IDs.

## Decision

**The codename "AetherCast" is retired. The project is named Castrel.**

Every downstream identifier changes **together**, never piecemeal:

| Identifier | Value | Status |
| --- | --- | --- |
| Repository | `birukyitayew/castrel` | Done (GitHub redirects the old URL) |
| Display name | Castrel | Done |
| D-Bus namespace | `org.castrel.Castrel` | Provisional — rebased on the owned domain |
| Flathub ID | `org.castrel.Castrel` | Provisional |
| Daemon binary | `castreld` | Provisional |
| Config directory | `~/.config/castrel` | Provisional |
| Domain | *pending* | Must be registered before the first public release |

Renaming now is cheap — no users, no releases, no product code depends on the name. Renaming after a Flathub submission would not be: a published app ID, a versioned D-Bus API, and package names in five distributions are effectively permanent.

### Clearance checklist still outstanding for Castrel

The rename to Castrel was made on a **registry sweep**, not a trademark search. Before any public brand, Flathub submission, or domain registration, the following must pass:

1. Trademark search — registered and common-law — in the US, EU, and UK.
2. Code-host, package registry, and app-ID search (GitHub, GitLab, npm, PyPI, crates.io, Flathub, Docker Hub).
3. Domain, social handle, and app-store availability; spelling and read-aloud test.
4. Brand-usage review: avoid "Miracast" and "Wi-Fi CERTIFIED" in the name and marketing, since both are licensed marks with usage rules ([Wi-Fi Alliance brands](https://www.wifi.org/our-brands)).
5. Similar-product search in adjacent categories (broadcast, streaming, lighting, presentation software).

### Candidates swept on 2026-10-20

Registry sweep (GitHub repo search, npm, PyPI):

| Candidate | GitHub | npm | PyPI | Outcome |
| --- | --- | --- | --- | --- |
| `castrel` | none | free | free | **Selected** |
| `beamcast` | none | free | free | Runner-up |
| `lumenplay` | none | free | free | Runner-up |
| `signalcast` | none | free | free | Runner-up |
| `castwave` | none | free | free | Runner-up |
| `mirrorling` | none | free | free | Rejected — hard to say and spell |
| `lumenbeam` | none | free | free | Rejected — Lumenpulse sells a "Lumenbeam" lighting product family |
| `waveline` | none | taken | taken | Rejected — package conflicts |
| `screenbeam` | none | taken | free | Rejected — package conflicts |
| `prismatic` | none | taken | taken | Rejected — package conflicts, and generic |

**None of these are trademark-cleared.** The sweep only proves absence in code and package registries.

## Consequences

- The repository, D-Bus namespace, Flathub ID, binary names, and config directory are all decided from one name, so they are chosen once.
- The research notes keep the historical codename in their body text and carry a naming banner, because note 05 §4.2 is the evidence for this decision. Rewriting them would destroy the reasoning.
- The project identity is "Castrel" in all future material: README, journal, releases, and package metadata.
- If the trademark search fails, we rename again — which is why the checklist runs before, not after, the first release.

## Alternatives considered

- **Keep "AetherCast" and rely on it being a new coinage.** Rejected: it is not new in this niche, and the collision is on name, domain, and API concept simultaneously.
- **Rename after the first release.** Rejected: a published Flathub ID, a versioned D-Bus API, and package names are the most expensive things to change.
- **Use a descriptive name** ("Linux Screen Mirror"). Rejected: it cannot be trademarked, cannot build a brand, and reads like a utility.

## References

- [`docs/research/05-product-architecture.md`](../research/05-product-architecture.md) §4.2
- <https://github.com/ubports/aethercast>
- <https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi>
