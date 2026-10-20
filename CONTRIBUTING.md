# Contributing to AetherCast

AetherCast is in the research phase. The most valuable contributions right now are **evidence, protocol notes, hardware reports, and design critique** — not product code.

## Before you start

- Read [`docs/README.md`](docs/README.md) and the research notes relevant to your idea.
- Search existing issues and discussions.
- For a large change, open an issue or discussion first so we can agree on the approach.
- Be precise about hardware: distribution, kernel, desktop/session, Wi-Fi adapter, driver, and sink model.

## Research contributions

Research is a first-class contribution.

- Cite primary sources (specifications, source code, kernel docs, issue trackers).
- Separate **verified**, **reported**, and **speculative** claims.
- Include reproducible commands and sanitized logs where possible.
- Record negative results. "This did not work on hardware X" is valuable.
- Do not upload raw packet captures containing private network data unless they are sanitized and necessary.

## Code contributions

Product code is intentionally paused until the architecture pack is accepted. Once it opens:

- Keep changes focused and reviewable.
- Match the surrounding style and module boundaries.
- Add tests for protocol parsing, state machines, and error paths.
- Update the journal or an ADR when a decision changes.
- Never add code that circumvents HDCP, FairPlay, Widevine, or other content protection.

## Documentation style

- Markdown, one sentence per line is welcome but not required.
- Use relative links inside the repository.
- Put protocol diagrams in `docs/protocols/` and decisions in `docs/adr/`.
- Date hardware and software claims, because drivers and distributions change.

## Commit messages

Use the imperative mood and a short scope when useful:

```text
research: document WFD M3 capability negotiation
docs: add single-radio compatibility notes
fix: handle RTSP timeout during M4
```

## Licensing

AetherCast is licensed **GPL-3.0-or-later** (see [ADR-0003](docs/adr/0003-license-gpl3.md)).

- **Sign off every commit** with the Developer Certificate of Origin: use `git commit -s` or `git commit --signoff`. A CI check enforces it on pull requests.
- We do **not** use a CLA. The DCO gives us the copyright and licensing grant we need, without the friction that stops contributions.
- Do not contribute code copied from projects with incompatible licenses. GPL prior art may be **studied** for protocol understanding, but not copied into files we ship. `openairplay/airplay2-receiver` ships no LICENSE file at all and must never be copied under any circumstances.
- We do not add code that circumvents HDCP, FairPlay, Widevine, or any other content protection.

## Code of conduct

Participation is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Be kind, assume good faith, and critique ideas rather than people.
