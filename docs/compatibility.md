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
| AltoBeam-based TV | Miracast sink | Untested | Existing GND 0.99.0 discovers no sinks on the test laptop |
| QCA9565 / AR9565 | Wi-Fi source | Known constraint | 2.4 GHz only; single radio; `#channels <= 1` |

Add new rows only with reproducible evidence.
