# WFD Sink Behaviour, as Observed

**Scope:** one device, examined on 2026-10-21. This is an observation, not a
specification, and not a claim about Miracast sinks in general. The full
evidence — binaries, strings, logcat, captures — is in
[`spike/18-sink-analysis.md`](../../spike/18-sink-analysis.md).

| Field | Value |
| --- | --- |
| Device | CVTE / Hisilicon `Hi3751V350_DMO`, model `SMART_TV` |
| Software | Android 9 (SDK 28), hardware `bigfish` |
| Wireless | ATBM chip, 2.4 GHz only, associated at 2412 MHz, 65 Mbps |
| Role | Miracast sink, vendor application `com.hisilicon.miracast` |

## It speaks standard Wi-Fi Display

The sink's own `libhiwfd.so` contains the WFA RTSP profile, not a
proprietary one:

```text
Public: org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER
Require: org.wfa.wfd1.0
OPTIONS * RTSP/1.0
SETUP %s RTSP/1.0
PLAY %s RTSP/1.0
Session: %s
Content-Type: text/parameters
client-port
```

Port constants found in the binaries: 7236 (the WFD control port), 7250
(the MS-MICE port), and 7237. The vendor libraries contain no WFA OUI
(`50:6F:9A`): the information element is built by the platform supplicant,
not by the vendor code.

## Why this box is hard to find

Observations from the device:

1. **The driver refuses to change channel.** The ATBM driver logs
   `atbm_config:channel not chage`, so a Wi-Fi Direct listen cannot move to a
   listening channel of its choosing.
2. **The application has no configuration.** `WifiBusiness` logs
   `no cfg saved`. The APK ships a first-run setup wizard
   (`WelcomeActivity`, `step2.png`–`step4.png`) that has never been completed.
3. **No P2P, WFD or vendor element was seen from it in the capture.** While
   the screen-share screen was up, the box was off the LAN and the captured
   probe requests carried only SSID, supported rates, extended rates and HT
   capabilities.

On their own these are reasons to expect discovery to be difficult. They are
not proof that the box never advertises: observation 3 covers one capture
window on the captured channel, and a Wi-Fi Direct device spends most of its
time on a listening channel that the window may not have covered.

## Correction (2026-10-22)

The first version of this note concluded that "a device that advertises
nothing cannot be found by any source, which is why Windows, Android, GNOME
Network Displays, and AetherCast all fail to see it."

That conclusion was not supported by the evidence, and it is withdrawn. At
the time of the live test, AetherCast's own discovery path was broken in two
independent ways: the control connection never sent `ATTACH`, so
`wpa_supplicant` never delivered `P2P-DEVICE-FOUND` to it, and the parser
matched the C macro name `P2P_EVENT_DEVICE_FOUND` instead of the wire name
`P2P-DEVICE-FOUND`. Its "no peers" result was therefore not evidence about
the sink at all. Both bugs are fixed and covered by tests; see the
2026-10-22 journal entry.

Whether this TV can be discovered by a correct source is an open question
again. It is to be settled by the checklist and a known-good source, not by
this note.

## Hypotheses this rules out

| Hypothesis | Verdict |
| --- | --- |
| The sink is on 5 GHz | Dead — the chip is 2.4 GHz only and associated at 2412 MHz |
| The sink speaks a proprietary protocol | Dead — it is standard `org.wfa.wfd1.0` |
| Our WFD element is malformed | Dead — it reads identically under every candidate layout |
| Our radio is broken | Dead — the queue is clean, searches complete, captures work |
| Our discovery code is correct | **Dead** — it had no `ATTACH` and matched macro names (fixed 2026-10-22) |
| The sink is a good reference sink | Not established — no known-good source has found it |

## What it means for AetherCast

- This device is not a reference implementation and not a reason to change
  the protocol code. The standard implementation is correct.
- "The sink is on the network but no Wi-Fi Display element was observed" is a
  real diagnostic state. It is weaker than "the sink advertises nothing": it
  describes what one capture saw, not what the device does on every channel.
  The diagnostics should be able to name it alongside a wedged radio-work
  queue and a band-limited adapter.
- The compatibility matrix records the device as **unproven as a sink**, with
  the evidence, rather than as "unsupported" or "cannot be found".

## Reproducing the observation

See the *Reproducing* section of
[`spike/18-sink-analysis.md`](../../spike/18-sink-analysis.md) for the `adb`
pulls, `strings` searches, and port scans. Do not add new devices to the
compatibility matrix without the same kind of evidence.
