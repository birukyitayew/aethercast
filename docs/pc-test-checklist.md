# PC Test Checklist

**Why this file exists.** The repository is developed on a machine with no
wireless adapter, so every check that needs a radio, a real sink, or a
display is collected here. Each item lists the exact command and the result
that means it passed. Nothing here has been observed on the development
machine; it is the list of things that still need a PC.

Work through the items in order: each one needs the previous one to be
healthy. Record what you see, including failures.

## Before you start

- A wireless adapter that supports Wi-Fi Direct. `iw list` must list
  `P2P-client` and `P2P-GO` under the supported interface modes.
- A dual-band adapter is strongly preferred; a 2.4 GHz-only adapter cannot
  hear a sink that listens on 5 GHz, and AetherCast will say so.
- A Miracast sink (TV, dongle, or an Android device in receiver mode) in its
  screen-share / Miracast mode.
- A second network path (ethernet, or a phone hotspot) if you are working
  over SSH: casting takes over the Wi-Fi radio.
- Build once: `cargo build --release`.

Set the interface name used below:

```console
$ IFACE=wlan0   # change to the wireless interface from `iw dev`
```

## 1. The adapter and the radio are healthy

```console
$ ./target/release/aethercastd doctor --iface "$IFACE"
```

Expected:

- `radio-work queue : 0 item(s)` and `radio health : ok`
- `adapter` names the phy and its bands
- `wfd element : not armed` on a fresh boot (this is correct before item 2)

Pass: the queue is empty and the adapter line names real bands rather than
`unknown`. A 2.4 GHz-only adapter printing the limitation sentence is
**correct behaviour**, not a failure.

If the queue is not empty, run `aethercastd recover --iface "$IFACE"` and
repeat. If it wedges again, record the queue contents and open an issue.

## 2. The global WFD information element can be armed

```console
$ ./target/release/aethercastd arm --iface "$IFACE"
$ ./target/release/aethercastd doctor --iface "$IFACE"
```

Expected:

- `armed WFD element: …` from the first command
- `wfd element : armed` from the second, with `id=0` and a body carrying the
  control port (`1c44` = 7236) and 200 Mbit/s (`00c8`)
- `device type : source  session available: true  TDLS preferred: false`

Pass: the element reads back from `wpa_supplicant`, not just from our
in-memory copy. If `WFD_SUBELEM_SET` is refused, record the supplicant
version and the error; this is the path NetworkManager cannot provide.

## 3. Discovery finds the sink

```console
$ ./target/release/aethercastd discover --iface "$IFACE" --duration 30
```

Expected when the sink advertises: `N peer(s) found`, and for the sink
`wfd role : primary sink`, `control port: 7236`, a session state, and a
maximum rate.

The search is event-driven: a 10-second search returns in about 10 seconds,
not 15. If it takes the full duration plus five seconds, the control
connection is not receiving `P2P-FIND-STOPPED`/`P2P-DEVICE-FOUND` events; the
daemon now reports `the control connection is not attached for events`
instead of an empty list.

Expected when it does not: the command says whether the radio is wedged,
whether the adapter is band-limited, and that the sink must be in Miracast
mode. Pass for this step means **the output explains the result**; record
which explanation appeared.

If the sink is missing, a capture is the next step. The monitor-mode
scripts from the research phase are in `spike/` (`10-monitor-sweep.sh`,
`16-monitor-sweep-v2.sh`); they need root and a monitor-capable adapter.
Redact MAC addresses and SSIDs before attaching anything to an issue.

## 4. A Wi-Fi Direct group forms

Two shapes matter, and they are different:

```console
# The source listens; the sink initiates the connection (the Miracast model).
$ ./target/release/aethercastd p2p listen --iface "$IFACE" --seconds 120
```

Expected: `listening: a sink can now find us`, then, when the sink connects,
`a sink joined on p2p-wlan0-0 (192.168.49.1)` and the address to serve on.

```console
# The source becomes Group Owner and waits for the sink to join.
$ ./target/release/aethercastd p2p group --iface "$IFACE" --freq 2412 --wait 60
```

Expected: `group started`, the `p2p-wlan0-0` interface, its address, and
`the RTSP server must listen on <address>:7236`. On a single-radio adapter
the group must share the station's channel, which is why `--freq` exists.

Pass: a group interface exists **and has an IPv4 address**. An interface
without an address means no peer has joined yet.

## 5. The WFD handshake against the real sink

With the group up and the sink connected:

```console
$ ./target/release/aethercastd rtsp serve --iface "$IFACE" --port 7236 --seconds 120
```

Expected: the log walks M1 through M7 (M1 `OPTIONS`, M2 `200 OK`, M3
capabilities, M4 format selection, M5 `SETUP` trigger, M6 transport, M7
`PLAY`), then `handshake reached Playing` and the sink's capabilities.

Pass: `Playing`, with a sane `rtp_port0`. If the sink refuses M4, record the
formats it advertised; that is a compatibility finding, not a bug in the
handshake.

## 6. A cast to a physical sink

```console
$ ./target/release/aethercastd cast --iface "$IFACE" --group --seconds 60
```

Expected: the group comes up, the sink connects, the handshake reaches
`Playing`, `streaming to <sink-ip>:<rtp-port>` appears, and the sink shows
the screen for the duration.

Then audio, using the monitor of the default output
(`pactl list short sources` shows it):

```console
$ ./target/release/aethercastd cast --iface "$IFACE" --group --seconds 60 \
      --audio --audio-device <monitor-name>
```

Pass: video is visible, audio is audible, and the two are in sync. Record
the sink model and any stutter; a cast that works but drops frames is still
a pass for this step, with a note.

## 7. The client

In two terminals:

```console
$ ./target/release/aethercastd daemon
$ ./target/release/aethercast
```

Expected:

- the sink appears in the list;
- **Cast** creates a session and its state changes without restarting the
  window (the daemon now emits `SessionStateChanged`);
- with no sink visible, the window explains *why* (radio wedged, adapter
  band limit, or no advertisement) instead of showing an empty list;
- stopping the daemon makes the window say it is not answering, and it
  reconnects when the daemon returns.

Pass: the empty state names a real cause, and the window never freezes while
the daemon is slow or gone.

## 8. Media regression on this machine

These do not need a sink and should pass anywhere the plugins are installed:

```console
$ TEST_PATTERN=1 NO_AUDIO=1 ./spike/19-end-to-end.sh 10
$ sudo NO_AUDIO=1 TEST_PATTERN=1 ./spike/04-media-conformance.sh
```

Expected: the full M1–M7 handshake against the reference sink, a transport
stream `ffprobe` accepts, video PID `0x1011`, RTP payload type 33, 1336-byte
datagrams, and a largest PCR gap at or below 100 ms.

## 9. 720p (known open question)

```console
$ ./target/release/aethercastd media test --test-pattern \
      --width 1280 --height 720 --seconds 10 --out /tmp/opencode/720p.ts
```

Expected on a software-only encoder: the pipeline runs, but 720p30 may be
below realtime (OpenH264 measured 0.32–0.81× on the reference CPU). Record
the encoder, the CPU, and whether the output sustained the frame rate.

Pass is **not** "it worked": the point is to find out whether this machine
can sustain 720p, and to record the answer.

## What to do with the results

- Add a row to [`compatibility.md`](compatibility.md) with the adapter, the
  driver, the sink, the result, and a link to the evidence.
- Open an issue for every failure, with the command, the output, and the
  versions (`uname -r`, `wpa_supplicant -v`, `gst-inspect-1.0 --version`).
- Redact SSIDs, MAC addresses, and private addresses before pasting logs.
- Mark any pull request that needs one of these checks as **needs hardware
  test** and leave it unmerged until the check has been done.
