# Compatibility Matrix

**Status: Research phase.** This matrix will be filled from reproducible tests, not manufacturer claims.

## Planned columns

| Field | Why it matters |
| --- | --- |
| Wi-Fi adapter / chipset | P2P and channel-concurrency support is driver-specific |
| Driver and kernel | Behavior changes between releases |
| Station + P2P concurrency | Single-radio laptops may need a different strategy |
| Supported bands/channels | 2.4 GHz vs 5 GHz and DFS affect discovery |
| Desktop/session | X11 vs Wayland changes capture and portal requirements |
| Sink make/model | Miracast implementations vary widely |
| WFD version/features | HDCP, UIBC, codec, and resolution support |
| Result | Works, works with caveats, discovery only, fails |
| Evidence | Issue, log, capture, or test report |

## Test status

| Device | Role | Status | Notes |
| --- | --- | --- | --- |
| AltoBeam-based Android TV (`android_b313`) | Miracast sink | **Unreachable** | In Miracast mode it leaves the LAN and is not heard on 2.4 GHz channels 1/6/11. Most likely a 5 GHz listener, which the reference adapter cannot see |
| QCA9565 / AR9565 | Wi-Fi source | **Known constraint** | 2.4 GHz only; single radio; `#channels <= 1` |

## The hardware requirement this implies

A Wi-Fi Direct sink chooses its own listen channel, and a sink that
listens on 5 GHz is invisible to a 2.4 GHz-only adapter. The reference
adapter is Band 1 only:

```
$ aethercastd doctor
adapter          : phy0 (2.4 GHz)
  limitation     : this adapter is 2.4 GHz only (2.4 GHz), so a sink that
                   advertises on 5 GHz cannot be seen
```

**A dual-band adapter is required to cast to sinks that listen on
5 GHz.** AetherCast detects this and says so, rather than showing an
empty list — which is the difference between a tool and a product.

Until such an adapter is available, a sink that can be validated against
must advertise on 2.4 GHz: a Miracast dongle with a 2.4 GHz radio, an
Android device in receiver mode, or the mock sink in `spike/06-mock-sink.py`.

Add new rows only with reproducible evidence.
