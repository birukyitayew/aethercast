# ADR 0004: The project is named AetherCast, and the name collision is a known risk

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Biruk Yitayew, AetherCast research team

## Context

The project started under the working name **AetherCast**. The name-clearance research found prior art in the same technical niche:

| Prior art | Evidence | Collision |
| --- | --- | --- |
| `ubports/aethercast` | Archived 2022-11-24, LGPL-3.0, 604 commits — "a generic abstraction layer … common DBus API" for Miracast-like technologies ([repo](https://github.com/ubports/aethercast)) | Same name, same domain, same API concept |
| Canonical "Aethercast" | Miracast for Ubuntu, Launchpad project, 25 Dec 2015 ([Phoronix](https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi)) | Same name and technology, prior use in this ecosystem |

A registry sweep (GitHub repo search, npm, PyPI) on 2026-10-20 found **no** live conflict in code or package registries — `ubports/aethercast` is archived, and no `aethercast` package is published on npm or PyPI.

The collision is therefore real but **historic and dormant**, not active. The research proposed alternatives (`castrel`, `beamcast`, `lumenplay`, `signalcast`, `castwave`, `mirrorling`; `lumenbeam` rejected — Lumenpulse lighting products; `waveline`, `screenbeam`, `prismatic` rejected on package conflicts) and the project was briefly renamed to `castrel` while that was being reviewed.

The maintainer's decision is to **keep `AetherCast`**, on the grounds that:

- the prior art is archived and inactive, not a live product in the same market,
- the name is short, pronounceable, and already used consistently across 30,000 words of research, the journal, the code audit, and the existing scripts — the daemon name `aethercastd` and the D-Bus namespace follow from it,
- renaming a public repository, and every identifier derived from it, costs more than the residual ambiguity,
- the risk is a trademark/confusion risk, not a functional one, and it is fully mitigable by a clearance pass before any public release.

## Decision

**The project is named AetherCast.** The repository is `birukyitayew/aethercast`.

The collision is recorded as an **accepted risk** with an explicit rename trigger, not ignored.

### Identifiers

| Identifier | Value | Status |
| --- | --- | --- |
| Repository | `birukyitayew/aethercast` | Final |
| Display name | AetherCast | Final |
| Daemon binary | `aethercastd` | Final |
| D-Bus namespace | `io.github.birukyitayew.aethercast` | Provisional — rebased on the owned domain when one is registered |
| Client application ID | `io.github.birukyitayew.aethercast.Client` | Provisional — same |
| Flathub ID | `io.github.birukyitayew.aethercast` | Provisional — same |
| Config directory | `~/.config/aethercast` | Final |
| Domain | *pending* | Must be registered before the first public release |

### Why the client has its own application ID

The daemon owns the bus name `io.github.birukyitayew.aethercast` and
serves the API at the object path `/io/github/birukyitayew/aethercast`.
A GApplication derives its own object path from its application ID, so a
client whose ID was the same string would try to export at the very path
the daemon already owns. Two things cannot own one name, and the client
fails to start with a registration error that says nothing about the
cause.

The client therefore uses `io.github.birukyitayew.aethercast.Client`, and
the daemon keeps the plain name — the name a client library looks up.

### Accepted risk and mitigation

1. **We do not claim to be the original "AetherCast".** The README, the About text, and the project description say "AetherCast is an independent project, not affiliated with ubports or Canonical".
2. **No trademark use.** The name is used nominatively, as a project name. We make no claim over the name and will not register it as a trademark.
3. **Clearance gate before the first release.** Before the first Flathub submission, domain registration, or any public announcement, a clearance pass must run:
   - trademark search — registered and common-law — in the US, EU, and UK,
   - code-host, package registry, and app-ID search (GitHub, GitLab, npm, PyPI, crates.io, Flathub, Docker Hub),
   - domain, social handle, and app-store availability; spelling and read-aloud test,
   - brand-usage review: avoid "Miracast" and "Wi-Fi CERTIFIED" in marketing, since both are licensed marks with usage rules,
   - similar-product search in adjacent categories (broadcast, streaming, presentation software).
4. **Rename triggers.** We rename if any of these occur:
   - the clearance pass finds a live trademark in the casting/display category,
   - a user or contributor is confused by the collision in a way that damages the project,
   - the domain cannot be registered cleanly, or
   - the project is commercialised.

### Rename mechanics, if triggered

All downstream identifiers change **together, never piecemeal**: repository, D-Bus namespace, Flathub ID, binary names, package names, config directory, and website. GitHub redirects the old repository URL, so links do not break. A rename before the first release is cheap; after a Flathub ID and a versioned D-Bus API are published, it is not.

## Consequences

- The repository URL is `github.com/birukyitayew/aethercast`, matching the research notes, the journal, and the spike scripts. The research notes keep a naming banner pointing here.
- The research notes and ADR-0001/0002/0003 no longer need a rename caveat; only ADR-0004 and the notes' banner mention the collision.
- The project carries a known, documented, non-trivial naming risk. We accept it deliberately, with a gate and a trigger, rather than leaving it to chance.
- Commercialisation would require a fresh name; that is an acceptable future problem, and the domain and Flathub ID stay provisional until then.

## Alternatives considered

- **Rename to `castrel` or another cleared candidate.** Rejected by the maintainer: the collision is historic and dormant, and the churn of renaming the repository and every derived identifier is not justified by it.
- **Ignore the collision entirely.** Rejected: an unrecorded risk is an unmanaged risk. This ADR exists so the risk, its mitigation, and its trigger are on the record.
- **Use a descriptive name** ("Linux Screen Mirror"). Rejected: it cannot be trademarked, cannot build a brand, and reads like a utility.

## References

- [`docs/research/05-product-architecture.md`](../research/05-product-architecture.md) §4.2
- <https://github.com/ubports/aethercast>
- <https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi>
- GitHub issue [#7 — Name clearance gate](../..) on the repository issue tracker
