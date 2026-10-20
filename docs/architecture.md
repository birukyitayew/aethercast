# Architecture

**Status: Research phase.** This document will be synthesized from the five research tracks before product code begins.

## What belongs here

- The system decomposition: discovery, session negotiation, media pipeline, audio, UI, CLI, and privileged helpers
- The provider interface for Miracast, Google Cast, AirPlay, and DLNA
- The state machine for a casting session
- Error taxonomy and recovery behavior
- Process, IPC, and permission boundaries
- Packaging and sandbox constraints

## Research inputs

- [`research/01-miracast-wfd.md`](research/01-miracast-wfd.md)
- [`research/02-linux-wifi-p2p.md`](research/02-linux-wifi-p2p.md)
- [`research/03-media-pipeline.md`](research/03-media-pipeline.md)
- [`research/04-other-protocols.md`](research/04-other-protocols.md)
- [`research/05-product-architecture.md`](research/05-product-architecture.md)

The architecture decision records in [`adr/`](adr/) capture the choices that shaped this document.
