# Architecture

**Status:** the daemon, the published API, and the GTK4 client are built and
tested. A cast is proven end to end against a reference sink. What is missing
is a cast to a physical sink, which needs hardware this repository's
development machine does not have — see
[`pc-test-checklist.md`](pc-test-checklist.md).

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

## The API

The daemon serves one interface on the session bus:

```
io.github.birukyitayew.aethercast.Manager
  at /io/github/birukyitayew/aethercast
```

| Method | What it does |
| --- | --- |
| `Version` | the interface version |
| `ListDevices` | the sinks we know about, as JSON |
| `ListSessions` | the casting sessions, as JSON |
| `GetDiagnostics` | a snapshot of the environment, as JSON |
| `ArmWfdElement` | advertise a WFD information element, as hex |
| `StartCast` | cast to a device, and return a session identifier |
| `StopCast` | stop a session |

Structured data is returned as **JSON**, so the interface can grow
without breaking the clients that came before it. The published contract
is [`dbus/io.github.birukyitayew.aethercast.Manager.xml`](../dbus/io.github.birukyitayew.aethercast.Manager.xml),
and a test checks that the contract and the implementation do not drift
apart — the daemon refuses to start if the contract does not describe the
interface it serves.

The command line and the bus sit on the same `core::Manager`, so they
can never disagree about what the daemon is doing, and neither of them
owns state that the other one also owns.

Signals complete the API. `DeviceFound` carries a device as JSON when the
background search sees one for the first time, and `SessionStateChanged`
carries the session id and its new state whenever a session changes. Both are
implemented in the daemon, published in the contract, and consumed by the
client — the client polls as a safety net, not because it has to.

### The three crates

| Crate | What it is |
| --- | --- |
| `aethercast-api` | the contract: the names, the data types, the client proxy, and the published XML |
| `aethercastd` | the daemon: the radio, the discovery, the media, and the D-Bus service |
| `aethercast-client` | the window: GTK4 and libadwaita, on top of the API |

The API lives in its own crate so that the daemon and the client cannot
drift apart: both use the same definitions, the same proxy, and the same
contract file. The client never links the daemon, and the daemon never
links the client.

## The client

The client is a GTK4 and libadwaita application. It shows three things:

- **the sinks the machine can see**, each with a Cast button;
- **the sessions that are running**;
- **the diagnostics**: the adapter's bands and limits, the radio's
  health, the station, and the WFD element.

The part that matters is what happens when there is nothing to cast to.
The empty state does not say "no devices found" and stop there — it says
*why*:

> No sink is advertising on a band this adapter can hear.
> This adapter is 2.4 GHz only (2400-2500 MHz), so a sink that
> advertises on 5 GHz cannot be seen.

That sentence is the difference between AetherCast and a list of empty
results. It comes straight from the daemon's diagnostics, over the bus,
in the same JSON the API publishes.

### How the client is put together

The window never blocks on the bus. A worker thread holds the connection
and sends what it learns over a channel; the window picks it up on a
200 ms timer, and the user's commands travel back the same way. The
interface cannot freeze because the daemon is slow or gone, and the
banner offers to start the daemon when it is not there.

### The application ID

The client uses `io.github.birukyitayew.aethercast.Client`, and the
daemon keeps `io.github.birukyitayew.aethercast`. They cannot be the
same string: a GApplication derives its object path from its own ID, and
the daemon already serves the API at that path. The reason is recorded
in [ADR-0004](adr/0004-name-aethercast.md).

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
| The supplicant control protocol is correct | `supplicant` tests against a fake datagram socket: framing, documents, `FAIL`, empty replies, interleaved events, timeouts |
| The API tells clients about changes | tests on a real session bus for `DeviceFound` and `SessionStateChanged` |

## What is not proven

- **A cast to a physical sink.** The reference sink connects over the
  loopback interface. A real sink joins our Wi-Fi Direct group and
  connects to the group's address. The group forms — `p2p group` creates
  the interface — but the group interface has no address until a peer
  joins, so the last mile cannot be exercised without a second Wi-Fi
  Direct device. The exact checks are in
  [`pc-test-checklist.md`](pc-test-checklist.md).
- **Casting to the device the user chose.** `StartCast` takes a device id,
  but the session waits for whichever sink connects first; the source does
  not yet connect to the sink's control port itself. The client's Cast
  button therefore starts a session rather than targeting that sink. Filed
  as an issue; finishing it needs a real sink.
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

1. **A physical sink.** P2P group formation with a peer, the RTSP server
   bound to the group's address, and a real TV or phone on the other
   end. [`pc-test-checklist.md`](pc-test-checklist.md) is the run book.
2. **Targeted casting.** `StartCast(device)` must connect to that device's
   control port instead of waiting for any sink, and `StopCast` must stop
   the pipeline rather than only forgetting its record.
3. **720p.** It needs hardware encoding; software encoding is measured
   below realtime at 720p30 on the reference CPU.
4. **More protocols.** The provider interface is designed for it: Google
   Cast as a control-plane provider, DLNA, and AirPlay with the FairPlay
   line written into the contributor guidance before any of it starts.
