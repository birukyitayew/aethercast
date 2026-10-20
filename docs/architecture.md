# Architecture

**Status:** the first two milestones are built and tested. This document
describes what exists, what is proven, and what is not.

## The shape of the problem

Casting is a stack of independent problems, and most of them fail
silently. The design goal was therefore not "support Miracast" but
**"always be able to say why it is not working"**.

| Layer | What it does | Where it lives |
| --- | --- | --- |
| Radio | Wi-Fi Direct, the global WFD information element, group formation | `radio.rs`, `p2p.rs` |
| Protocol | The Wi-Fi Display RTSP handshake, M1 to M7 | `rtsp.rs` |
| Media | Capture, encode, MPEG-TS with the WFD PID map, RTP | `media.rs` |
| Diagnosis | What the adapter can do, what the radio is doing, what the sink said | `hw.rs`, `doctor`, `discover` |
| Interface | A CLI first, a GTK4 client later | `main.rs` |

The protocol layer is deliberately small and the diagnosis layer is
deliberately loud, because the research showed that the difference
between the existing implementations and a good one is not protocol
coverage — it is telling the truth about failure.

## The daemon

`aethercastd` is a single process that owns the session state machine.
The media pipeline runs in the *same* process for now, behind a clean
interface, so that a media crash cannot be confused with a protocol
crash. Splitting it into a separate process is a later change, not a
redesign.

```
aethercastd
├── doctor        radio health, wedge detection, adapter capabilities
├── arm          set the global WFD information element
├── discover     search for sinks, and report why none were found
├── p2p group    become the Group Owner and detect the group interface
├── p2p connect   invite a peer into our group
├── p2p listen    stay discoverable for a sink-initiated connection
├── wfd decode    decode an information element under every layout
├── media probe   what the pipeline can use on this machine
├── media test    run the pipeline into a file and validate it
├── rtsp serve    run the WFD handshake against a sink
├── cast          handshake and stream to a sink
└── recover       flush a wedged radio-work queue
```

## The three things that were hard

### 1. The radio-work queue can wedge, and nothing tells you

`wpa_supplicant` serialises scans through a radio-work queue. If a scan
never completes — which the ath9k driver on a single-radio adapter can
do — the queue is blocked forever. Every later request is rejected
instantly, and NetworkManager keeps adding to the queue, so it grows
without bound.

The symptom is an empty device list. The cause is a wedged queue. The
fix is `FLUSH`, and AetherCast both detects the wedge and recovers it
(`doctor`, `recover`).

### 2. The global WFD information element is ours to set, and it is easy to get wrong

NetworkManager exposes no API for the global WFD element, and
`wpa_supplicant` does — `WFD_SUBELEM_SET`. Without an element in the
probe requests, a strict sink may ignore us entirely.

Worse, the two readings of the specification disagree about the bit
layout of the 2-byte device information field, and the readings disagree
about what the *same bytes* mean. AetherCast therefore advertises a
**symmetric** value, `0x1010` in both octets, so that every reading
sees the same thing: a source, session available, no TDLS preference, no
HDCP claim. `wfd decode` shows both readings side by side, so a real
sink's element settles the question with evidence rather than argument.

### 3. A sink that advertises nothing cannot be found

The reference implementation in this repository found a real TV that
speaks the standard protocol, on the right band, with a healthy radio on
our side — and that TV sent **no P2P element, no WFD element and no
vendor element at all**. It was invisible to every source, including
Windows and GNOME Network Displays.

That is a class of failure, not a quirk, and it is why the diagnosis
layer exists.

## The reference sink

`spike/18-reference-sink.py` is the project's oracle. It plays the sink
role completely: it connects to the source's RTSP control port, answers
M2, publishes its capabilities in M3, accepts M4, sends M6 and M7, and
then receives the RTP media and writes the transport stream to a file.

Because it is ours, it gives us three things a borrowed device cannot:

- **an end-to-end test with no hardware**, which is what CI runs;
- **a place to prove the media path** — PID map, payload type, packet
  count, PCR cadence — independently of any sink's willingness to
  cooperate;
- **a regression test** for the handshake, which is otherwise only
  exercisable against a TV that may be switched off.

## What is proven

| Claim | How it is proven |
| --- | --- |
| The WFD handshake works | `spike/07-rtsp-handshake-test.sh`, 8 message checks |
| The media path conforms | `spike/04-media-conformance.sh`: PIDs `0x1011` / `0x1100`, PT 33, seven TS packets per datagram, PCR ≤ 100 ms |
| A full cast works | `spike/19-end-to-end.sh`: capture → encode → mux → RTP → a sink that ffprobe accepts |
| The radio is healthy and recoverable | `doctor`, `recover`, and the wedge report |
| The adapter's limits are visible | `doctor` names the band limitation |

## What is not proven

- **A cast to a physical sink.** The reference sink connects over the
  loopback interface. A real sink joins our Wi-Fi Direct group and
  connects to the group's address. The group forms — `p2p group` creates
  the interface — but the group interface has no address until a peer
  joins, so the last mile cannot be exercised without a second Wi-Fi
  Direct device.
- **720p30.** Measured at 0.32–0.81× realtime with OpenH264 on this
  CPU, so 720p needs hardware encoding. 640×480p30 is the default and is
  the format the specification mandates.
- **HDCP.** Not implemented, and not implementable without a licence.
- **Every sink.** One real TV was examined and found not to advertise
  itself. The compatibility matrix records what we know, not what we
  assume.

## Decisions that shaped this

See the ADRs:

- [ADR-0001](adr/0001-research-first-and-v1-scope.md) — research before code
- [ADR-0002](adr/0002-rust-control-gstreamer-media.md) — Rust control plane, GStreamer media plane, daemon + D-Bus + GTK4 + CLI
- [ADR-0003](adr/0003-license-gpl3.md) — GPL-3.0-or-later, DCO, no CLA
- [ADR-0004](adr/0004-name-aethercast.md) — the name, and the collision we accept

## Where the next milestones go

1. **Physical sink.** P2P group formation with a peer, the RTSP server
   bound to the group's address, and a real TV or phone on the other
   end.
2. **The client.** A GTK4 application over the D-Bus API, with the
   device list, the quality presets, and the diagnostics the CLI
   already prints.
3. **More protocols.** The provider interface is designed for it: Google
   Cast as a control-plane provider, DLNA, and AirPlay with the FairPlay
   line written into the contributor guidance before any of it starts.
