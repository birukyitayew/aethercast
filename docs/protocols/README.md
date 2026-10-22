# Protocol Notes

This directory holds the protocol-level documentation that the implementation will be built against: sequence diagrams, packet flows, state machines, capability matrices, and capture notes.

## Planned notes

| Note | Protocol | Status |
| --- | --- | --- |
| [`wfd-sink-observed.md`](wfd-sink-observed.md) | One real WFD sink, observed: what it speaks and why it cannot be found | Observed 2026-10-21 |
| `wfd-rtsp.md` | Miracast/WFD RTSP M1–M7 | Research |
| `wfd-media.md` | WFD RTP/MPEG-TS, codecs, A/V sync | Research |
| `wfd-p2p.md` | Wi-Fi Direct discovery and group formation | Research |
| `wfd-mice.md` | Miracast over infrastructure | Research |
| `castv2.md` | Google Cast discovery and CASTV2 | Deferred |
| `airplay.md` | AirPlay/AirPlay 2 | Deferred |
| `dlna.md` | DLNA/UPnP | Deferred |

## Standards

- Every diagram should identify the direction of messages and the transport (UDP/TCP, port).
- Capture files belong in a local `captures/` directory and must not be committed unless they are sanitized and small.
- Record the device, driver, and software versions used for any capture.
- Mark unverified behavior clearly.
