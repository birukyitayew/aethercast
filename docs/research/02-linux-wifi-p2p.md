> **Naming note.** "AetherCast" is the project name. It collides with prior art in this niche (`ubports/aethercast`, Canonical's 2015 Ubuntu Miracast project). That is a known and accepted risk with an explicit rename trigger recorded in [ADR-0004](../adr/0004-name-aethercast.md).

# 02 — Wi-Fi Direct / P2P on Linux for a Miracast Source

**Status:** Research complete, pre-implementation. **Date:** 2026-10-20.
**Scope:** Wi-Fi Direct (P2P) as the transport for a Linux Miracast source: wpa_supplicant, NetworkManager, cfg80211/mac80211 and driver constraints, discovery behavior, permissions and packaging, and a recommended architecture for AetherCast. WFD media and RTSP details belong to tracks 01/03; this note stops at a working P2P group and the first TCP connection.

**Test baseline.** Kali Linux 2026.2, kernel 6.19.14+kali-amd64, Qualcomm Atheros QCA9565/AR9565 (`ath9k`, PCI `168c:0036`), 2.4 GHz only, associated to AP `Biruk` on channel 1, wpa_supplicant 2.10, NetworkManager 1.56.1, GNOME Network Displays (GND) 0.99.0. The TV is an AltoBeam-based Miracast sink that GND did not discover in this environment ([JOURNAL](../../JOURNAL.md); see [05 — Product and Architecture](05-product-architecture.md)).

**Evidence labels.** **Verified** = read in a primary source (specification, kernel or application source, official documentation) or reproduced on the test machine. **Reported** = stated by a third party, not independently reproduced. **Speculative** = inference, forecast, or recommendation. Local command output is labelled *verified locally* and is reproducible with the commands in §11.

---

## 1. Executive summary

- **P2P is the right transport; the radio is the binding constraint.** The default `ath9k` combination allows a managed station and one P2P client/GO on **one channel at a time** (verified locally via `iw list`; [`ath9k/init.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath9k/init.c#L851-L878)). Discovery works off-channel via `remain_on_channel`; a P2P group cannot run on a second channel while the station is associated.
- **The second constraint is the API.** NetworkManager exposes discovery as `StartFind` with a timeout only, peers with no operating-channel property, and a `wifi-p2p` setting limited to `peer`, `wfd-ies`, `wps-method` (verified: [NM D-Bus docs](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.WifiP2P.html), [NM settings](https://networkmanager.dev/docs/api/latest/settings-wifi-p2p.html), [libnm peer](https://networkmanager.dev/docs/libnm/latest/NMWifiP2PPeer.html)). Its internal `Connect` hard-codes `join=false`, `persistent=false`, `go_intent=7` (verified: [NM source](https://gitlab.freedesktop.org/NetworkManager/NetworkManager/-/blob/main/src/core/supplicant/nm-supplicant-interface.c#L3072-L3095)).
- **GND’s discovery loop is actively harmful on a busy single-channel radio.** It calls `StartFind` every 20 s with no timeout while NM’s default find timeout is 30 s; the overlap leaves a scan pending and wpa_supplicant rejects the next trigger (verified: [GND source](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/0.99.0/src/nd-wfd-p2p-provider.c#L190-L208), [NM docs](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.WifiP2P.html); local logs show repeated `P2P: Reject scan trigger since one is already pending` then `call-p2p-find: failed with Could not start P2P find`).
- **“Channel unavailability” is deterministic.** With no unused channel, a P2P group forced onto a frequency not already in use makes `wpas_p2p_setup_freqs()` return `-2`, surfaced as `connect failed due to channel unavailability` (verified in wpa_supplicant 2.10, `p2p_supplicant.c:5740-5748`, `dbus/dbus_new_handlers_p2p.c:675-678`, tarball at <https://deb.debian.org/debian/pool/main/w/wpa/wpa_2.10.orig.tar.xz>). Retrying the same connection is pointless.
- **Bottom line:** expect co-channel operation or no simultaneous Wi-Fi; a dual-channel adapter is the only comfortable fix, and the UI should say so rather than promise “any Miracast TV.”

---

## 2. The protocol, and the channels it uses

Wi-Fi Direct lets two devices form a group with no access point: one becomes the **Group Owner (GO)**, the other a **Client**; the GO runs DHCP and the client gets an address (verified in the [Wi-Fi Display Technical Specification v2.1](https://www.wi-fi.org/discover-wi-fi/miracast) §3.2, §4.5; the PDF is member-gated).

| Concept | Meaning | Consequence for AetherCast | Basis |
| --- | --- | --- | --- |
| P2P Device | Discovery/management role; a dedicated netdev or the station interface | Discovery is separate from the data path | Verified (wpa_supplicant `README-P2P`) |
| GO | Group owner: beaconing, DHCP, channel owner | The GO’s channel *is* the group’s channel | Verified (WFD spec §3.2) |
| Client | Joins the GO’s group | A client cannot move the group’s channel | Verified (WFD spec §3.2, §4.5.2) |
| WFD Source / Sink | Source sends A/V and runs the RTSP server; sink receives and initiates TCP | AetherCast is the source | Verified (WFD spec §4.5.4) |
| WFD IE | Vendor IE (OUI `50-6F-9A`, type `0x0A`) with WFD subelements | Without it a peer is not identifiable as Miracast | Verified (WFD spec §5.1.1) |

**Discovery.** WFD discovery builds on P2P discovery: the source scans, sends probe requests with the P2P wildcard SSID and WFD IE, and listens on a social channel; sinks carry the WFD IE in beacons and probe responses (verified in WFD spec §4.3, §5.1.1). No discovery channel is fixed by the spec; in practice wpa_supplicant listens on social channels 1, 6, 11 and starts `P2P_FIND_START_WITH_FULL` as a scan of all supported channels (verified in `p2p_supplicant.c:394-500`; `src/p2p/p2p.c:1158-1250`).

**Group formation.** The peers exchange GO Negotiation frames carrying a GO Intent (0–15); higher wins, ties broken by a tie-breaker bit. Provisioning then uses WPS PBC or PIN, then the 4-way handshake and DHCP (verified in `README-P2P:145-190`; WFD spec §4.5.2). A source that wants to control the channel must *win* GO Negotiation; a loser becomes a client on the winner’s channel. For a session with one sink, “either a WFD Source or a WFD Sink may be a P2P GO” (verified in WFD spec §3.2.1) — so a correct source cannot assume the TV is the GO, and GO intent is a real interoperability variable.

**After the group.** The sink initiates TCP to the source’s RTSP server; default port 7236, or a private port in 49152–65535 (verified in WFD spec §4.5.4). GND advertises exactly 7236 and 200 Mbps in its WFD IE (decoded in §9).

### 2.1 Which wpa_supplicant controls matter for a source

| Command / knob | Effect for a Miracast source | Use it? |
| --- | --- | --- |
| `p2p_find [timeout]`, `p2p_listen [timeout]` | Scan + listen cycle discovering peers and their WFD IEs; listen-only cycle on our listen channel | **Yes** — the discovery primitive; `p2p_listen` only to be discoverable by a sink that initiates discovery |
| `p2p_connect <addr> pbc\|PIN [go_intent=] [freq=]` | GO Negotiation + provisioning + group formation | **Yes** — the connect primitive; `go_intent`/`freq` decide everything |
| `p2p_group_add [freq=]` | Create a local GO on a chosen channel | As a fallback when a sink will join but will not negotiate |
| `p2p_no_group_iface=1` | Run the group on the main interface instead of a new netdev | **No** — it would displace the station connection |
| `p2p_oper_channel`, `p2p_listen_reg_class`, `p2p_go_intent` | Preferred GO channel, listen channel, default GO intent | Only as preconfigured defaults (§8) |

### 2.2 Listen channels: how a sink is found

A sink is a P2P device: it picks its own listen channel (normally a social channel 1/6/11), advertises itself in probe responses and beacons with a WFD IE, and if it is already a GO it also beacons on its group channel (verified in WFD spec §4.3, §5.1.1; `p2p_supplicant.c:4848-4885`). Three consequences: the source need not sit on the sink’s channel to find it, because `p2p_find` scans all supported channels (our listen channel only matters for sink-initiated discovery); the source need not leave the AP channel during discovery, because off-channel work uses `remain_on_channel` (ROC) and the association is maintained (verified locally: ROC is advertised); and the group must end up on one channel — on a one-channel radio, the station’s channel, which is where `-2` comes from (§5).


## 3. The Linux stack

| Layer | Owns | Relevant behavior | Basis |
| --- | --- | --- | --- |
| `cfg80211` / `mac80211` | Interface combinations, channel contexts, ROC | Declares how many channels and interfaces a wiphy can run at once | Verified ([`cfg80211.h`](https://github.com/torvalds/linux/blob/master/include/net/cfg80211.h#L5816-L5826)) |
| Driver (`ath9k`, `iwlwifi`, …) | Firmware/hardware limits | Sets `num_different_channels`, P2P interface types, DFS rules | Verified (driver sources, §6) |
| wpa_supplicant | P2P state machine, GO negotiation, WPS, group interface | Derives the channel budget from driver capabilities, picks channels, exposes D-Bus and a control socket | Verified (wpa_supplicant 2.10 source) |
| NetworkManager | Device/connection lifecycle, D-Bus API, polkit | Owns `wlan0`, proxies P2P to the supplicant, exposes `p2p-dev-*` objects. Its P2P path is supplicant-based; iwd is unsupported for P2P | Verified ([NM P2P docs](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.WifiP2P.html)); iwd limitation reported ([GND README](https://gitlab.gnome.org/GNOME/gnome-network-displays#debugging)) |
| GND / AetherCast | Policy, UI, WFD session | Decides when to find, which peer to connect, which WFD IEs to advertise | Verified ([GND source](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/0.99.0/src/nd-wfd-p2p-provider.c)) |

### 3.1 The dedicated P2P device is a driver feature, not an API call

wpa_supplicant 2.10 has no `p2p_add_device` command: a separate `P2P-device` netdev appears only when the driver advertises `WPA_DRIVER_FLAGS_DEDICATED_P2P_DEVICE` **and** the configuration points at a P2P-device config file; otherwise the main interface doubles as the P2P management interface (verified: wpa_supplicant 2.10 `wpa_supplicant/wpa_supplicant.c:7141-7143`, `src/drivers/driver_nl80211_capa.c:1116-1124`). This AR9565 advertises P2P-client/GO but no P2P-device mode, so the supplicant manages P2P on `wlan0` itself (verified locally: `iw dev` shows only `wlan0`, yet `fi.w1.wpa_supplicant1.Interface.P2PDevice` is exposed on `Interfaces/0`).

GND’s advice that `iw dev` must show a `P2P-device` interface, “otherwise the device is likely unsupported” (reported: [GND README](https://gitlab.gnome.org/GNOME/gnome-network-displays#debugging)), is too strict: it detects a design, not P2P capability.

### 3.2 Who owns the radio

NetworkManager owns `wlan0` and the station connection; wpa_supplicant owns the P2P state machine behind it; NM exposes a `p2p-dev-wlan0` object even when no P2P netdev exists, and the group, once formed, appears as `p2p-wlan0-0` with `NoGroupIface` false (verified locally via `nmcli device status` and D-Bus `P2PDeviceConfig`).

Two managers on one radio produce scan wars and state races. MiracleCast takes the opposite extreme — stop NetworkManager and wpa_supplicant, spawn a private supplicant (reported: [MiracleCast README](https://github.com/albfan/miraclecast#readme)) — fine for an appliance, hostile to a desktop session. Direct supplicant calls behind NM’s back are possible but unsupported and easy to desynchronize; they belong in a deliberate helper path.

---

## 4. This machine, measured

| Fact | Value | Basis |
| --- | --- | --- |
| Adapter | QCA9565/AR9565, PCI `168c:0036`, `ath9k`; 2.4 GHz only (Band 1) | Verified locally (`lspci -nn`, `iw list`) |
| Interface modes | IBSS, managed, AP, AP/VLAN, monitor, mesh point, P2P-client, P2P-GO; **no P2P-device** | Verified locally (`iw list`) |
| Combination | `#{ managed } <= 2048, #{ AP, mesh point } <= 8, #{ P2P-client, P2P-GO } <= 1, total <= 2048, #channels <= 1, STA/AP BI must match` | Verified locally (`iw list`) |
| Off-channel work | `remain_on_channel` supported | Verified locally (`iw list`) |
| Module parameter | `/sys/module/ath9k/parameters/use_chanctx` = `0`; mode `0444` (read-only at runtime) | Verified locally; [`ath9k/init.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath9k/init.c#L72-L74) |
| Supplicant | wpa_supplicant 2.10; capabilities include `p2p` | Verified locally (`wpa_supplicant -v`; D-Bus `Capabilities`) |
| NetworkManager | 1.56.1; `wlan0` connected to `Biruk` (ch1); `p2p-dev-wlan0` disconnected | Verified locally (`nmcli device status`) |
| Regulatory | `CN`, phy `99` (`DFS-UNSET`); 2.4 GHz 2402–2472 | Verified locally (`iw reg get`) |

**The supplicant’s P2P defaults mean “let the stack decide.”** A live `Get` of `P2PDeviceConfig` returned `GOIntent=7`, `ListenRegClass=0`, `ListenChannel=0`, `OperRegClass=0`, `OperChannel=0`, `NoGroupIface=false`, `p2p_search_delay=500` (verified locally). Zero listen/operating channel means a random social channel is chosen; intent 7 loses to any peer with intent 8–15 and ties break randomly (verified: `p2p_supplicant.c:4848-4885`; `README-P2P:153-190`).

**Observed failure signature.** During GND runs the journal repeatedly shows the supplicant rejecting a P2P scan because another is pending, immediately followed by NM failing the find (verified locally, §11): a scan-scheduling failure, not evidence that no sink is nearby — a distinction GND never surfaces.

---

## 5. Channel concurrency: the core constraint

`cfg80211` describes each interface combination with `num_different_channels`: “can use up to this many different channels” (verified: [`cfg80211.h`](https://github.com/torvalds/linux/blob/master/include/net/cfg80211.h#L5816-L5826)). mac80211 enforces the combination when interfaces are added or channel contexts are assigned, so the driver’s table — not the chip’s marketing name — is the first thing to read. wpa_supplicant turns the driver’s `NL80211_IFACE_COMB_NUM_CHANNELS` into `wpa_s->num_multichan_concurrent`, but only for the first combination containing both a managed station and a P2P GO/client, and only if the value exceeds one (verified: `src/drivers/driver_nl80211_capa.c:180-200, 1116-1121`; `wpa_supplicant/wpa_supplicant.c:6727, 6755-6756`).

| Step | Value | Basis |
| --- | --- | --- |
| Driver combination with STA + P2P → `num_multichan_concurrent` | `#channels <= 1` → 1 | Verified locally; [`ath9k/init.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath9k/init.c#L851-L878); derived from the verified code path |
| STA associated on ch1 | 1 frequency in use | Verified locally |
| `wpas_p2p_num_unused_channels()` | 1 − 1 = 0 | Verified: `p2p_supplicant.c:182-196` |
| Forced P2P frequency ≠ 2412 | `num_unused <= 0 && !freq_in_use` → `-2` | Verified: `p2p_supplicant.c:5680-5750` |
| D-Bus error | `connect failed due to channel unavailability` | Verified: `dbus_new_handlers_p2p.c:675-678` |

```c
if (num_unused <= 0 && !freq_in_use) {
    wpa_printf(MSG_DEBUG, "P2P: Cannot start P2P group on %u MHz as there are no available channels", freq);
    res = -2;
    goto exit_free;
}
```

That is the mechanism behind GND issue #296, where an rtw89 adapter reports the same error with a Galaxy Tab as the display (reported: [GND #296](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/296)). It is not rtw89-specific: any one-channel combination with an active station can reach the same branch.

### 5.1 What the AR9565 can realistically do

| Question | Answer | Basis |
| --- | --- | --- |
| Discover a Miracast sink while associated to `Biruk`? | **Yes.** `p2p_find` scans all 2.4 GHz channels and uses ROC to listen on a social channel; the AP association survives | Verified locally (ROC advertised); `p2p_supplicant.c:394-500` |
| Switch channels briefly without dropping the AP? | **Yes.** ROC tunes away and back; association and BSSID are maintained, with a short latency/throughput dip | Verified locally; timing detail speculative |
| What does `p2p_find` do on 1/6/11? | Full scan of all supported 2.4 GHz channels, then repeated social-channel scans plus listen cycles on a randomly chosen social channel. Channels 12/13 are not social channels and are not P2P operating channels by default | Verified: `p2p_supplicant.c:415, 449-500`; `src/p2p/p2p.c` |
| Keep Wi-Fi *and* a P2P group? | Only **co-channel**: the group is forced onto the station’s channel (`freq_in_use` bypasses `-2`), or a different channel fails with `-2` | Verified: §5 table; `p2p_supplicant.c:5754-5825` |
| Run a P2P group on any channel with the station disconnected? | **Yes.** With no station frequency in use, `num_unused` becomes 1 | Verified by code; end-to-end untested here |

**Caveat.** The `-2` branch is exact for a *forced* frequency not in use; with no forced frequency wpa_supplicant forces the station’s frequency instead, so some negotiations fail with a GO-negotiation timeout instead (verified: `p2p_supplicant.c:3033-3070, 5754-5825`). The invariant is unchanged — no second channel — so “no `-2`” is not proof of a two-channel session.

---

## 6. Driver landscape

Source-code survey, not a compatibility list. “Channels” is the value in the combination containing station + P2P.

| Driver / family | Channels | P2P interface types | Notes | Basis |
| --- | --- | --- | --- | --- |
| `ath9k` (AR9565 here) | 1 by default; 2 with `use_chanctx=1` | P2P-client/GO; P2P-device only in the 2-channel table | Default table: `max_interfaces=2048`, one channel. `if_comb_multi`: 2 channels, 3 interfaces, P2P-device — selected only when the module parameter is on, and it replaces the default table | Verified ([`init.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath9k/init.c#L840-L900)) |
| `ath10k` | 1 (some QCA QCS firmware advertise 2) | P2P-client/GO/device unless firmware sets `NO_P2P` | Two-channel table is a firmware variant | Verified ([`mac.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath10k/mac.c#L9630-L9790)) |
| `ath11k` | 1; 2 only with `support_dual_stations` | P2P-client/GO/device if firmware exposes P2P_DEVICE | Up to 16 interfaces; dual-station is a firmware capability, not a userspace switch | Verified ([`mac.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath11k/mac.c#L10296-L10365)) |
| `iwlwifi` (MVM) | 2 | P2P-client/GO/device | 3 interfaces in the two-channel combination — the most P2P-friendly stack here | Verified ([`mac80211.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/intel/iwlwifi/mvm/mac80211.c#L43-L70)) |
| `rtw88` | 1 | **None in mainline** | Mainline advertises only STATION, AP, ADHOC and a 1-channel STA+AP combination; P2P needs a vendor driver | Verified ([`main.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw88/main.c#L106-L126), [modes](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw88/main.c#L2286-L2288)) |
| `rtw89` | 1 or 2 depending on chip/chanctx | P2P-client/GO/device | `n_iface_combinations` drops to one when the chip has a single channel context; the two-channel table is STA + one P2P/AP role | Verified ([`core.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw89/core.c#L168-L208), [selection](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw89/core.c#L7690-L7698)) |
| `brcmfmac` | 1, or 2 with RSDB/MCHAN | P2P-client/GO/device if firmware feature `P2P` | Channels = `1 + (rsdb \|\| (p2p && mchan))` | Verified ([`cfg80211.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/broadcom/brcm80211/brcmfmac/cfg80211.c#L7499-L7540)) |
| `mt76` | Not set by the driver | P2P-client/GO (no P2P-device) | No driver-specific combination table, so the effective channel budget is the kernel default; verify per device with `iw list` | Verified for modes ([`mac80211.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/mediatek/mt76/mac80211.c#L537-L545)); channels not verified |

“Supports P2P” ≠ “supports P2P while associated on another channel”, and the same chip family differs by firmware and kernel config — so a compatibility list must be built from `iw list` on the actual machine.

The `ath9k` two-channel mode is a documented workaround, not a default: MiracleCast issue #75 describes enabling the kernel flag, building the channel-context driver and setting `options ath9k use_chanctx=1`, then reloading the module (reported: [MiracleCast #75](https://github.com/albfan/miraclecast/issues/75), pointing at the [ath9k-devel thread on multi-role support](http://ath9k-devel.ath9k.narkive.com/ehPhw9BD/wi-fi-direct-multi-role-support-in-ath9k) and an [AskUbuntu write-up](http://askubuntu.com/a/425205/89479)). It is the only known way to expose `#channels <= 2` and a P2P-device on this AR9565, but it changes the combination table and is unvalidated here for stability or throughput.


## 7. NetworkManager’s P2P API, and the alternative

NetworkManager has exposed Wi-Fi P2P since 1.16. The surface is deliberately small.

| Object / method | Exposes | Does not expose | Basis |
| --- | --- | --- | --- |
| `Device.WifiP2P.StartFind(a{sv})` | `timeout` only, 1–600 s, default 30 s | Discovery type, listen channel, frequency, device types | Verified ([docs](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.WifiP2P.html)) |
| `Device.WifiP2P.StopFind()`, `Device.WifiP2P.Peers` | Stops a find; lists peer object paths | — | Verified (same) |
| `WifiP2PPeer` | `HwAddress`, `Name`, `Manufacturer`, `Model`, `ModelNumber`, `Serial`, `WfdIes`, `Strength`, `LastSeen` | **No operating channel**, no role, no listen channel | Verified ([libnm peer](https://networkmanager.dev/docs/libnm/latest/NMWifiP2PPeer.html)) |
| `wifi-p2p` setting | `peer`, `wfd-ies`, `wps-method` | GO intent, frequency, join, persistent group, listen channel | Verified ([settings](https://networkmanager.dev/docs/api/latest/settings-wifi-p2p.html)) |
| `Connect` (internal) | Sends `peer`, `wps_method`, `pin`, `join=false`, `persistent=false`, `go_intent=7` | Any caller-selected role or channel | Verified ([source](https://gitlab.freedesktop.org/NetworkManager/NetworkManager/-/blob/main/src/core/supplicant/nm-supplicant-interface.c#L3072-L3095)) |

The settings documentation states that `peer` is “currently … the only way to create or join a group”, and `wfd-ies` is “only useful when implementing a Wi-Fi Display client” (verified: [NM settings](https://networkmanager.dev/docs/api/latest/settings-wifi-p2p.html)). It is a client-oriented API: it can connect to a known peer, but cannot say “be the GO on channel 1” or “join that GO on its channel”.

### 7.1 NM versus direct wpa_supplicant versus hybrid

| Dimension | NM only | Direct wpa_supplicant | Hybrid (recommended) |
| --- | --- | --- | --- |
| Reliability | High: NM serializes scans, connections and device state | Low: races with NM’s scans and activation; NM may cancel a group it did not create | High: NM owns the device; the helper issues one `Connect`/`GroupAdd` with explicit parameters under a single owner |
| Permissions and packaging | Polkit-mediated, no root; Flatpak-clean with `--share=network --system-talk-name=org.freedesktop.NetworkManager` | Control socket is `root:netdev` (group suffices locally); upstream D-Bus policy allows only root; needs a host component or `--system-talk-name=fi.w1.wpa_supplicant1` | Helper as a system unit or `netdev`-group user service; the Flatpak app reaches it over D-Bus and never touches the radio |
| Coexistence with the station | Guaranteed — NM knows the connection exists | Poor — two state machines on one radio | Good, if the helper defers to NM between sessions |

**Recommendation: hybrid.** Use NM for what it does well (device lifecycle, discovery, activation, polkit) and add one narrow helper that can express GO intent and operating frequency when the radio’s channel budget demands it. Do not fork NetworkManager.

### 7.2 Permissions and packaging

`StartFind` is authorized with `NM_AUTH_PERMISSION_WIFI_SCAN`; connection activation uses `network-control` (verified: [NM source](https://gitlab.freedesktop.org/NetworkManager/NetworkManager/-/blob/main/src/core/devices/wifi/nm-device-wifi-p2p.c#L1370-L1393)). On this Kali install, `/usr/share/polkit-1/actions/org.freedesktop.NetworkManager.policy` sets both to `allow_any=auth_admin`, `allow_inactive=yes`, `allow_active=yes` (verified locally); distro defaults differ, so a remote or locked session can behave differently.

| Operation | Privilege | Verified |
| --- | --- | --- |
| `iw list`, `iw dev`, `iw reg get` | None (read-only netlink) | Verified locally as a non-root user |
| `nmcli` P2P discovery/connection | Polkit: active/inactive local user; `auth_admin` otherwise | Verified locally (policy file) |
| `wpa_cli` (control socket) | `netdev` group (socket `root:netdev`, mode 770) | Verified locally |
| D-Bus to `fi.w1.wpa_supplicant1` | `netdev` group on Debian/Kali; **root only** upstream | Verified locally vs. tarball `dbus/dbus-wpa_supplicant.conf` |
| Changing interface state (monitor mode, channel), module parameters (`options ath9k use_chanctx=1` + module reload, parameter is `0444`), supplicant config | Root / `CAP_NET_ADMIN` / system unit | Reported generally; only read-only commands were exercised here ([`ath9k/init.c`](https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath9k/init.c#L72-L74)) |

Flatpak: GND runs with `--share=network` and `--system-talk-name=org.freedesktop.NetworkManager` (plus Avahi and FirewallD), not requesting the supplicant bus name (verified: [manifest](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json)). Flatpak filters D-Bus by default, recommends minimum `talk-name` permissions, and discourages `--socket=system-bus` (verified: [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)). A systemd *user* service cannot hold `CAP_NET_ADMIN` or read a root-owned socket without group membership, so a helper needing more than `netdev` must be a system unit.

---

## 8. Direct wpa_supplicant control

The supplicant can do everything NM cannot, but its knobs are global or per-interface, not per-connection.

| Knob | Effect | Value here | Basis |
| --- | --- | --- | --- |
| `p2p_go_intent` | 0–15; 15 forces GO, 0 forces client | 7 | Verified (`README-P2P:722`; local D-Bus) |
| `p2p_oper_reg_class` / `p2p_oper_channel` | Preferred/forced GO operating channel | 0/0 (auto) | Verified (`p2p_supplicant.c:4877-4895`) |
| `p2p_listen_reg_class` / `p2p_listen_channel` | Listen channel for discovery | 0/0 (random social) | Verified (`p2p_supplicant.c:4852-4875`) |
| `p2p_search_delay` | Extra delay between search iterations, ms | 500 | Verified (`wpa_supplicant.conf:397-402`; local D-Bus) |
| `p2p_no_group_iface` | Run the group on the main interface | false | Verified (`p2p_supplicant.c:704-741`) |
| `p2p_go_allow_dfs` | Allow GO on DFS channels | false | Verified (`p2p_supplicant.c:3655-3657, 6260-6300`) |
| D-Bus `Connect` keys | `go_intent`, `frequency`, `join`, `persistent`, `wps_method`, `pin` | NM sends 7/false | Verified (`dbus_new_handlers_p2p.c:600-660`) |

Two commands summarize the difference; the control interface accepts the parameters even though `wpa_cli`’s help string for `p2p_connect` is stale (verified: `wpa_supplicant/ctrl_iface.c`, `p2p_ctrl_connect`; `README-P2P:153-190`):

```bash
wpa_cli -i wlan0 p2p_connect <peer-mac> pbc go_intent=15 freq=2412
wpa_cli -i wlan0 p2p_group_add freq=2412
```

The first forces GO negotiation at intent 15 on channel 1; the second creates a local GO on channel 1 for the sink to join. The D-Bus equivalent is `fi.w1.wpa_supplicant1.Interface.P2PDevice.Connect` with `{'go_intent': <int32 15>, 'frequency': <int32 2412>}` (verified: `dbus_new_handlers_p2p.c:615-620`).

**A config file alone is not enough:** `p2p_go_intent=15` is overridden because NM passes `go_intent=7` explicitly on every `Connect` and the D-Bus handler prefers the method argument (verified: `dbus_new_handlers_p2p.c:615-618, 656`). Nor should GO always be forced — a sink may not accept the client role — so forcing GO is a per-sink bet, not a default.

**DFS.** For a GO, wpa_supplicant avoids DFS channels unless `p2p_go_allow_dfs` is set or the driver offloads DFS, and its 5 GHz fallback list is limited to UNII-1 (36–48) and UNII-3 (149–161) (verified: `p2p_supplicant.c:6260-6300`): on a dual-band card, a sink parked on a DFS channel can be unreachable.

---

## 9. Discovery behavior and the GND case study

**What GND does.** It watches NM’s `p2p-dev-*` device, starts `StartFind`, restarts it every 20 s with no timeout argument, ignores peers whose `WfdIes` property is empty (re-adding them if it later changes), and activates a `wifi-p2p` connection with static WFD IEs through NM (verified: [`nd-wfd-p2p-provider.c`](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/0.99.0/src/nd-wfd-p2p-provider.c#L80-L100, L190-L208), [`nd-wfd-p2p-sink.c`](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/0.99.0/src/nd-wfd-p2p-sink.c#L540-L560)).

**Why the 20 s restart is a bug on this radio.** NM’s default find timeout is 30 s (verified: [NM docs](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.WifiP2P.html)), so GND’s second call lands while the first find is still running, and wpa_supplicant rejects a new P2P scan when one is pending (verified: `p2p_supplicant.c:420-426`):

```c
if (wpa_s->p2p_scan_work) {
    wpa_dbg(wpa_s, MSG_INFO, "P2P: Reject scan trigger since one is already pending");
    return -1;
}
```

The D-Bus `Find` handler turns that into `Could not start P2P find` (verified: `dbus_new_handlers_p2p.c:177-180`); the local journal shows that sequence 20–40 s apart (verified locally, §11). This is not cosmetic: a failed `StartFind` means the discovery window never covers the sink’s listen interval, the list stays empty, and the user concludes the TV is incompatible. GND #254 (“Could not start P2P find”) and #84 (NM 1.22 never triggered a P2P scan; fixed by NM MR 399) are the same failure family (reported: [GND #254](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/254), [GND #84](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/84)).

**The WFD IE filter is right but brittle.** Dropping peers with no WFD IE is correct for a Miracast source, but a transiently missing IE hides a real sink; the `notify::wfd-ies` retry is a workaround (verified in `nd-wfd-p2p-provider.c:80-100`). AetherCast should show “P2P peer, no WFD IE” rather than drop it silently.

**The static WFD IE deserves a second look.** GND sends the 9-byte subelement `00 00 06 00 90 1c 44 00 c8` (verified: `nd-wfd-p2p-sink.c:546-558`). Decoded against the WFD subelement format: ID 0 (Device Information), length 6, device information `0x0090`, control port `0x1c44` = 7236, throughput `0x00c8` = 200 Mbps (verified against WFD spec §5.1.2, Table 28). The device-type bits are `0b00` (WFD Source) — correct. But bit 7 is the Preferred Connectivity bit, and `0x0090` sets it to 1, which the spec defines as **TDLS preferred**, not P2P (verified against WFD spec §5.1.2, Table 29); GND contains no TDLS code. Sinks honoring the PC bit may attempt TDLS and fail. A plausible, testable interoperability bug, not a proven cause of the empty list (speculative impact).

---

## 10. Recommendations for AetherCast

**R1. Make the channel budget a first-class, visible fact.** At startup, read `iw list`, find the combination containing both managed and P2P, and compute the same `num_multichan_concurrent` the supplicant will compute. If it is 1 and the station is connected, show “single-channel mode: the TV must use your Wi-Fi channel, or Wi-Fi will be paused.” (Basis: verified code in §5; judgment speculative.)

**R2. Fix discovery before adding features.** One `StartFind` with a long timeout (up to 600 s), restart on `FindStopped` or a deliberate backoff, never on a fixed timer. Treat `Could not start P2P find` as a scheduling error with backoff, not as “no sinks”, and log it beside the supplicant’s `Reject scan trigger` line (basis: verified NM/GND code).

**R3. Adopt the hybrid architecture.** An `aethercastd` daemon owns the P2P lifecycle and exposes a small versioned D-Bus API; a thin library wraps it for the UI. The daemon uses NM for discovery and activation and falls back to a scoped supplicant helper only when GO intent or frequency must be forced. One owner of radio state means the app cannot race itself. (Basis: verified API limits; architecture recommendation, speculative.)

**R4. Map errors to states with a fallback ladder.** Never retry a deterministic error; always offer the next action.

| Observed | Meaning | Next action offered |
| --- | --- | --- |
| `Could not start P2P find` | Scan scheduling collision | Backoff and retry; do not show “no devices” |
| `connect failed due to channel unavailability` (`-2`) | No second channel available | Offer “pause Wi-Fi” or “use a dual-channel adapter” |
| Activation timeout | GO negotiation or WPS stalled | Offer PIN pairing, then “try as GO” |
| Peer with no WFD IE | Not a Miracast sink | Show as unsupported, keep visible |
| Group never reaches IP/DHCP | GO/DHCP failure | Offer `p2p_group_add` on the station channel as fallback |

**R5. Offer two explicit fallbacks, not hidden retries.** (a) *Pause Wi-Fi*: disable the station connection’s `autoconnect`, disconnect it, run the P2P group, then restore — with no station frequency in use, `num_unused` becomes 1 and a group on the sink’s channel is allowed (verified by code; end-to-end untested). (b) *Experimental two-channel mode*: `options ath9k use_chanctx=1` plus a module reload, behind a warning and a one-command rollback.

**R6. Prefer hardware with a real two-channel combination.** A new adapter should show `#channels <= 2` in a combination that includes managed + P2P: `iwlwifi` MVM advertises 2 channels/3 interfaces, some `ath11k`, `rtw89` and `brcmfmac` builds advertise 2 depending on firmware, and mainline `rtw88` advertises no P2P modes. Treat USB adapters as unknown until `iw list` is inspected.

**R7. Package NM-first.** Flatpak with `--share=network` and `--system-talk-name=org.freedesktop.NetworkManager` is sufficient for the NM path, as GND demonstrates. Keep the helper out of the sandbox: expose it over the session or system bus under its own name rather than granting the sandbox raw supplicant control. Never request `--socket=system-bus`.

---

## 11. Test plan and diagnostics

### 11.1 Commands that answer the important questions

```bash
# 1. Concurrency limits; 2. NM's view of the device
iw list | sed -n '/valid interface combinations/,+4p'
iw dev
nmcli device status
nmcli -f GENERAL.DRIVER,GENERAL.STATE,GENERAL.DBUS-PATH device show p2p-dev-wlan0

# 3. The supplicant's P2P defaults
gdbus call --system --dest fi.w1.wpa_supplicant1 \
  --object-path /fi/w1/wpa_supplicant1/Interfaces/0 \
  --method org.freedesktop.DBus.Properties.Get \
  fi.w1.wpa_supplicant1.Interface.P2PDevice P2PDeviceConfig

# 4. A long find through NM (600 s), using the real object path
path=$(nmcli -g GENERAL.DBUS-PATH device show p2p-dev-wlan0)
gdbus call --system --dest org.freedesktop.NetworkManager \
  --object-path "$path" \
  --method org.freedesktop.NetworkManager.Device.WifiP2P.StartFind \
  "{'timeout': <int32 600>}"

# 5. Peers (only when AetherCast/GND is not driving P2P)
wpa_cli -i wlan0 p2p_peers
wpa_cli -i wlan0 p2p_peer <peer-mac>

# 6. Correlate scan scheduling with the failure
journalctl -b -u wpa_supplicant -u NetworkManager --no-pager | \
  grep -E "P2P|p2p|CTRL|channel"

# 7. Group traffic, once a group exists
tcpdump -i p2p-wlan0-0 -w /tmp/p2p.pcap
```

Local interpretation (verified locally on 2026-10-20): `#channels <= 1` means no simultaneous second channel; `GOIntent=7` with zero listen/operating channel means “let the stack pick”; repeated `Reject scan trigger` plus `Could not start P2P find` means the discovery cadence is broken, not that the TV is absent. Journal excerpt from this machine:

```
wpa_supplicant[771]: wlan0: P2P: Reject scan trigger since one is already pending
NetworkManager[706]: sup-iface[…,0,wlan0]: call-p2p-find: failed with Could not start P2P find
```

### 11.2 Compatibility test matrix

| # | Station | Sink / role | Radio mode | Expected | Why |
| --- | --- | --- | --- | --- | --- |
| 1 | disconnected | GO on ch6 | default | Connect; group on ch6 | No station frequency in use; `num_unused=1` |
| 2 | ch1 | GO on ch1 | default | Connect; co-channel group | `freq_in_use=1` bypasses `-2` |
| 3 | ch1 | GO on ch6 | default | Fail with `-2`, or forced back to ch1 | One-channel combination |
| 4 | ch1 | accepts client | source GO on ch1 (`go_intent=15`) | Connect if the sink accepts the client role | Forced GO + frequency |
| 5 | ch1 | GO on ch6 | `use_chanctx=1` | Connect on two channels if stable | `if_comb_multi`: 2 channels |
| 6 | ch36 (5 GHz) | GO on ch6 | 2-channel adapter (e.g. `iwlwifi` MVM) | Connect if the driver advertises 2 channels; a DFS-channel sink may be unreachable | `num_different_channels=2`; `p2p_go_allow_dfs=0` |

Rows 1–3 and 5 decide the product story on the current laptop; rows 4 and 6 decide whether a helper or a hardware recommendation is the better investment.

### 11.3 Bug-report bundle and caveats

Capture `iw list`’s combination, `iw dev`, `nmcli device status`, `P2PDeviceConfig`, NM and supplicant versions, the exact NM error string, the peer’s WFD IE bytes, the station channel and whether the sink was in pairing mode; redact MACs and SSIDs. GND documents the `tcpdump` plus `G_MESSAGES_DEBUG=all` workflow (reported: [GND README](https://gitlab.gnome.org/GNOME/gnome-network-displays#debugging)); AetherCast should emit this bundle automatically. Monitor-mode tools such as `airmon-ng` can tear down managed interfaces, so do not run them during a P2P test; only read-only `iw` queries were exercised here as a non-root user.

Caveats: monitor-mode tools such as `airmon-ng` can tear down managed interfaces and disturb NetworkManager/wpa_supplicant, so do not run them during a P2P test; only read-only `iw` queries were exercised here as a non-root user.

---

## 12. Bottom line for AetherCast

On this AR9565 laptop the default `ath9k` configuration is a **single-channel** station + P2P combination. Discovery works off-channel through ROC, but a P2P group on a different channel from the associated station is not sustainable: the stack either forces the group onto the station’s channel or fails with `-2`. That is a hardware/driver property, not a bug AetherCast can code around. The workable paths are co-channel operation, pausing the station connection, or an experimental `use_chanctx=1` mode.

The second constraint is the API: NetworkManager cannot choose GO intent or operating frequency, does not expose the peer’s operating channel, and cannot request `join=true`. Deterministic behaviour on a one-channel radio needs a helper that can express those parameters to wpa_supplicant — narrowly scoped, owned by one daemon, subordinate to NetworkManager.

Therefore: **NM-first, diagnostic-first, hybrid where it matters.** Show the channel budget, fix discovery cadence, map `-2` to a clear fallback, and test both fallbacks against the actual TV before promising support. A dual-channel adapter is the only comfortable long-term answer for “cast while staying online.”


## 13. Open questions

1. **Does the AltoBeam sink accept a source-as-GO group on channel 1?** The spec allows either side to be GO; the sink’s firmware may not. Test `go_intent=15 freq=2412` and `p2p_group_add freq=2412`.
2. **What channel does the sink use, and is it stable across sessions?** NM does not expose it; a supplicant-level capture or a sniff on 1/6/11 is needed. This decides whether “pause Wi-Fi” is rare or common.
3. **Does `use_chanctx=1` survive a real session on AR9565?** The combination table changes to 2 channels/3 interfaces; test association, group formation, throughput, suspend/resume and module reload. MiracleCast #75 is a report, not a validation.
4. **Does disconnecting the station free the channel end-to-end?** The code says `num_unused` becomes 1, but the supplicant and NM state machines must also tolerate a P2P group while the station is down, with a reliable restore path.
5. **How do Flatpak and polkit behave in inactive or remote sessions?** Local policy allows inactive sessions here; distro defaults differ, so a locked or remote session may prompt or deny.

---

## 14. Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| Single-channel radio cannot connect to a sink on another channel | High here | High: the core job fails | Co-channel guidance, pause-Wi-Fi fallback, dual-channel hardware recommendation |
| `use_chanctx=1` unstable or unsupported | Medium | High: crashes, disconnects, regulatory issues | Opt-in only, rollback command, explicit “experimental” label |
| NM API cannot express role or channel | Certain | Medium: no deterministic GO | Narrow helper scoped to `Connect`/`GroupAdd`; do not fork NM |
| Direct supplicant calls conflict with NM | Medium | High: state races, dropped station | One owner (the daemon); never call behind an active activation |
| A 20 s discovery restart causes false “no sinks” | High if copied | Medium: support load, bad first impression | Long find + `FindStopped`/backoff; surface scheduling errors distinctly |
| Sink firmware quirks (GO-only, WFD IE timing, WPS) | High | Medium: per-model failures | Compatibility matrix, per-sink strategy, visible diagnostics |
| DFS sink channel; WPS pairing window; name collision | Medium | Medium: connect failure, join window, rebrand cost | Avoid DFS for GO; short pairing window with confirmation; rename before release |

---

## 15. Sources

**Specifications and official documentation**

1. Wi-Fi Alliance, *Wi-Fi Display Technical Specification v2.1* (2017), §3.2, §4.3, §4.5, §5.1: <https://www.wi-fi.org/discover-wi-fi/miracast> (PDF member-gated; section numbers refer to the v2.1 text used locally).
2. Linux kernel, `include/net/cfg80211.h`: <https://github.com/torvalds/linux/blob/master/include/net/cfg80211.h#L5816-L5826>.
3. NetworkManager, `Device.WifiP2P` D-Bus API: <https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.WifiP2P.html>.
4. NetworkManager, `wifi-p2p` settings: <https://networkmanager.dev/docs/api/latest/settings-wifi-p2p.html>.
5. libnm, `NMWifiP2PPeer`: <https://networkmanager.dev/docs/libnm/latest/NMWifiP2PPeer.html>.
6. Flatpak, sandbox permissions: <https://docs.flatpak.org/en/latest/sandbox-permissions.html>.

**Source code**

7. Linux `ath9k/init.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath9k/init.c#L840-L900>.
8. Linux `ath10k/mac.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath10k/mac.c#L9630-L9790>.
9. Linux `ath11k/mac.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/ath/ath11k/mac.c#L10296-L10365>.
10. Linux `iwlwifi/mvm/mac80211.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/intel/iwlwifi/mvm/mac80211.c#L43-L70>.
11. Linux `rtw88/main.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw88/main.c#L106-L126>.
12. Linux `rtw89/core.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/realtek/rtw89/core.c#L168-L208>.
13. Linux `brcmfmac/cfg80211.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/broadcom/brcm80211/brcmfmac/cfg80211.c#L7499-L7540>.
14. Linux `mt76/mac80211.c`: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/mediatek/mt76/mac80211.c#L537-L545>.
15. wpa_supplicant 2.10 tarball: <https://deb.debian.org/debian/pool/main/w/wpa/wpa_2.10.orig.tar.xz>. Cited files: `wpa_supplicant/{p2p_supplicant.c,ctrl_iface.c,wpa_supplicant.c,scan.c,wpa_supplicant.conf,README-P2P}`, `wpa_supplicant/dbus/{dbus_new_handlers_p2p.c,dbus-wpa_supplicant.conf}`, `src/drivers/driver_nl80211_capa.c`, `src/p2p/p2p.c`.
16. NetworkManager, `nm-supplicant-interface.c`: <https://gitlab.freedesktop.org/NetworkManager/NetworkManager/-/blob/main/src/core/supplicant/nm-supplicant-interface.c#L3030-L3095>.
17. NetworkManager, `nm-device-wifi-p2p.c`: <https://gitlab.freedesktop.org/NetworkManager/NetworkManager/-/blob/main/src/core/devices/wifi/nm-device-wifi-p2p.c#L1370-L1393>.
18. GND 0.99.0, `nd-wfd-p2p-provider.c`: <https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/0.99.0/src/nd-wfd-p2p-provider.c>.
19. GND 0.99.0, `nd-wfd-p2p-sink.c`: <https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/0.99.0/src/nd-wfd-p2p-sink.c>.
20. GND Flatpak manifest: <https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json>.

**Issue reports and field evidence**

21. GND #296, channel unavailability on rtw89: <https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/296>.
22. GND #254, “Could not start P2P find”: <https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/254>.
23. GND #84, NetworkManager 1.22 P2P scan bug (MR 399): <https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/84>.
24. MiracleCast #75, ath9k `use_chanctx=1`: <https://github.com/albfan/miraclecast/issues/75> (also the [ath9k-devel multi-role thread](http://ath9k-devel.ath9k.narkive.com/ehPhw9BD/wi-fi-direct-multi-role-support-in-ath9k) and the [MiracleCast README](https://github.com/albfan/miraclecast#readme)).

**Local evidence (Kali 2026.2, 2026-10-20)**

25. `iw list`, `iw dev`, `iw reg get`; `lspci -nn`; `nmcli device status`; `wpa_supplicant -v`; `NetworkManager --version`; D-Bus reads of `P2PDeviceConfig`/`Capabilities`; `journalctl -b` excerpts; `/usr/share/polkit-1/actions/org.freedesktop.NetworkManager.policy`; `/usr/share/dbus-1/system.d/wpa_supplicant.conf`; `/run/wpa_supplicant/wlan0` (`770 root:netdev`); `/sys/module/ath9k/parameters/use_chanctx`. Reproduced with §11.1.
