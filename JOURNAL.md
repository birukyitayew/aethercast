# AetherCast Build Journal

A running record of decisions, experiments, dead ends, and lessons. Newest entries at the top.

---

## 2026-10-22 — The supplicant gets a test bench, and the API keeps its promise

**Merged on `main`**

| PR | What it did |
| --- | --- |
| #15 | `EACCES` on the control socket is now an actionable error that names the owning group and the command to join it; tests accept either errno |
| #16 | `connect_to(path)` plus a fake `wpa_supplicant` datagram socket: eight tests for framing, documents, `FAIL`, `UNKNOWN COMMAND`, empty replies, interleaved events, and timeouts |
| #17 | `SessionStateChanged` is actually emitted by `update_session`, and a session-bus test proves the client receives it |

**Verification on this machine.** `cargo fmt --check` and `cargo clippy
--all-targets -- -D warnings` are clean. `cargo test -p aethercastd` is 46
passed (37 before this pass), the two signal tests pass under
`dbus-run-session`, and the client tests pass under `xvfb-run`. Both media
spikes still pass: the reference sink receives 2052 RTP packets at payload
type 33, and the conformance check measures a 33 ms PCR gap.

**CI caveat.** The pull-request CI runs for #15 and #16 hung for more than
26 minutes in the `apt-get install` step — before compiling any code — and
were cancelled. The merges were made on the local gate the repository
requires (fmt, clippy, tests), not on a green CI run. If a later run fails,
check the runner's package installation first; the diff is not the suspect.

**Privacy redaction.** This repository is public, and earlier entries
contained the home SSID, BSSIDs, and private addresses from real captures.
They are replaced with documentation placeholders (`home-net`,
`02:00:00:00:00:0x`, `192.0.2.x`). No finding, number, or conclusion
changed.

**Issues.** Filed #18 (`StartCast` does not target the device it is given),
#19 (`StopCast` leaves the cast running), and #20 (two CLI `--json` outputs
that are not machine-readable). Closed #6 (the sink question is answered),
#9, #10, and #11 (milestones 2–4 are implemented and proven). Open: #7
(name clearance), #12 (physical sink), #13 (PCR PID), and the three above.

**Handoff.**

- **Done:** the supplicant error path and its protocol tests; session
  signals; `docs/pc-test-checklist.md`; `docs/protocols/wfd-sink-observed.md`;
  refreshed architecture and roadmap; issue housekeeping.
- **Left, no hardware needed:** #19 (stop cancellation) and #20 (JSON
  output), plus the PCR PID decision in #13. None are started; the next
  session can pick them up without re-reading the code.
- **On your PC:** work through `docs/pc-test-checklist.md` in order. Items
  1–3 (doctor, arm, discover) settle whether the radio path is healthy.
  Item 4–6 are the first cast to a physical sink; item 9 answers whether
  this machine can sustain 720p. Record results in
  `docs/compatibility.md`, and mark any PR that depends on them **needs
  hardware test**.

---

## 2026-10-22 — Taking stock on a machine with no radio

**Environment.** This pass was made on a VPS (Ubuntu 26.04, 2 vCPU) with no
wireless adapter: `ip -br link` shows only `lo`, `enp39s0` (Amazon ENA
ethernet) and `docker0`, and `iw` finds no phy. Anything that needs
`wpa_supplicant`, P2P, or a real sink is untestable here. Those checks are
collected in [`docs/pc-test-checklist.md`](docs/pc-test-checklist.md) for the
maintainer's PC. Everything below was actually run here; nothing is assumed.

**Architecture, as built.**

| Crate | Responsibility |
| --- | --- |
| `aethercast-api` | the contract: bus names, `Device`/`Session`/`Diagnostics`, the zbus proxy, and the published XML |
| `aethercastd` | the session daemon: radio health, WFD IE, discovery, P2P, RTSP M1–M7, media pipeline, CLI, D-Bus service |
| `aethercast-client` | a GTK4/libadwaita window that talks only to the API crate |

Inside `aethercastd`, `main.rs` is the CLI and command dispatch; `core.rs` is
the shared `Manager` (devices, sessions, diagnostics, signals) that both the
CLI and the bus sit on; `radio.rs` and `supplicant.rs` own the datagram
control interface to `wpa_supplicant`; `discovery.rs` and `p2p.rs` turn
supplicant events into typed peers and groups; `wfd.rs` encodes and decodes
the WFD information element; `rtsp.rs` is the sink-facing handshake;
`media.rs` builds the GStreamer pipeline; `cast.rs` joins handshake and
pipeline; `dbus.rs` serves the API. The client uses a worker thread and a
channel so the window never blocks on the bus.

**What is proven on this machine.** `cargo build --release`, `cargo fmt
--check`, and `cargo clippy --all-targets -- -D warnings` all pass.
`spike/19-end-to-end.sh` completes the whole M1–M7 handshake against the
reference sink and delivers a stream ffprobe accepts (2052 RTP packets, PT
33). `spike/04-media-conformance.sh` passes every check (video PID 0x1011,
PT 33, 1336-byte datagrams, largest PCR gap 33 ms). Client tests pass under
`xvfb-run`; the API test passes under `dbus-run-session`.

**What failed here, and why.** `supplicant::tests::reports_a_missing_socket`
fails on this host with `EACCES`: `/run/wpa_supplicant` exists with mode
`750 root:netdev`, and the test user is not in `netdev`, so connecting to a
missing socket returns permission denied rather than not-found. CI passes
because its runner has no `/run/wpa_supplicant`. The error is also what a
real unprivileged daemon would show, and "Permission denied (os error 13)"
is exactly the kind of message this project promises not to ship.

**Open issues found in this pass.**

| # | Issue | Severity | Needs radio? |
| --- | --- | --- | --- |
| 1 | `StartCast` records the device name but the cast ignores it: `cast::run` always waits for *a* sink and never targets the named one | high | yes (to finish) |
| 2 | `StopCast` only deletes the session record; the pipeline and its thread keep running | high | no |
| 3 | `SessionStateChanged` is published in the contract and subscribed to by the client, but the daemon never emits it | high | no |
| 4 | `EACCES` on the control socket is reported as a generic I/O error | medium | no |
| 5 | The supplicant protocol layer has one test (a missing socket), so request/reply framing, interleaved events, `FAIL`, and timeouts are unverified | medium | no |
| 6 | The PCR PID is not `0x1000` (issue #13) | medium | no |
| 7 | `aethercastd recover --json` prints human-readable lines before the JSON document | low | no |
| 8 | `media test --udp --json` reports a `file` that was never written | low | no |
| 9 | `docs/architecture.md` still says signals and the client are "next" | low | no |
| 10 | Issues #9, #10, #11 describe milestones that are now implemented | housekeeping | no |
| 11 | A cast to a physical sink, 720p, and HDCP are still unproven | known limit | yes |

**Plan for this pass.**

1. `fix/supplicant-permission-error` — map `EACCES` to an actionable
   `SocketDenied` error and relax the test to accept either errno.
2. `test/supplicant-control-protocol` — make the control client testable
   against a fake socket, then test framing, interleaved events, `FAIL`,
   empty replies, and timeouts. This is the layer every radio feature stands
   on and it can be verified without hardware.
3. `feat/session-state-signals` — emit `SessionStateChanged` whenever a
   session changes, and test it on a real session bus. The API should not
   promise a signal it does not send.
4. `docs/status-and-pc-checklist` — refresh `architecture.md`, add
   `docs/pc-test-checklist.md`, close the completed milestone issues, and
   leave this journal and a handoff accurate.

**Why this order.** The first item was requested and is the smallest. The
second turns the first into a regression test and covers the riskiest
unverified layer. The third makes the published contract true and is fully
testable here. Documentation and housekeeping come last so they describe
what actually landed. `StartCast`/`StopCast` are left as issues: targeting a
named sink is a protocol feature (the source has to connect to the sink's
RTSP port) and cannot be finished or verified without a real sink.

**Result of the pass.** See the entries below, and
[`docs/pc-test-checklist.md`](docs/pc-test-checklist.md) for what still has
to happen on hardware.

---

## 2026-10-20 — Day 0: Research phase begins

**Goal:** build a better open-source Linux casting app, starting with Miracast source support.

**Decisions**
- v1 scope: Linux Miracast source (laptop → TV).
- Research and architecture pack before product code.
- Public GitHub repository.
- Tech stack and license: decided after the research pack, not by preference.
- Working name: **AetherCast** (provisional; trademark/name check pending).

**Environment**
- Kali Linux 2026.2, XFCE, X11.
- Qualcomm Atheros QCA9565 / AR9565 (`ath9k`), 2.4 GHz, single radio.
- Connected to `home-net` on channel 1; P2P device `p2p-dev-wlan0`.
- TV: AltoBeam-based Miracast sink, likely `192.0.2.10` / `.8`, currently not discovered by GNOME Network Displays.
- PipeWire 1.6.6; GStreamer has `ximagesrc`, `openh264enc`, `avenc_aac`, `mpegtsmux`, `rtpmp2tpay`; missing `x264enc` and `pipewiresrc`.
- GNOME Network Displays 0.99.0 source checked out at `/tmp/opencode/gnd-src` (tag `0.99.0`).

**GitHub**
- Authenticated `gh` as `birukyitayew`.
- Public repository: [`birukyitayew/aethercast`](https://github.com/birukyitayew/aethercast).

**Work started**
- Five parallel research tracks launched:
  1. Miracast/WFD and Wi-Fi Direct protocol, plus GND/MiracleCast code audit.
  2. Linux Wi-Fi P2P, wpa_supplicant, NetworkManager, driver/channel constraints.
  3. Screen capture, encoding, muxing, audio, A/V sync, GStreamer pipeline.
  4. Google Cast, AirPlay, DLNA, and open-source prior art.
  5. Product, UX, architecture, packaging, security, licensing, CI, and roadmap.
- Project skeleton and documentation structure created.
- Added ADR-0001 (research first, Miracast source first), issue/PR templates, contributing/security/conduct docs, and roadmap/compatibility placeholders.

**Known hard limits to design around**
- HDCP/DRM cannot be bypassed.
- Wi-Fi P2P support varies by driver and chipset.
- Single-radio laptops cannot be on two channels at once.
- Wayland capture requires the ScreenCast portal; X11 needs a fallback.

**Next**
- Connect the public GitHub repository and push the research pack.
- Synthesize the five research notes into an architecture and roadmap.
- Decide the stack and license with evidence.
- Build a minimal discovery + RTSP handshake spike before a full UI.

---

## 2026-10-20 — The sink was transmitting all along, and it is not a standard sink

The corrected monitor sweep (channels 1/6/11, with a positive control)
finally produced a usable capture, and it changed the picture completely.

**On channel 1, the sink sent six probe requests** from
`02:00:00:00:00:01` — its own MAC. Our earlier captures missed them
because the channel 6 and 11 captures were broken (zero frames, not even
the known `Nebyu` network), and because the vendor service tears its
Wi-Fi Direct listen down whenever its Wi-Fi association changes.

**And its probe requests carry no information elements that any standard
source would recognise:**

```
02:00:00:00:00:01 subtype=4 ie=0  len=0  data=              (SSID, wildcard)
02:00:00:00:00:01 subtype=4 ie=1  len=8  data=82848b960c121824  (supported rates)
02:00:00:00:00:01 subtype=4 ie=50 len=4  data=3048606c         (extended rates)
02:00:00:00:00:01 subtype=4 ie=45 len=26 data=721102ff...     (HT capabilities)
```

**No P2P information element. No WFD information element. No vendor
information element at all.** A sink that advertises nothing cannot be
found by a standard Miracast source — not by Windows, not by Android, not
by GNOME Network Displays, not by us.

This is a **vendor implementation** (`com.hisilicon.miracast`, on a
CVTE/Hisilicon Android TV with an ATBM 2.4 GHz-only chip), and it does
not follow the standard advertising path.

**What we know for certain now**

- The radio is healthy; the searches run to completion; our WFD element
  is correct under every reading of the specification.
- The sink is alive, transmitting, and on the same 2.4 GHz band.
- It does not advertise itself in a way any standard source can use.
- While its screen-share screen is up, it is **off the LAN entirely** —
  so it is not an MS-MICE (infrastructure) sink either.
- When the screen-share screen is closed it rejoins the LAN and its
  service stops; the two states are exclusive.

**The next step is to read the vendor's own code.** The application is
`/system/app/Miracast/Miracast.apk` on the device. Reading it answers
the only question left: does it use a vendor OUI for its WFD element,
what exactly does it expect from a source, and is it a P2P sink at all?

If it turns out to be a proprietary variant, that is not a dead end: it
becomes a documented compatibility entry, and a candidate **vendor
extension provider** — something GNOME Network Displays can never be.

---

## 2026-10-20 — The sink is an Android box, and AOSP's WFD sink is talkative

The TV's screen says **"waiting for connection"** and names the device
**`android_b313`**. That name is the AOSP convention, so the sink is an
**Android TV or an Android TV box** — which fits the AltoBeam Wi-Fi chip
perfectly (AltoBeam is common in low-cost Android boxes and dongles).

That is not a cosmetic detail; it changes what we should expect:

- Android's Wi-Fi Display sink is **AOSP's** `WifiDisplaySinkController`
  and `libwfd`.
- **AOSP's sink sends periodic probe requests carrying its own WFD
  information element** while it is waiting for a source. It is one of
  the chattier WFD implementations.
- Our capture saw **zero** probe requests from any device across three
  channels and ~45 seconds — and only two from a phone in the earlier
  30-second capture.

So a working AOSP sink on 2.4 GHz should have been impossible to miss.
Three readings follow:

1. **The sink's P2P stack is on 5 GHz.** Our adapter is Band 1 only, so
   we cannot see it. Fits the observation exactly.
2. **The box's screen says "waiting for connection" but its Wi-Fi Direct
   stack never started.** Cheap Android boxes ship broken or vendor-
   mangled Wi-Fi drivers, and the screen is served by an app that does
   not necessarily depend on the P2P stack working. If this is the case,
   *no* source could ever connect to it, and that is a sink defect
   rather than a source defect.
3. **The screen is not a Wi-Fi Display sink at all.** "Screen sharing" on
   an Android box may be a vendor app, a DLNA/MediaShare app, or the
   Google Cast path — and the box's device name is what a *Cast* or
   *DLNA* screen would show too.

Reading 2 and 3 are cheap to exclude, and both change the plan, so the
next step is to fingerprint the box on the LAN rather than to keep
poking at the radio: when it leaves the screen-share screen it rejoins
the network, and then its ports and mDNS advertisements tell us exactly
which protocol it speaks.

**What we will build either way.** The provider architecture means the
answer does not change the design, only the order:

| What the box speaks | Our provider | Status |
| --- | --- | --- |
| Miracast over P2P (needs 5 GHz) | WFD P2P | built, waiting on a 5 GHz radio |
| Miracast over infrastructure | WFD MICE | not started |
| Google Cast | Cast control-plane | researched, not started |
| DLNA/UPnP | DLNA | researched, not started |
| AirPlay | AirPlay sender | researched, not started |

---

## 2026-10-20 — The sweep: the software side is now exhausted

The multi-channel capture finished. It is the most informative result of
the project so far, and it is mostly negative — which is progress.

**What the sweep found**

| | Channel 1 | Channel 6 | Channel 11 |
| --- | --- | --- | --- |
| Frames | 3,222 | 0 | 0 |
| Beacons | 185 (the `home-net` AP) | 0 | 0 |
| WFD information elements | none | none | none |
| P2P information elements | none | none | none |
| Probe requests | none | none | none |
| AltoBeam transmitters | none | none | none |

**The sink was in Miracast mode the whole time.** It is absent from the
neighbour table, and a sink in Miracast mode leaves the access point to
form a Wi-Fi Direct group. So the TV was advertising — just not where we
could hear it.

**What that rules out**

- Our radio is healthy: the radio-work queue is empty, searches run to
  completion, and the access point's beacons are captured in the
  expected count.
- Our element is now correct under every reading of the specification
  (`0x1010`, symmetric), and discovery was re-run with it.
- Both directions were exercised: scanning for sinks, and staying
  discoverable for a sink-initiated connection. Neither produced a peer.
- The sink is not advertising a P2P information element on 2.4 GHz at
  all, which rules out a plain "it is on channel 6" explanation.

**What is left**

1. **The sink's Miracast listener is on 5 GHz.** Our adapter is Band 1
   only (`iw phy phy0 info`), and a monitor-mode capture cannot see a
   band the radio does not have. This fits every observation.
2. **The "screen share" screen is not a Wi-Fi Display sink.** Some sets
   label a vendor app or a Chromecast/AirPlay menu "screen share".
3. The sink answers only probes that carry a WFD element *and* the
   element it wants — the `arm --device-info` escape hatch exists for
   that, but we need the sink's own bytes to know what it wants.

**The product answer, whatever the cause**

AetherCast must be able to say *why* it found nothing. The states we
can already detect:

- the adapter is 2.4 GHz only, and sinks advertise on 5 GHz;
- the sink requires HDCP, which we cannot provide;
- the sink's element says the session is unavailable;
- the radio-work queue is wedged.

GNOME Network Displays shows an empty list for all of them.

---

## 2026-10-20 — The sink may be filtering us: four mechanisms, and a bug in our own element

The user asked the right question: maybe the TV only accepts a phone or a
PC, and rejects everything else. That is a real possibility, so we
enumerated the ways a sink can ignore a source.

**1. The WFD device information field.** Two bytes carrying the device
type (source, primary sink, secondary sink, dual), session availability,
the WFD protocol version, MAC address availability, information element
availability, WSD availability, the TDLS preference, HDCP support, time
synchronisation, and the audio flags.

Here we found a bug — **in our element and in GNOME Network Displays'**.
The two readings of the specification disagree about which octet holds
the low bits, and the readings disagree about what the *same bytes*
mean:

```
$ aethercastd wfd decode 000600901c4400c8      # GNOME Network Displays
  bits 0-7 in the low octet
    device type      : source
    session available: true
    TDLS preferred   : true
  bits 0-7 in the high octet
    device type      : source
    session available: false        <- a sink may ignore this
```

The element AetherCast advertised had the same problem: under the
specification's own drawing it claimed **no session availability**.

The fix is to stop guessing: advertise a **symmetric** value, `0x1010` in
both octets, so every reading sees exactly the same thing — a source,
session available, no TDLS preference, no HDCP claim. Both readings now
agree, and a unit test enforces it.

**2. The P2P primary device type.** The P2P device type attribute is
`0050F204-<category>-<subcategory>`: `1-1` computer/PC, `1-6`
computer/desktop, `10-3` phone/smartphone, `7-1` display/TV. A sink may
accept only one category — "a phone" or "a PC" — and ignore the rest.
`wpa_supplicant` takes this from its configuration file, and
**NetworkManager exposes no property for it**, so changing it means
AetherCast has to own the supplicant configuration. That is an
architectural decision for v1, not a flag.

**3. Content protection.** If the sink requires HDCP and the source
advertises none, it may refuse. We advertise no HDCP, which is honest:
we cannot offer it.

**4. The channel and the listen channel.** Covered by the capture sweep.

**Tooling built for this**

- `aethercastd wfd decode <hex>` decodes an element under **both**
  layouts, so a captured sink element immediately shows which reading is
  right — a sink must say "primary sink" and "session available", so only
  one reading can be.
- `aethercastd arm --device-info <hex>` advertises an exact device
  information field, so once we have the sink's own bytes we can copy
  them instead of reasoning about them.

---

## 2026-10-21 — Milestone 8: the client

The window is a GTK4 and libadwaita application, and it sits on the API
the last milestone published. Nothing was duplicated to make that
possible: the contract, the data types, and the client proxy moved into
their own crate, `aethercast-api`, and both the daemon and the client
use them. The client never links the daemon, and the daemon never links
the client, so the two cannot drift apart.

| Crate | What it is |
| --- | --- |
| `aethercast-api` | the contract: the names, the data types, the client proxy, the published XML |
| `aethercastd` | the daemon: the radio, the discovery, the media, the D-Bus service |
| `aethercast-client` | the window: GTK4 and libadwaita, on top of the API |

### What the window shows

- the sinks the machine can see, each with a **Cast** button;
- the sessions that are running;
- the diagnostics: the adapter's bands and limits, the radio's health,
  the station, and the WFD element.

### The part that matters

When there is nothing to cast to, the empty state does not say "no
devices found" and stop there. It says why:

> No sink is advertising on a band this adapter can hear.
> This adapter is 2.4 GHz only (2400-2500 MHz), so a sink that
> advertises on 5 GHz cannot be seen.

That sentence is the difference between AetherCast and a list of empty
results. It comes straight from the daemon's diagnostics, over the bus,
in the same JSON the API publishes — no extra work, no second code path,
and it is right about *this* machine.

### Two things that were harder than they looked

**The application ID cannot be the daemon's bus name.** A GApplication
derives its object path from its own ID, and the daemon already serves
the API at `/io/github/birukyitayew/aethercast`. A client with the same
ID tries to export at the same path, and fails to start with

```
Failed to register: GDBus.Error:org.freedesktop.DBus.Error.UnknownInterface:
Unknown interface 'org.gtk.Actions'
```

which says nothing about the cause. The client therefore uses
`io.github.birukyitayew.aethercast.Client`, and the daemon keeps the
plain name — the name a client library looks up. Recorded in ADR-0004.

**An AdwApplicationWindow has a content of its own.** `gtk_window_set_child`
on one is a fatal error (`Adwaita-ERROR: gtk_window_set_child() is not
supported for AdwApplicationWindow`), and the window exits with SIGABRT
before it is ever drawn.

### The window never blocks on the bus

A worker thread holds the connection, subscribes to the signals, and
sends what it learns over a channel; the window picks it up on a 200 ms
timer, and the user's commands travel back the same way. The interface
cannot freeze because the daemon is slow or gone, and the banner offers
to start the daemon when it is not there.

### Verified

The client runs on the reference machine, against the real daemon, on the
real session bus, and says the sentence above. The screenshot in
`docs/images/client.png` is that window.

The client's tests run in CI under `xvfb-run`, because a build server has
no display.

---

## 2026-10-21 — Milestone 7: the D-Bus API

The GUI needs a contract to sit on, so the contract came first.

The daemon now serves `io.github.birukyitayew.aethercast.Manager` on the
session bus, and the contract is published as
`dbus/io.github.birukyitayew.aethercast.Manager.xml`. A test compares
the method names in the contract with the ones the implementation
serves, and the daemon **refuses to start** if the contract does not
describe the interface it serves: a contract that has drifted is worse
than no contract at all.

| Method | What it does |
| --- | --- |
| `Version` | the interface version |
| `ListDevices` | the sinks we know about, as JSON |
| `ListSessions` | the casting sessions, as JSON |
| `GetDiagnostics` | a snapshot of the environment, as JSON |
| `ArmWfdElement` | advertise a WFD information element, as hex |
| `StartCast` | cast to a device, and return a session identifier |
| `StopCast` | stop a session |

Structured data travels as **JSON**, so the interface can grow without
breaking the clients that came before it.

The command line and the bus sit on the same `core::Manager`, and the
cast itself moved into its own module, so both front ends run exactly
the same code and cannot disagree about what the daemon is doing.

Verified from outside the process:

```
  $ gdbus call --session --dest io.github.birukyitayew.aethercast \
      --object-path /io/github/birukyitayew/aethercast \
      --method io.github.birukyitayew.aethercast.Manager.GetDiagnostics
  {"interface":"wlan0",
   "station":{"ssid":"home-net","wpa_state":"COMPLETED","connected":true},
   "radio_work_items":0,"radio_healthy":true,
   "adapter":{"bands":"2.4 GHz",
              "limitation":"this adapter is 2.4 GHz only (2400-2500 MHz),
                           so a sink that advertises on 5 GHz cannot be seen"},
   "wfd_element":"00000610101c4400c8"}
```

**That response is the product.** A user of the future client will see
*why* nothing was found, not an empty list — which is the difference
between GNOME Network Displays and AetherCast.

**Signals are next**, with the client: a client has to be told when a
device appears and when a session changes state, rather than polling.

---

## 2026-10-21 — Milestone 6: a cast over the Wi-Fi Direct group interface

The last untested line in the product is now tested. With the group
formed, the RTSP server bound to the **group interface's** address, and a
sink connecting to that address:

```
  the group is up on p2p-wlan0-4 (192.168.43.1)
  a sink must join the group and connect to 192.168.43.1:7236
  sink connected from 192.0.2.1:41848
  -> M1 OPTIONS (Require: org.wfa.wfd1.0)
  ...
  <- M7 PLAY
  -> M7 100 Continue
  handshake reached Playing
  streaming to 192.168.43.1:50000 for 12s
```

and on the sink's side:

```
  [reference-sink] received 290 RTP packet(s), 381640 bytes of MPEG-TS
  [reference-sink] RTP payload types: [33]
```

So the whole source path is proven end to end, on the same interface a
real sink would use:

| Step | Proven by |
| --- | --- |
| Discovery and the global WFD element | `aethercastd discover`, `arm` |
| Group formation | `aethercastd p2p group` |
| The RTSP server on the group's address | `spike/20-group-cast.sh` |
| The media path | `spike/19-end-to-end.sh`, `spike/04-media-conformance.sh` |
| The conformance of the stream | PID map, payload type, packetisation, PCR cadence |

**What is still not proven**, and it is one thing: the **radio link
itself**, because a second Wi-Fi Direct device is needed to join our
group. Everything on either side of that link is done.

**Two bugs in the check itself**, which is worth recording because the
failure looked like a product bug:

* the transport stream can have its final size before its tables are
  readable, so the validation raced the writer;
* the group test never asked for audio, and then it checked for an audio
  PID that could not exist.

The general lesson, and it is the same one the whole project is built
on: **a check that cannot explain its own failure will be believed over
the code.** Both were found by reading the actual output rather than the
summary.

---

## 2026-10-21 — Milestone 5: an end-to-end cast

The TV could not be made to cooperate — it advertises nothing, its driver
refuses to change channel, and it is a system app we must not touch. So
the project stopped depending on it and **built its own sink**.

`spike/18-reference-sink.py` plays the sink's role properly: it connects
to the source's RTSP control port, answers M2, publishes its capabilities
in M3, accepts M4, sends M6 and M7, and then **receives the RTP media
and writes the transport stream to a file**.

`spike/19-end-to-end.sh` runs the whole path and validates it:

```
$ ./spike/19-end-to-end.sh 12
  [reference-sink] received 150 RTP packet(s), 406644 bytes of MPEG-TS
  [reference-sink] RTP payload types: [33]
  PASS  the video PID is the WFD one (0x1011)          0x1011
  PASS  the audio PID is the WFD one (0x1100)          0x1100
  PASS  the video codec                                h264
  PASS  the audio codec                                aac
== the source cast a conformant stream to a sink ==
```

**This is the first real cast in the project.** Screen capture, H.264
Constrained Baseline encoding, MPEG-TS with the WFD PID map, RTP with
payload type 33, and a sink that receives a stream ffprobe accepts.

Two things this buys us beyond the demonstration:

- **The media path is testable in CI**, with no hardware and no TV. The
  reference sink is the project's own oracle.
- **The product no longer depends on a broken device.** A real TV is now
  a compatibility question, not a blocker.

What is still missing for a physical sink: P2P group formation, and the
sink connecting to our RTSP port on the group interface. The reference
sink connects over the LAN instead, which is the only difference.

---

## 2026-10-20 — Milestone 4: Wi-Fi Direct, and the wrong model of it

**What we learned by trying to form a group on this hardware**

`aethercastd p2p group` asks `wpa_supplicant` to become a Group Owner on its own. It works — the interface `p2p-wlan0-0` appears with `type P2P-GO` — and then the group is **removed again within milliseconds**, because an autonomous group with no client attached has no reason to exist. The supplicant's log says it plainly:

```
P2P-GROUP-STARTED p2p-wlan0-3 GO ssid="DIRECT-I1" freq=2412 go_dev_addr=02:00:00:00:00:03
p2p-wlan0-3: interface state ENABLED->DISABLED
AP-DISABLED
CTRL-EVENT-DISCONNECTED reason=3 locally_generated=1
nl80211: deinit ifname=p2p-wlan0-3
```

So the autonomous Group Owner is the wrong model for Miracast. **The sink initiates**: the source stays discoverable, the sink finds it, and the group forms with the sink attached — which is why the group then has a client, an address, and a reason to live. `aethercastd p2p listen` implements that: arm the WFD element, `P2P_LISTEN`, and wait for a group interface that has an IPv4 address, which is the signal that a sink has joined.

**Three things that cost time and are worth writing down**

1. **The group-start event does not reach the control socket.** It is written to the supplicant's log instead, because the P2P group interface is an internal device with no control socket of its own. The daemon therefore watches `/sys/class/net` for a new `p2p-*` interface, and reads its address with `getifaddrs`. That is unprivileged and does not depend on how the supplicant chooses to publish its events.
2. **The snapshot has to be taken before the group is created.** Taking the list of interfaces afterwards means the new one is already in it, and the group is never noticed. (Our first implementation made exactly this mistake.)
3. **The group interface has no address until a client attaches**, so "the interface exists" is not enough — the signal is "the interface exists *and* has an IPv4 address". That address is the address the sink will use to reach our RTSP server.

**Where the project stands**

| Piece | State |
| --- | --- |
| Radio health, wedge recovery, WFD element | done |
| P2P discovery, listen mode, group detection | done |
| WFD RTSP handshake M1-M7 | done, tested against a mock sink |
| Media pipeline, WFD conformance | done, all checks pass |
| **A sink that answers** | **the one missing thing** |

---

## 2026-10-20 — Milestone 3: the WFD RTSP handshake

**What we built**

`crates/aethercastd/src/rtsp.rs` implements the source side of the Wi-Fi Display RTSP profile: M1 `OPTIONS`, M2, M3 `GET_PARAMETER` with the sink's capabilities parsed, M4 `SET_PARAMETER` with our format selection, M5, M6 `SETUP` with the `Session` header, and M7.

**The protocol no longer needs a TV to be developed against.** `spike/06-mock-sink.py` plays the sink's role exactly, and `spike/07-rtsp-handshake-test.sh` checks the whole sequence:

```
PASS  the server logged '-> M1 OPTIONS'      PASS  '<- M6 SETUP'
PASS  the server logged '<- M2 OPTIONS'      PASS  '<- M7 PLAY'
PASS  the server logged '-> M3 GET_PARAMETER' PASS  the RTP port was parsed
PASS  the server logged '-> M4 SET_PARAMETER' PASS  the handshake reached the playing state
```

The second RTP port is read the way the specification defines it — a second RTP stream for a coupled secondary sink, **not** RTCP — so a sink reporting `rtp-port1=0` means "one MPEG-TS stream" rather than a reserved combination. This is the detail the code audit found GNOME Network Displays getting wrong.

**The topology, established by experiment**

| Question | Answer | How |
| --- | --- | --- |
| Is this machine on Ethernet? | **No** — it has no Ethernet interface at all, only `wlan0` | `ip -br link` |
| Where is the sink? | `192.0.2.10` and `.8`, AltoBeam MAC `02:00:00:00:00:01`, **REACHABLE** | neighbour table sweep |
| Is the network 5 GHz? | **No** — the `home-net` AP is channel 1 (2412 MHz) and there is no 5 GHz network anywhere | `nmcli dev wifi list` |
| Can the adapter see 5 GHz? | **No** — `iw phy phy0 info` shows Band 1 only | `iw phy` |
| Does the sink advertise mDNS services? | **No** — `_display._tcp`, `_googlecast._tcp`, `_airplay._tcp` are all empty, and no casting port is open | `avahi-browse -rt` |
| Did the sink leave the AP earlier? | **Yes** — the AltoBeam device disappeared entirely while the sink was in Miracast mode, which is exactly what entering Wi-Fi Direct does | neighbour table, before and after |

So the sink's Miracast listener is on the same 2.4 GHz band we can hear, which rules out the "5 GHz only" explanation. It is a Wi-Fi Direct question, not a hardware-blindness question — and the remaining test is a monitor-mode capture of the air, which is the only thing a managed interface cannot do.

**Next**
- The sink enters Miracast mode, and the monitor-mode capture (`spike/05-monitor-capture.sh`) shows whether it transmits on 2.4 GHz and what it advertises.
- P2P group formation: `P2P_GROUP_ADD`, the sink joining our group, and the RTSP server bound to the group interface.

---

## 2026-10-20 — Milestone 2: the media pipeline conforms

**What we built**

`crates/aethercastd/src/media.rs` builds the real capture → encode → mux → transport pipeline, and `spike/04-media-conformance.sh` proves it against the specification.

```
$ ./spike/04-media-conformance.sh
== 1. MPEG-TS file ==
  PASS  video PID is 0x1011                            0x1011
  PASS  audio PID is 0x1100                            0x1100
  PASS  video codec                                    h264
  PASS  audio codec                                    aac
== 2. PCR cadence (the specification requires 100 ms or less) ==
  PASS  largest PCR gap is at most 100 ms              yes
        largest gap: 33 ms
== 3. RTP transport ==
  PASS  RTP payload type is 33                         33
  PASS  UDP payload is 1336 bytes (7 TS packets)       1336
  PASS  the payload is MPEG-TS (sync byte 0x47)        47
== all checks passed ==
```

**The GStreamer API cost us more than the protocol did**

| Finding | Detail |
| --- | --- |
| GEnum properties | `queue leaky`, `openh264enc rate-control` and `videorate framerate` are enums, and the Rust bindings do not generate them. `set_property_from_str` resolves them by name; the launch parser does too. |
| `videoconvert` will not scale | The reference display is **1821x1024 at 25 fps**, an odd geometry. `videoconvert` refuses the size negotiation, so `videoscale` has to be explicit. |
| `videorate` has no `framerate` | GStreamer 1.28 replaced it with `max-rate` (frames per second). A fixed rate is imposed by a `capsfilter` *after* `videorate`, so it can duplicate as well as drop. |
| The element is `pulsesrc` | `pulseaudio` is the *plugin* name. The GStreamer PulseAudio device provider also enumerates nothing on PipeWire, so the source has to be named explicitly — the system mix is `alsa_output.pci-0000_00_1f.3.analog-stereo.monitor`. |
| The plugin cache can blacklist a plugin | After the distribution upgrade, `libgstpulseaudio.so` stayed at 1.28.3 while the core moved to 1.28.7, and the plugin ended up blacklisted in the registry cache. Clearing `~/.cache/gstreamer-1.0/registry.*.bin` restores it. |
| `mpegtsmux` has no PAT/PMT PID properties | The WFD PIDs come from the **request pad names** — `sink_4113` is 0x1011, `sink_4352` is 0x1100. The PCR lands on the first video PID rather than the 0x1000 the specification asks for: a known gap, tracked. |
| `libpulse` still aborts on default server discovery | The same crash that broke GNOME Network Displays. The PulseAudio socket has to be named explicitly. |

**The finding that changed the default profile**

The first conformance run failed: a **117 ms PCR gap**, against a 100 ms limit. The cause is the encoder, not the muxer — exactly what the research predicted.

| Profile | Largest PCR gap | Verdict |
| --- | --- | --- |
| 1280x720p30 | 133 ms | **fails** — OpenH264 cannot sustain 720p30 on this CPU |
| 640x480p30 | 67 ms, then 33 ms | **passes** |

640x480 is also the format WFD mandates as a minimum, so nothing is given up. **The default profile is now 640x480p30**, and 720p30 is only offered with a hardware encoder, which needs `gstreamer1.0-vaapi` and a driver that supports it.

**The remaining piece**

The pipeline is proven; what is missing is the sink. The next milestone is P2P group formation and the RTSP handshake, so that a real TV can be asked for its capabilities.

---

## 2026-10-20 — Milestone 1: `aethercastd` exists

**What we built**

The first real AetherCast code: a Rust daemon with a CLI, 14 unit tests, and a clean clippy run. It is not a caster yet — it is the layer that decides whether a cast can start at all.

| Module | What it does |
| --- | --- |
| `supplicant.rs` | Client for the `wpa_supplicant` control interface |
| `wfd.rs` | WFD information element: build, parse, and decode device information |
| `radio.rs` | Radio health, wedge detection and recovery, Wi-Fi Direct control |
| `discovery.rs` | Peer model and event parsing |
| `main.rs` | CLI: `doctor`, `arm`, `recover`, `discover` |

**The control interface is not what the documentation suggests.** Four things cost us real time and are worth recording:

1. It is a **datagram** socket (`SOCK_DGRAM`), not a stream. `UnixStream` fails with `Protocol wrong type for socket`.
2. The client must **bind a local address** first. Without it the request is delivered but the reply is lost and the caller blocks forever — `wpa_ctrl_open()` does the same thing.
3. Commands must be sent **without a trailing newline**. The control interface parses the rest of the line as arguments, and the stray newline makes `WFD_SUBELEM_SET` fail on the hex payload.
4. Most commands reply `OK`/`FAIL`, but `STATUS`, `WFD_SUBELEM_GET`, `RADIO_WORK show` and `P2P_PEER` return a **raw document**. And there is no `P2P_PEERS` command: peers are walked with `P2P_PEER FIRST` then `P2P_PEER NEXT-<address>`.

**Two protocol bugs our own tests caught**

- The WFD device information field is a big-endian 16-bit value with **bits numbered from the LSB of the low byte**, not from the first octet. The first implementation had the two bytes swapped, which made GNOME Network Displays' own element decode as "session unavailable".
- `P2P_EVENT_DEVICE_FOUND` carries **two** addresses: the bare address the frame came from, and `p2p_dev_addr`, the peer's Wi-Fi Direct device address. They are not the same value.

**What it does on this machine, right now**

```
$ aethercastd doctor
interface        : wlan0
station          : home-net (02:00:00:00:00:02)  [connected]
channel          : 2412 MHz
radio-work queue : 0 item(s)
radio health     : ok
wfd element      : armed
  - id=0 body=00101c4400c8
  device type     : source  session available: true  TDLS preferred: false

$ aethercastd discover --duration 25
no peers found in 25s
  radio: healthy, the search ran to completion
  hint: make sure the sink is in Miracast / Screen Share mode
```

The WFD element we advertise is AetherCast's own: source, session available, **TDLS preference cleared** — the bit GNOME Network Displays sets incorrectly.

**Still missing, in order**

1. The sink is still not discovered. The radio is healthy, the search completes, the probe requests carry a WFD IE. The remaining variable is the TV itself.
2. P2P group formation (GO negotiation) — `P2P_GROUP_ADD`, invitation, the sink joining our group.
3. The WFD RTSP server: M1 `OPTIONS`, M2 public device information, M3 session description, M4 accept, M5 establish.
4. The media pipeline, which needs the GStreamer development headers.
5. The GUI, last.

**Next**
- Group formation and the RTSP handshake, so a sink can be *asked* for its capabilities.
- The media pipeline, once `gstreamer1.0-dev` and friends are installed.

---

## 2026-10-20 — Phase 0 spike: the radio-work wedge

**Goal:** answer one question with evidence — does the AltoBeam TV respond to Wi-Fi Display probes? That question turned out to be downstream of a much bigger finding.

**What we found**

1. **GNOME Network Displays' empty sink list is not an application bug.** `wpa_supplicant`'s radio-work queue was wedged: one `p2p-scan` work item stuck at `started=1` for 12,737 seconds (3h31m), with 45 NetworkManager scans queued behind it and growing by one every 5 minutes. Every P2P find was rejected instantly (`P2P: Reject scan trigger since one is already pending`) and even a plain `wpa_cli scan` returned `FAIL-BUSY`. There was not a single `P2P: Scan results received` event in the whole session, and no kernel-side error: the driver accepted the request and never completed it.
2. **Recovery exists and needs no root.** `wpa_cli -i wlan0 FLUSH` aborted the stuck scan: queue 47 → 0, plain scan `FAIL-BUSY` → `OK`, `p2p_find` `FAIL` → `OK` and completing without re-wedging. The station is reactivated automatically afterwards (~10 s).
3. **We can set a global WFD IE without root, and NetworkManager cannot.** `wpa_cli WFD_SUBELEM_SET all <hex>` stores a WFD-IE body in wpa_supplicant; `wifi_display_update_wfd_ie()` propagates it into `p2p_set_wfd_ie_probe_req()`, and `p2p_scan_ie()` adds it to every P2P find scan's probe requests (`params->p2p_probe = 1`). **Our discovery probes now carry a real WFD IE** — something the existing app can never do, because NM exposes no API for it. The subelement ID space is 0–11 (`MAX_WFD_SUBELEMS`).
4. **The TV has still not been seen.** Discovery now runs cleanly, but zero peers, zero `P2P_EVENT_DEVICE_FOUND`. The remaining suspects are the TV not being in Miracast mode, and the single-radio channel constraint.

**Delivered**
- `spike/01-radio-work-wedge-report.md` — full technical report with source-level references
- `spike/02-recover-radio-work.sh` — automated wedge recovery
- `spike/03-discovery-window.sh` — supervised discovery window
- `spike/evidence/radio-work-wedged.txt` — the captured queue

**Next**
- Put the TV in Miracast mode and re-run the discovery window with the WFD IE armed.
- If the sink still does not appear, capture the air in monitor mode to see whether it answers a WFD-bearing probe at all.
- Design implication for v1: the "no sinks" screen must diagnose the radio, not just list devices.

---

## 2026-10-20 — Research phase complete

**What landed**
Five cited research notes, ~30,000 words, 393 URLs, 393+ line-level claims labelled verified / reported / speculative:

| Note | Words |
| --- | --- |
| 01 — Miracast / WFD protocol + GND code audit | 5,999 |
| 02 — Linux Wi-Fi P2P, drivers, channel constraints | 5,981 |
| 03 — Media pipeline, codecs, latency | 5,958 |
| 04 — Google Cast, AirPlay, DLNA, prior art | 6,334 |
| 05 — Product, architecture, packaging, license, name | 5,999 |

**The findings that matter**
1. Discovery fails *before* RTSP. The machine has no global WFD IE (`wpa_cli get WFDIEs` → `FAIL`); NetworkManager only applies `wfd-ies` at activation, never while scanning. Prime suspect for "discovers no sinks".
2. The radio is the binding constraint. `ath9k` on the QCA9565/AR9565 is `#channels <= 1`; a P2P group on another channel fails with `-2`. Discovery off-channel still works via ROC.
3. NetworkManager cannot express GO intent, operating frequency, or peer channel — a narrow helper is required.
4. Software encode is 720p30-only and only with x264. Measured here: OpenH264 0.32–0.81× realtime at 720p30, x264 2.64×. `vah264enc` is missing from stock GStreamer 1.28.3.
5. GND's negotiation core has four high-severity defects (no global WFD IE in discovery, second RTP port misread as RTCP, resolution selection compiled out, 512 kbit/s clamp). Forking inherits them.
6. "AetherCast" collides with `ubports/aethercast` (archived 2022) and Canonical's 2015 Ubuntu Miracast project. Dormant, not live. Name kept, risk recorded in ADR-0004.

**Decisions taken (maintainer sign-off)**
- **License:** GPL-3.0-or-later, DCO, no CLA (ADR-0003 → Accepted).
- **Architecture:** Rust control plane + GStreamer media plane, daemon + versioned D-Bus + GTK4 + CLI (ADR-0002 → Accepted).
- **Name:** the project is **AetherCast** and the repository is `birukyitayew/aethercast`. The prior-art collision (`ubports/aethercast`, Canonical 2015) is dormant and is recorded as an accepted risk with a clearance gate and an explicit rename trigger in ADR-0004.

**Decisions recorded**
- ADR-0002 (Proposed): Rust control plane, GStreamer media plane, daemon + versioned D-Bus + GTK4 + CLI.
- ADR-0003 (Proposed): GPL-3.0-or-later, DCO, no CLA, no DRM circumvention, no proprietary codec bundling.
- ADR-0004 (Accepted): the project keeps the name AetherCast; the prior-art collision is an accepted risk with a clearance gate before the first release and an explicit rename trigger.

**Next**
- Milestone 1: a headless discovery spike that sets the global WFD IE and captures a scan, to settle whether the AltoBeam TV answers WFD probes at all.
- Then a minimal WFD M1–M3 exchange to read the sink's real capability bitmap.
- Then the first frame, then the UI.
