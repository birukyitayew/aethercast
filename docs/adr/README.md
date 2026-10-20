# Architecture Decision Records

An ADR captures one decision, the context around it, and its consequences. We write one whenever a choice would be expensive to reverse or confusing to rediscover later.

## Format

Each record is a numbered Markdown file:

```text
0001-short-title.md
```

Use this structure:

```markdown
# ADR NNNN: Title

- **Status:** Proposed | Accepted | Superseded by ADR-NNNN | Rejected
- **Date:** YYYY-MM-DD
- **Deciders:** names/handles

## Context

What problem are we solving? What constraints and evidence matter?

## Decision

What did we choose? Be specific.

## Consequences

What becomes easier? What becomes harder? What do we owe future maintainers?

## Alternatives considered

What did we reject, and why?

## References

Links to research, issues, specifications, or experiments.
```

## Index

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-research-first-and-v1-scope.md) | Research first, Miracast source first | Accepted |
| [0002](0002-rust-control-gstreamer-media.md) | Rust control plane, GStreamer media plane | Proposed |
| [0003](0003-license-gpl3.md) | GPL-3.0-or-later, DCO, no CLA | Proposed |
| [0004](0004-codename-not-brand.md) | "AetherCast" is a codename, not a brand | Accepted |
