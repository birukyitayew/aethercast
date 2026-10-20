> **Naming note.** This note was written on 2026-10-20, when the project was still the codename "AetherCast". The project was renamed to **Castrel** the same day because the codename collided with prior art in this exact niche — see [ADR-0004](../adr/0004-codename-not-brand.md). "AetherCast" below refers to the codename.

# 05 — Product and Architecture

**Status:** Research complete, pre-implementation. **Date:** 2026-10-20.
**Scope:** users and jobs, competitive teardown, differentiation, architecture, packaging, security and privacy, licensing and patents, project setup and CI, roadmap, documentation. Protocol and media detail lives in tracks 01–03 ([01](01-miracast-wfd.md), [02](02-linux-wifi-p2p.md), [03](03-media-pipeline.md)).

> **Naming warning.** "AetherCast" is a codename, not a cleared name, and it collides with prior art in this exact domain. [`ubports/aethercast`](https://github.com/ubports/aethercast) is an archived (24 Nov 2022), LGPL-3.0, 604-commit remote-display service for Miracast-like technologies that "approaches a generic abstraction layer … and collects them all under a common DBus API" ([README](https://github.com/ubports/aethercast#readme)). Canonical separately ran an "Aethercast" Miracast project for Ubuntu on 25 Dec 2015 ([Phoronix](https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi)). The name collides on domain, technology, and API shape. **Verified**; see §4.2.

**Evidence labels.** **Verified** = read in a primary source (specification, source code, official documentation, vendor policy) or reproduced locally. **Reported** = stated by a third party, not reproduced. **Speculative** = inference or forecast. Claims with a link to a primary source and no other label are **verified**. Targets are not observed results.

**Test baseline.** Kali Linux 2026.2, XFCE on X11, Qualcomm Atheros QCA9565/AR9565 (`ath9k`), 2.4 GHz only, single radio, wpa_supplicant 2.10, NetworkManager 1.56.1, PipeWire 1.6.6. GStreamer has `ximagesrc`/`openh264enc`/`avenc_aac`/`mpegtsmux`/`rtpmp2tpay` but **missing** `x264enc` and `pipewiresrc` ([journal](../../JOURNAL.md); hardware detail in [02](02-linux-wifi-p2p.md)). The sink is an AltoBeam-based Miracast TV; GND 0.99.0 discovers **no sinks** here. Retiring that is the first spike.

---

## 1. Executive summary

- **The niche is narrow and real.** The only credible open-source Linux Miracast *source* is GND; the products usually compared with it are *receivers* solving the inverse problem ([GND](https://github.com/GNOME/gnome-network-displays), [AirServer](https://www.airserver.com/)). Execution, not protocol breadth, is the moat.
- **Reliability is the product.** The GND tracker centers on discovery/P2P failure, freezing, missing cancel feedback, PIN pairing, and latency — not codecs ([issues API](https://gitlab.gnome.org/api/v4/projects/GNOME%2Fgnome-network-displays/issues?per_page=1&state=all)).
- **Recommended shape:** a Rust session daemon (`aethercastd`) with a versioned D-Bus API, a GTK4/libadwaita app, a first-class CLI, a per-session GStreamer process, NetworkManager/wpa_supplicant for P2P, and the ScreenCast portal plus PipeWire. This mirrors GND's app/daemon/stream split while replacing its coarse error model ([daemon](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/daemon/main.c)).
- **License and packaging:** GPL-3.0-or-later, because GND is GPL-3.0-or-later and code reuse must remain GPL ([header](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-sink.h)); SPDX/REUSE and a DCO, not a CLA. Flatpak first — GND's manifest shows the P2P permission model with explicit system-bus access to NetworkManager, Avahi, and firewalld ([manifest](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json)).
- **Two pre-code risks:** the 2.4 GHz radio must discover and connect while associated, and the name collides with prior art.

---

## 2. Users, jobs, and success criteria

### 2.1 Who this is for

| Persona | Context and job | Cost of failure | Basis |
| --- | --- | --- | --- |
| Meeting-room presenter | Linux laptop, shared TV/dongle, often no guest Wi-Fi; cast a deck in under a minute without switching networks | Starts late in front of clients | Reported, from OS "Project" workflows ([Microsoft](https://support.microsoft.com/en-us/windows/hardware/display-graphics/screen-mirroring-and-projecting-to-your-pc-or-wireless-display)) |
| Conference/classroom speaker | Unknown display, locked-down network; connect reliably and recover fast | Public failure, no plan B | Reported: a school user's request for PIN pairing ([GND #248](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/248)) |
| IT admin / developer | Managed image, mixed displays; deploy predictably or script/QA sessions | Help-desk load, flaky bugs | Verified gap: GND exposes D-Bus but no CLI binary ([`src/`](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/tree/master/src)) |
| Headless/kiosk integrator | Mini-PC driving a signage screen | No unattended recovery | Speculative; follows from the daemon/CLI split |

Accessibility users and contributors are secondary. v1 is explicitly **not** for DRM-protected video, latency-sensitive gaming, or receiver mode (§7.1, §10).

### 2.2 Jobs to be done

**Functional:** "When I walk into a room with a Miracast display, I want to connect from my Linux laptop in under a minute without leaving my Wi-Fi."

**Emotional:** "When it fails, I want a plain-language explanation and one next action, so I do not look incompetent in front of an audience."

**Social:** "I want to recommend this to my team without adding 'it works if you…'."

These imply visible discovery, a diagnosis for an empty list, cancellable connections, recovery without restart, and automatic audio. GND still makes users switch the PulseAudio output by hand ([README](https://gitlab.gnome.org/GNOME/gnome-network-displays#audio)).

### 2.3 Success criteria

Proposed targets, not measurements; they make the roadmap falsifiable.

| Metric | Target | Measurement |
| --- | --- | --- |
| Time to first frame | p50 ≤ 20 s, p90 ≤ 45 s | User action to first decoded frame |
| Discovery / session success | ≥ 95% / ≥ 90% | Lab runs per sink/adapter; state-machine outcomes |
| End-to-end latency | p50 ≤ 150 ms at 720p30 (speculative; [03](03-media-pipeline.md)) | Camera-measured screen→TV delay |

---

## 3. Competitive teardown

The market splits into open-source Linux sources, commercial receivers, and OS-native casting; only the first competes directly. The direct competitor is GND, not AirServer or Reflector: those bring content *to* a PC, while AetherCast sends it *from* a Linux PC. The UX bar is set by OS vendors — a short list, one action, visible state, a recovery path — and matching it is already differentiation.

| Product | Role | License/price | What it proves | Where it leaves a gap | Basis |
| --- | --- | --- | --- | --- | --- |
| GNOME Network Displays 0.99.0 | Miracast source, MICE, Chromecast | GPL-3.0-or-later, free | A WFD source on Linux is possible; ships a Flatpak with P2P permissions; publishes a device list | Coarse errors, fragile P2P, manual audio, GNOME-centric UX, no CLI, 500 ms hard-coded latency | Verified ([repo](https://github.com/GNOME/gnome-network-displays), [manifest](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json)) |
| MiracleCast | Miracast sink + source | LGPL-3.0, free | Sink-side WFD works; documents the NM/wpa_supplicant conflict | Source side "not implemented yet"; requires stopping NetworkManager and wpa_supplicant | Verified ([README](https://github.com/albfan/miraclecast)) |
| AirServer / Reflector 4 | Receiver (phone/PC → computer) | Commercial | The inverse job is viable; "just works" is expected | No Linux source mode; Reflector is Windows/macOS | Reported ([AirServer](https://www.airserver.com/), [Reflector](https://www.airsquirrels.com/reflector)) |
| Windows "Project" | OS-native source | Included | UX bar: a list, one click, a recovery path | Windows only; support varies by hardware | Verified ([Microsoft](https://support.microsoft.com/en-us/windows/hardware/display-graphics/screen-mirroring-and-projecting-to-your-pc-or-wireless-display)) |
| Google Cast / AirPlay | Sender/receiver protocols | Free SDK / proprietary | Clean discovery UX (AirPlay adds an optional passcode) | Not Miracast; different ecosystem requirements | Verified ([Google](https://developers.google.com/cast/docs/overview), [Apple](https://support.apple.com/en-us/102661)) |

### 3.1 What GND's tracker reveals

A GitLab API query on 2026-10-20 returned **487 issues: 61 open, 426 closed** ([issues API](https://gitlab.gnome.org/api/v4/projects/GNOME%2Fgnome-network-displays/issues?per_page=1&state=all)). Five reports, each verified for title and state, map to five requirements. [#373 — wpa_supplicant connection fails](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/373) (open): P2P fails at the supplicant layer → **environment diagnostics** (driver, `p2p-dev-*`, WFDIEs, channel, firewall). [#20 — Streaming freezes after a few seconds](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/20) (closed) → **liveness watchdog** and automatic recovery. [#4 — No feedback when the sink cancels](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/4) (closed) → **typed remote-disconnect events** and a next action. [#248 — PIN Miracast](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/248) (open) → **PIN flow**, or an explicit unsupported message. [#246 — Authorization supplicant timed out](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/246) (closed) → **supplicant/driver diagnostics** and version checks.

**Open feature requests are a free roadmap** and direct evidence of user demand (API-verified, 2026-10-20): [#507](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/507) "Hard-coded 500 ms pipeline latency adds ~0.4 s of lag" (a latency budget is user-visible, not internal — [01](01-miracast-wfd.md) F6); [#154](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/154) "Enable resolution selection code" (presets); [#275](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/275) "Extract functionality as a library" (a stable API); [#174](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/174) "RFE: Airplay support" (Phase 3).

The README adds two failure classes: NetworkManager must use `wpa_supplicant`, not `iwd`; and a local firewall can block DHCP or the connection ([README](https://gitlab.gnome.org/GNOME/gnome-network-displays#debugging)). These are the normal failure surface.

### 3.2 Lessons

**Do not out-protocol GND on day one** — it already covers WFD P2P, MICE, and Chromecast, and adding AirPlay or DLNA before Miracast is reliable repeats its weakest pattern. **Diagnosis is a feature**: most reports above are failures of explanation as much as of networking. And a capture path that only works on GNOME/Wayland, P2P that works on one chipset, or a compatibility list without evidence is a support burden.

---

## 4. Where AetherCast can win

### 4.1 Differentiators, ranked

| Differentiator | User problem | v1? | Defensibility | Evidence |
| --- | --- | --- | --- | --- |
| Explainable discovery | "The list is empty and I do not know why" | Yes | High: probes and a rule engine | [GND #373](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/373), [README](https://github.com/GNOME/gnome-network-displays#debugging) |
| One-click connect with presets | "I do not want to choose codecs or audio sinks" | Yes | Medium: UX and defaults | [GND #154](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/154), [README](https://github.com/GNOME/gnome-network-displays#audio) |
| CLI and stable D-Bus API | "I need to script, test, or use another desktop" | Yes | High: architectural, hard to retrofit | GND's whole contract is `StartStream(sink_uuid) → stream_unit_name` and `StopStream(stream_unit_name)` — no discovery, state, or errors ([interface](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/org.gnome.NetworkDisplays.Manager.xml)); [#275](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/275) |
| Compatibility center | "Will it work with my TV?" | Yes, seeded | High: evidence accumulates | [GND tested devices](https://github.com/GNOME/gnome-network-displays#devices) |
| Flatpak-first, narrow permissions | "Do not break my distro or run as root" | Yes | Medium | [GND manifest](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json) |
| Diagnostics bundle | "I need to file an actionable bug" | Yes | Medium | [GND #4](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/4) |
| Provider/plugin API | "I want to add a protocol or reuse the engine" | Design now, post-MVP | Medium: architecture | [#275](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/275), [#299](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/299) |
| Cross-desktop, accessible UI | "I use XFCE/KDE, not GNOME" | Yes | Low: table stakes | [GTK4](https://docs.gtk.org/gtk4/) |

**Priority call (judgement, speculative):** discovery diagnostics, one-click connect, and the CLI/D-Bus contract are the v1 moat. The provider API must be *designed* in v1 — a provider trait, a sink/session model, a versioned wire contract — but only the WFD provider ships. Telemetry is deferred (v2+). Do not compete on codec count, receiver features, DRM circumvention, or an "AI" layer.

### 4.2 The name must change

"AetherCast" fails the clearance test on three axes:

| Prior art | Evidence | Collision |
| --- | --- | --- |
| `ubports/aethercast` | Archived 2022-11-24, LGPL-3.0, 604 commits, 10 stars / 7 forks ([repo](https://github.com/ubports/aethercast), [mirror](https://gitlab.com/ubports/development/core/aethercast)) | Same name, domain, API concept |
| Canonical "Aethercast" | Miracast for Ubuntu, Launchpad project, 25 Dec 2015 ([Phoronix](https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi)) | Same name and technology, prior use here |
| Our own new repo | [`birukyitayew/aethercast`](https://github.com/birukyitayew/aethercast) | Public already; renaming is cheap now, expensive later |

Treat it as a codename and run a two-week naming sprint before any Flathub submission: (1) trademark search in US, EU, and UK, registered and common-law; (2) code-host and package/app-ID search; (3) domains, handles, spelling, read-aloud; (4) brand-usage review, since "Miracast" and "Wi-Fi CERTIFIED" carry usage rules ([Wi-Fi Alliance brands](https://www.wi-fi.org/our-brands)). Naming territories: light/beam, bridge/relay, short invented words — provisional **uncleared** candidates *Lumenbeam*, *Castrel*, *Mirrorling*, *Waveline*. Avoid "Chromecast." Rename repo, D-Bus namespace, Flathub ID, and packages together; GitHub redirects are cheap, the rest is not.

---

## 5. Architecture

| Option | Pros | Cons | Verdict |
| --- | --- | --- | --- |
| A. Fork GND | Fastest working WFD source; proven on devices; manifest exists | Inherits coarse errors and GNOME-centric UX; fork divergence; C maintenance; no stable API | Fallback |
| B. Rust daemon + D-Bus + GTK4 + CLI | Typed state/errors; headless; multiple frontends; testable; crash isolation | Must implement WFD/RTSP and P2P orchestration; two-language stack; longer MVP | **Recommended** |
| C. Single-process GTK app | Simplest deployment; no IPC | Stream dies with UI; no headless; poor crash isolation | Reject |
| D. GND engine + new UX | Reuses proven code; faster than B | GPL obligations; internal APIs are not a library ([#275](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/275)); upstream churn | Hybrid if B slips |

Option B is recommended because the differentiators — diagnosis, CLI, stable D-Bus, crash isolation — are architectural and hard to bolt on later. The cost is real: WFD session setup, RTSP M1–M7, and P2P orchestration must be implemented or wrapped ([03 §7](03-media-pipeline.md)). Mitigate with a protocol spike first; keep A/D as fallbacks.

The process split borrows GND's design: a session daemon owns discovery and a manager, then spawns a transient stream unit per session ([daemon](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/daemon/main.c), [`StartTransientUnit`](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-manager.c)), so the UI can close without killing the stream and a media crash does not take down discovery.

### 5.1 Decomposition

```text
GTK4 app ─┐
CLI ──────┼── D-Bus (session) ── aethercastd ── NetworkManager / wpa_supplicant
other DE ─┘                        │            Avahi
                                    └── systemd user manager ── aethercast-stream
                                                                 GStreamer: capture → encode
                                                                 → MPEG-TS/RTP → sink
```

| Component | Responsibility |
| --- | --- |
| `aethercastd` | Provider registry, discovery, sink model, state machine, policy, D-Bus, diagnostics |
| `aethercast-stream` | Capture, encode, mux, RTP, A/V sync, watchdog; one per session |
| `aethercast` CLI | Scriptable commands, JSON, diagnostics export |
| GTK4 app | Selection, presets, recovery, accessibility |
| `libaethercast-core` | Domain types, provider trait, RTSP/WFD state machines, error taxonomy; I/O injected |

**Why a daemon:** survives UI closure, enables headless/kiosk and other desktops, isolates crashes, and makes tests possible without a display server. **Why not root:** NetworkManager and polkit own privileged Wi-Fi operations; a root daemon adds attack surface for no benefit.

**D-Bus contract.** Interface names are reverse-DNS and versioned ([D-Bus](https://dbus.freedesktop.org/doc/dbus-specification.html)). The daemon owns `org.aethercast.Manager1` (renamed with the product) on the session bus, D-Bus-activated as a systemd user service ([systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)).

| Interface | Members (sketch) | Notes |
| --- | --- | --- |
| `Manager1` | `GetSinks()`, `Connect(id, options) → object`, `Disconnect()`, `GetSessions()`, `GetDiagnostics()` | Sink IDs stable per discovery generation |
| `Session1` | `State`, `Sink`, `Error`, `Stop()`, `Pause()` | One per session; `StateChanged`, `ErrorOccurred` |
| `Diagnostics1` | `ExportBundle(options) → path`, `RunChecks() → results` | Redacted; explicit action |

Compare GND's verified contract: `org.gnome.NetworkDisplays.Manager` exposes only `StartStream` and `StopStream` ([interface XML](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/org.gnome.NetworkDisplays.Manager.xml)). A client cannot enumerate sinks, observe state, or read an error — that gap is the differentiator.

### 5.2 Session state machine

GND exposes disconnected, firewall, wait-P2P, wait-socket, wait-streaming, streaming, and a single error value — verified in `NdSinkState` (`DISCONNECTED`, `ENSURE_FIREWALL`, `WAIT_P2P`, `WAIT_SOCKET`, `WAIT_STREAMING`, `STREAMING`, `ERROR`) ([`nd-sink.h`](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-sink.h)). Keep the phases; make failure a first-class outcome with a cause.

```text
Idle → Discovering → Selected → PreparingLink → ConfiguringNetwork
     → Negotiating → Streaming → Stopping → Idle
                          ↘ Failed(recoverable?) ↗
```

| State | User-visible copy | Exit |
| --- | --- | --- |
| `Discovering` | "Looking for displays…" + live checklist | Sink found / timeout / environment error |
| `Selected` | "Checking what this display supports" | Capability result |
| `PreparingLink` | "Connecting to the display" | P2P group formed / failure |
| `ConfiguringNetwork` | "Setting up the connection" | Address / timeout |
| `Negotiating` | "Starting the stream" | `PLAY` / rejection / PIN |
| `Streaming` | "Casting to <name>" | Stop / sink disconnect / stall |
| `Stopping` | "Disconnecting" | Idle |

Every transition is timestamped with a session ID. The state machine is the single source of truth for UI, CLI, and diagnostics.

### 5.3 Error taxonomy

A single `ERROR` value cannot help a user or triage a bug. Each code gets a stable string, a user sentence, a detail field, and actions.

| Code | Typical cause | User action |
| --- | --- | --- |
| `wifi.p2p-group-failed` | Driver lacks P2P, or group/WPA setup fails | Supported adapter; retry; change channel |
| `wifi.channel-conflict` | Single radio cannot serve station + P2P on different channels | Disconnect station or pick a channel |
| `net.dhcp-timeout` | Firewall or dnsmasq blocked address assignment | Open firewall / retry |
| `rtsp.connect-failed` | Sink unreachable on 7236, or wrong IP | Retry; check sink |
| `pairing.pin-required` | Sink requires a WPS/WFD PIN the flow cannot complete | Show PIN entry, or explain unsupported |
| `media.capture-denied` | Portal permission denied or canceled | Reopen picker |
| `media.encoder-missing` | GStreamer encoder absent | Install codec pack; Flatpak bundles it |
| `media.pipeline-error` | Encoder/muxer failure | Export diagnostics; retry |
| `sink.disconnected` | Sink ended the session | Reconnect; check TV |

Evidence: the GND reports in [§3.1](#31-what-gnds-tracker-reveals) and the README's firewall/`iwd` notes. Each code gets a documentation anchor and, where possible, a one-click fix.

### 5.4 Capture, media, and network

**Capture.** Wayland uses the ScreenCast portal, which creates PipeWire streams and supports monitor/window/virtual sources, persistence tokens, and cursor modes ([portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html), [PipeWire](https://pipewire.org/)); it is the portable capture path in Flatpak. Persist the restore token and treat denial as normal. X11 falls back to XComposite/XDamage. The portal returns one stream per selected monitor, so the sink model must expose per-monitor sources ([03](03-media-pipeline.md)).

**Media.** H.264 + AAC in MPEG-TS over RTP is the pragmatic baseline; GND's dependency list is the matrix — `openh264` or `x264`; `fdkaacenc`, `faac`, or `avenc_aac`; plus `gst-rtsp-server` ([README](https://gitlab.gnome.org/GNOME/gnome-network-displays)). The stream process owns the pipeline; a missing encoder becomes a typed error, not a crash, and `x264enc` and `pipewiresrc` must be bundled in the Flatpak ([journal](../../JOURNAL.md)). GND requires a manual PulseAudio sink switch ([README](https://github.com/GNOME/gnome-network-displays#audio)); AetherCast should create a virtual sink, route the selected app or system mix, and restore the previous default on stop. "Presentation" (1080p30), "Balanced" (720p30), and "Low latency" (720p30) are proposed defaults, not guarantees. Latency budgets live in [03](03-media-pipeline.md).

**Network and privilege.** NetworkManager with `wpa_supplicant`; `iwd` is unsupported by GND, and the README documents `p2p-dev-*` and WFDIEs requirements ([README](https://gitlab.gnome.org/GNOME/gnome-network-displays#debugging)). Detect `iwd` and explain; do not pretend to support it in v1. A local firewall can block DHCP or RTSP; request the narrowest opening, scoped to the session, and roll it back. On a single radio, P2P may require leaving the current channel or dropping the station link — never silently disconnect the user's network; explain the tradeoff. This is **speculative** until measured on the QCA9565, but it is the likeliest source of "discovery works, connection fails." No root daemon, no setuid; Flatpak grants the system-bus names GND already uses — `org.freedesktop.NetworkManager`, `org.freedesktop.Avahi`, `org.fedoraproject.FirewallD1` — plus `--share=network` ([manifest](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json)).

---

## 6. Packaging and distribution

**Flatpak/Flathub is first** (medium effort): a strong sandbox with explicit system-bus names, proven by GND. **Debian, Fedora, and Arch packages are second** (medium): direct integration with system NM/Avahi/GStreamer. **Snap is low priority** (medium-high): strict interfaces, classic needs approval ([Snap](https://snapcraft.io/docs/explanation/security/snap-confinement/)). **AppImage is an optional fallback** (low): no sandbox.

GND's manifest shows the hard part is feasible inside Flatpak: it bundles NetworkManager, Avahi, firewalld, GStreamer ugly plugins, `gst-rtsp-server`, and libportal, with exactly these finish args (verified against the manifest in the 0.99.0 source tree):

```text
--device=dri
--share=ipc
--share=network
--socket=pulseaudio
--socket=wayland
--socket=fallback-x11
--system-talk-name=org.freedesktop.Avahi
--system-talk-name=org.freedesktop.NetworkManager
--system-talk-name=org.fedoraproject.FirewallD1
```

Start from this budget and justify every addition. Notably absent: home access, `--device=all`, `--talk-name=org.freedesktop.Flatpak` (the sandbox escape hatch), and `--socket=system-bus` (discouraged by Flatpak guidance). The default sandbox has "no access to the network", no host files, no system bus, and a limited session bus ([Flatpak](https://docs.flatpak.org/en/latest/sandbox-permissions.html)).

**The `--share=network` trade-off must be stated, not hidden.** Flatpak's documentation warns that giving network access "also grants access to all host services listening on abstract Unix sockets (due to how network namespaces work), and these have no permission checks. This unfortunately affects e.g. the X server and the session bus" ([Flatpak](https://docs.flatpak.org/en/latest/sandbox-permissions.html)). Miracast needs real network access, so the widening is unavoidable — but it belongs in the threat model and the Flathub justification.

**Sandbox limits specific to this app.** Wi-Fi P2P needs the system bus and a root-owned supplicant socket; a systemd *user* service cannot hold `CAP_NET_ADMIN` or read a root-owned socket without group membership, so any helper needing more than `netdev` must be a **system** unit ([02 §7.2](02-linux-wifi-p2p.md)). Capture inside Flatpak is fine via the portal, but X11 capture needs `--share=ipc` plus `--socket=fallback-x11`. Test both in the sandbox.

Distro packages serve users who will not use Flatpak; depend on the stack rather than bundling it, following Debian/Fedora Rust vendoring conventions ([Debian debcargo](https://salsa.debian.org/rust-team/debcargo), [Fedora](https://docs.fedoraproject.org/en-US/packaging-guidelines/Rust/)). Use SemVer ([link](https://semver.org/)) and Keep a Changelog ([link](https://keepachangelog.com/en/1.1.0/)); `Manager1` changes only on a compatibility break.

---

## 7. Security and privacy

| Asset | Adversary/failure | Mitigation |
| --- | --- | --- |
| Screen/audio | Eavesdropper on the WFD link; malicious sink | Rely on WFD link security and wpa_supplicant; never downgrade; treat the sink as untrusted; parser hardening |
| Pairing | Sink demands a PIN | PIN entry for WPS/WFD PIN ([#248](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/248)); never skip one silently |
| Infrastructure path | MICE peer with no handshake — hijack or spoofing | GND's MICE provider sends only a bare "Source Ready" (hostname, port, source ID) with **no** security handshake and **no** PIN exchange ([01 §6.5](01-miracast-wfd.md)); add DTLS or PIN for that path, or refuse it |
| LAN presence | mDNS advertisement leakage | Publish only while a session is active; per-session instance names; never publish the sink list ([01](01-miracast-wfd.md)) |
| Credentials and diagnostics | Same-user process, logs, or a public bug report | Never read/log secrets; redact by default; preview before export |
| D-Bus control | Any same-user process casting or stopping | Session bus is per-user; no cross-user service |
| Supply chain | Compromised dependency or CI token | Lockfiles, `cargo-deny`, `cargo-audit`, pinned Actions |

The security story is mostly a permission story. Run as the user, capture only through the portal on Wayland, request the minimum system-bus names, and never hand raw frames to the GUI. The stream process is short-lived and killed with the session; restore tokens reduce prompts but must be stored carefully ([portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html)).

### 7.1 HDCP and DRM: be honest

HDCP is not a codec toggle, and the licensing is not open. Verified from Digital CP's own pages: implementing HDCP requires a license from Digital Content Protection LLC; a licensee signs an agreement and pays an **Annual Fee**, after which DCP supplies "procedures for ordering device key sets, if applicable" ([licensing](https://www.digital-cp.com/licensing)); and a "Licensed Product" is defined to include "a software application", so a software implementation is possible in principle ([FAQ](https://www.digital-cp.com/faqs)). HDCP is licensed for HDMI/DVI, DisplayPort, GVIF and UDI interfaces ([FAQ](https://www.digital-cp.com/faqs)). **I found no statement about per-unit royalties on those pages** — the word does not appear — so the absence of a royalty rate is unknown, not established.

Open source cannot ship secret device keys: distributing them would violate the license and destroy the security property that makes HDCP work. If a sink requires HDCP, detect it, explain the limitation, and stop. Do not strip HDCP, spoof a sink, or capture protected surfaces, and do not market the app as a Netflix/Disney+ solution; the honest use case is presentations, photos, and unprotected video.

### 7.2 Privacy posture

- **Local-first:** no account, cloud relay, or analytics SDK; data stays on the machine unless exported.
- **Opt-in only:** telemetry is off by default, previewable, and limited to counters and error codes — never screen content, SSIDs, filenames, or MACs.
- **Diagnostics with preview:** a zip of redacted logs, environment facts, and a session timeline; the user sees what is shared.
- **Disclosure:** enable GitHub private vulnerability reporting and publish a response target ([GitHub](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)); a `SECURITY.md` exists ([SECURITY.md](../../SECURITY.md)).

---

## 8. Licensing, trademarks, and patents

| License | Reuse GND code | Patent grant | Copyleft | Verdict |
| --- | --- | --- | --- | --- |
| GPL-3.0-or-later | Yes | Explicit | Strong, whole work | **Recommended** |
| LGPL-3.0 | No (GND is GPL) | Explicit | Weak, library-level | Not sufficient alone |
| Apache-2.0 | No | Explicit | None | Not chosen |

GND's headers say GPL-3.0-or-later and ships the GPLv3 text ([`nd-sink.h`](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-sink.h), [COPYING](https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/COPYING)), so copied code keeps that license and the combined work must be GPL-3.0. GPLv3 includes an express patent grant — it refers to "any patent licenses granted under the third paragraph of section 11" and states that "the GPL assures that patents cannot be used to render the program non-free" ([GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html)). That matters in a patent-heavy protocol space.

**Recommendation:** GPL-3.0-or-later, SPDX identifiers, REUSE metadata ([REUSE](https://reuse.software/), [SPDX](https://spdx.dev/)), and DCO sign-off rather than a CLA ([DCO](https://developercertificate.org/)). Do not add a GStreamer exception by default: GStreamer notes that apps which may use proprietary plugins often choose LGPL/MPL/BSD or a GPL exception ([GStreamer](https://gstreamer.freedesktop.org/documentation/licensing.html)), while AetherCast stays GPL. Revisit with counsel only on a concrete distribution need. The PR template already warns that "the project license is not final and that copied code from incompatible licenses is not acceptable" ([PR template](../../.github/PULL_REQUEST_TEMPLATE.md)); update it once the ADR passes.

**Trademarks.** "Miracast®" is listed among the Wi-Fi Alliance's brands, and "Wi-Fi CERTIFIED®" is "an internationally-recognized seal of approval for products indicating that they have met industry-agreed …" standards; the Alliance also publishes a logo-use policy ([Wi-Fi Alliance brands](https://www.wi-fi.org/our-brands)). Say "works with Miracast"; do not use the logo or imply endorsement. The same applies to "Chromecast" (Google) and "AirPlay" (Apple); no other brand goes in the product name.

**Patents.** The WFD specification is published by the Wi-Fi Alliance ([Miracast Specification](https://www.wi-fi.org/file/miracast-specification)). **No public Miracast royalty rate was found** — an absence of evidence, not evidence of no royalties. Miracast-related patents exist (e.g., [US9762939B2](https://patents.google.com/patent/US9762939B2/en)), and H.264 has its own pool ([Via LA](https://www.via-la.com/licensing-2/avc-h-264/)). **Speculative:** a freedom-to-operate review is prudent before commercial distribution, especially if bundling codecs. Not legal advice.

---

## 9. Project setup and CI

The public repository is [`birukyitayew/aethercast`](https://github.com/birukyitayew/aethercast) ([journal](../../JOURNAL.md)). It already contains `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `JOURNAL.md`, `docs/{architecture,compatibility,roadmap}.md`, ADR 0001, four issue templates, and a PR template. What is **missing** and must be added before code: the license file, `REUSE.toml`, CI workflows, and user-facing documentation. The CI tables are a specification, not a description of current behaviour.

A Cargo workspace with six crates — `aethercast-core` (domain types, state machine, provider trait, error taxonomy), `aethercast-protocol` (WFD/RTSP types, D-Bus bindings), `aethercastd` (daemon, discovery, policy, D-Bus service), `aethercast-stream` (capture + GStreamer, one per session), `aethercast-cli` (`aethercast`, JSON output), and `aethercast-gui` (GTK4/libadwaita) — plus `data/` (systemd unit, D-Bus XML, metainfo, icons), `flatpak/`, `docs/`, `tests/`.

Use a workspace with a shared `rust-version` (MSRV) and a pinned toolchain ([Cargo](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field)); the core has no GTK or NetworkManager dependency.

| Gate | Tool | When | Blocking |
| --- | --- | --- | --- |
| Formatting, lints | `cargo fmt --check`; `cargo clippy -- -D warnings` | Every PR | Yes |
| Tests | `cargo test` / `nextest`, mocked NM/Avahi/D-Bus, dummy sink (Ubuntu, Fedora, Debian; stable + MSRV) | Every PR | Yes |
| Dependency policy | `cargo-deny` (licenses, advisories, bans) | Weekly | Yes |
| Vulnerability audit | `cargo-audit` / RustSec ([RustSec](https://rustsec.org/)) | Daily | Yes |
| License metadata, docs | REUSE / SPDX; Markdown link checker | Every PR | Yes |
| Flatpak build | `flatpak-builder`, real manifest, lint | PR + release | Yes |
| Security posture | OpenSSF Scorecard ([Scorecard](https://github.com/ossf/scorecard)) | Weekly | No |
| Release | Signed tag, checksums, SBOM, SLSA provenance ([SLSA](https://slsa.dev/)), Flathub PR | Tag | Yes |
| Hardware smoke test | Manual lab | Release candidate | Yes |

GND ships a dummy sink for this kind of testing ([README](https://github.com/GNOME/gnome-network-displays#testing)); the AetherCast dummy sink must be spec-shaped enough to exercise M1–M7. Pin third-party Actions to commit SHAs, use minimal `GITHUB_TOKEN` permissions, and avoid `pull_request_target` with untrusted code ([GitHub](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)); enable Dependabot ([docs](https://docs.github.com/en/code-security/dependabot)). Hardware-in-the-loop CI is impractical for Wi-Fi P2P; keep a compatibility lab and a manual smoke test.

Process: Conventional Commits ([link](https://www.conventionalcommits.org/en/v1.0.0/)), DCO sign-off, issue templates requesting a diagnostic bundle and compatibility entry, `good first issue` labels scoped to docs/UI copy/provider tests, monthly minors pre-1.0, out-of-band security fixes, and releases shipping a changelog, Flatpak build, packages, checksums, and SBOM.

---

## 10. Roadmap

Durations are estimates and therefore **speculative**; the ordering is the decision.

| Phase | Duration | Scope | Dependencies | Exit criteria | Main risk |
| --- | --- | --- | --- | --- | --- |
| 0 — Research and spike | 2–3 weeks | Research pack, ADRs, name clearance, protocol/radio spike | GND 0.99.0 source; test TV; `ath9k`/wpa_supplicant 2.10 | Documented yes/no on discovery + first frame; stack and licence ADRs | The radio or TV cannot do P2P here |
| 1 — MVP source | 10–12 weeks | WFD P2P, RTSP M1–M7, H.264+AAC, GTK4, CLI, D-Bus, Flatpak, diagnostics | GStreamer with `openh264enc`/`avenc_aac`/`mpegtsmux`; ScreenCast portal; NM > 1.15.2 with `wpa_supplicant` | Cast to two real sinks on X11 and Wayland; recover from a failure without restart; Flatpak installs and runs | Scope creep into protocols |
| 2 — Hardening | 6–8 weeks | Presets, audio routing, multi-monitor, PIN, error UX, accessibility, translations, packages, compatibility | Phase 1 exit; `pipewiresrc` and `x264enc` in the Flatpak; a second sink | ≥ 95% discovery; ≥ 90% session success; no manual audio; accessibility audit | Hardware variance; support load |
| 3 — Protocols | 12+ weeks | Google Cast, AirPlay, DLNA via the provider API | Phase 2 exit; provider trait frozen in Phase 1 | Two providers share the session model without core changes | Each protocol is a new surface |
| 4 — Sink/headless | TBD | Miracast sink, kiosk, UIBC, diagnostics | Phase 3 exit; DTLS/PIN for the infrastructure path | Headless cast from CLI; sink receives from a phone | Different security model |

**Phase 0 is a gate.** Before UI work, answer: does the QCA9565/ath9k radio create a usable `p2p-dev-*`; does wpa_supplicant advertise WFDIEs; does the AltoBeam TV respond to probes; can a group form while associated? If no, the honest paths are (a) document the limit and target supported hardware, (b) use MICE/Avahi if the sink supports it, or (c) recommend a known-good USB adapter. A beautiful app around a radio that cannot discover is the worst outcome.

**Fallback rule:** if Rust/GStreamer is not streaming by mid-Phase 1, switch to Option A/D and deliver the differentiators around a reused engine.

### 10.1 MVP acceptance criteria

1. Discover a Miracast sink on X11 and Wayland, with an explanation when the list is empty.
2. Complete WFD negotiation and RTSP setup against at least two real sinks, then stream H.264 + AAC with A/V sync for 30 minutes without a freeze.
3. Recover from sink disconnect and pipeline error without restarting.
4. Route audio automatically and restore the previous default on stop.
5. Export a redacted diagnostic bundle with the session timeline and error code.
6. Install from Flatpak with documented permissions; run the CLI headlessly; pass an accessibility smoke test (keyboard-only navigation, screen-reader labels).

---

## 11. Documentation system

The repository already has `docs/research/`, `docs/protocols/`, `docs/adr/`, plus `docs/architecture.md`, `docs/compatibility.md`, `docs/roadmap.md`, and `JOURNAL.md` ([docs index](../README.md)). Add a consistent note contract so documents do not rot into undated claims.

| Type | Path | Required sections | Cadence |
| --- | --- | --- | --- |
| Research note | `docs/research/NN-topic.md` | Scope, evidence labels, findings, bottom line, questions, risks, sources | One per track |
| Protocol note | `docs/protocols/topic.md` | Actors, sequence, messages, state machine, failures, captures | On protocol change |
| ADR | `docs/adr/NNNN-title.md` | Context, decision, status, date, consequences, alternatives | Per irreversible choice |
| Compatibility entry | `docs/compatibility.md` | Adapter, driver, sink, session, result, evidence | Per test run |

**Templates.** Two are enough. The **ADR** template is `# ADR NNNN: <title>`, then status, date, deciders, then `## Context`, `## Decision`, `## Alternatives considered`, `## Consequences`, `## Revisit when` — matching ADR 0001 ([ADR 0001](../adr/0001-research-first-and-v1-scope.md)). The **compatibility row** is `Adapter | Chipset | Driver/kernel | Bands/channels | Station+P2P concurrency | Desktop/session | Sink make/model | WFD version | Result | Evidence | Date`; without a reproducible evidence link it is an anecdote, not a result.

Docs are versioned with the code; claims are dated and sourced; ADRs are immutable once accepted (supersede, do not rewrite); compatibility rows require reproducible evidence, not manufacturer claims. Use a Markdown link checker in CI and Vale if the docs grow ([Vale](https://vale.sh/)). The journal records dead ends deliberately — a documented wrong turn costs an afternoon, undocumented a month.

---

## Bottom line for AetherCast

1. **Rename before building a brand.** "AetherCast" has exact prior art in the same domain (`ubports/aethercast`), a Canonical history in the same niche (2015), and is already a public repo name. Treat it as a codename, run the §4.2 checklist, and rename repo, D-Bus namespace, Flathub ID, and packages together.
2. **Build the diagnosis layer first.** Explainable discovery, a typed state machine, and a redacted diagnostics bundle are what GND's tracker demands; a prettier UI over opaque failures will not win.
3. **Use daemon + D-Bus + GTK4 + CLI, with a Rust core and GStreamer media.** This matches GND's process split, enables headless and cross-desktop use, isolates media crashes, and makes the core testable. Ship only WFD P2P in v1.
4. **License GPL-3.0-or-later.** It permits GND reuse, includes an express patent grant, and fits the ecosystem. Add SPDX/REUSE and a DCO; skip the CLA.
5. **Ship Flatpak first, distro packages second.** GND's manifest demonstrates the permission model; keep the budget narrow and document `--share=network`'s abstract-socket widening explicitly.
6. **Gate on the radio, stay conservative, keep v1 small.** Prove discovery and a first frame on the QCA9565/AltoBeam combination before UI investment. No HDCP circumvention, no Wi-Fi CERTIFIED claim, no proprietary codec bundling. Cast/AirPlay/DLNA/sink/UIBC come later.

## Open questions

- **Can the test radio do P2P while associated?** Resolve with `nmcli device`, `iw dev`, `wpa_cli status`, and a supplicant debug log. If a channel conflict is fundamental, choose between "disconnect station" UX and a supported-adapter recommendation.
- **Does the AltoBeam TV respond to WFD probes or only MICE/Avahi?** A packet capture and a MICE browse change the v1 discovery path.
- **Which encoder ships in Flatpak, and is 1080p30 realistic on 2.4 GHz?** Test `openh264` vs `x264`, measure bitrate/drops/latency, and resolve patent/distribution implications before the first Flathub submission.
- **Audio capture model:** virtual sink vs per-app capture vs portal audio. GND's manual switch is unacceptable; the replacement needs a PipeWire design and test.
- **Name:** which shortlist candidate clears trademark, package, domain, and app-ID checks? Resolve in Phase 0.
- **Upstream vs fork:** which fixes belong in GND, wpa_supplicant, NetworkManager, or GStreamer, and which are AetherCast-specific?

## Risks

| Risk | Likelihood | Impact | Mitigation | Early warning |
| --- | --- | --- | --- | --- |
| Test radio/driver cannot discover or connect | High | High | Phase 0 spike; compatibility list; adapter guidance; MICE fallback | Empty list persists after fixes |
| Sink quirks break negotiation | High | Medium | Test multiple sinks; capability fallbacks; per-sink notes | Works on one TV, fails on another |
| Users expect DRM content to work | High | Medium | Pre-flight copy; detect HDCP; no silent failure | "Black video" reports |
| Patent, codec, or name conflict | Medium | High | GPL posture; no proprietary codecs; rename in Phase 0; counsel before commercialization | OEM interest; cease-and-desist |
| Scope creep into Cast/AirPlay/sink | High | High | v1 scope lock; roadmap gates; provider API designed, not implemented pre-MVP | Phase 1 slips while protocols land |
| Maintainer burnout | Medium | High | Narrow MVP; documented decisions; onboarding | PR backlog and release gaps |
| Vulnerability in daemon/parser | Medium | High | Untrusted-sink model; fuzzing; audits; disclosure policy | Crashes from malformed RTSP |

## Sources

**Project and prior art.**

https://github.com/GNOME/gnome-network-displays ; https://gitlab.gnome.org/api/v4/projects/GNOME%2Fgnome-network-displays/issues?per_page=1&state=all ; GND issues (4, 20, 119, 154, 174, 230, 246, 248, 275, 299, 373, 507) — https://gitlab.gnome.org/GNOME/gnome-network-displays/-/issues/ ; https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/build-aux/flatpak/org.gnome.NetworkDisplays.json ; https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/org.gnome.NetworkDisplays.Manager.xml ; https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-sink.h ; https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/daemon/main.c ; https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/src/nd-manager.c ; https://gitlab.gnome.org/GNOME/gnome-network-displays/-/blob/master/COPYING ; https://github.com/albfan/miraclecast ; https://github.com/ubports/aethercast ; https://gitlab.com/ubports/development/core/aethercast ; https://www.phoronix.com/news/Aethercast-Ubuntu-WiDi

**Market and OS behavior.**

https://www.airserver.com/ ; https://www.airsquirrels.com/reflector ; https://support.microsoft.com/en-us/windows/hardware/display-graphics/screen-mirroring-and-projecting-to-your-pc-or-wireless-display ; https://support.apple.com/en-us/102661 ; https://developers.google.com/cast/docs/overview

**Platform.**

https://docs.flatpak.org/en/latest/sandbox-permissions.html ; https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html ; https://pipewire.org/ ; https://snapcraft.io/docs/explanation/security/snap-confinement/ ; https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html ; https://wiki.archlinux.org/title/Systemd/User ; https://dbus.freedesktop.org/doc/dbus-specification.html ; https://docs.gtk.org/gtk4/ ; https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/ ; https://networkmanager.dev/docs/api/latest/ ; https://w1.fi/wpa_supplicant/ ; https://www.wi-fi.org/discover-wi-fi/wi-fi-direct ; https://www.wi-fi.org/file/miracast-specification

**Licensing and security.**

https://www.gnu.org/licenses/gpl-3.0.html ; https://www.apache.org/licenses/LICENSE-2.0 ; https://www.mozilla.org/en-US/MPL/2.0/ ; https://gstreamer.freedesktop.org/documentation/licensing.html ; https://www.digital-cp.com/faqs ; https://www.digital-cp.com/licensing ; https://www.wi-fi.org/our-brands ; https://patents.google.com/patent/US9762939B2/en ; https://www.via-la.com/licensing-2/avc-h-264/ ; https://rustsec.org/ ; https://slsa.dev/ ; https://github.com/ossf/scorecard ; https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions ; https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability ; https://docs.github.com/en/code-security/dependabot

**Process and docs.**

https://developercertificate.org/ ; https://reuse.software/ ; https://spdx.dev/ ; https://semver.org/ ; https://keepachangelog.com/en/1.1.0/ ; https://www.conventionalcommits.org/en/v1.0.0/ ; https://adr.github.io/ ; https://vale.sh/ ; https://salsa.debian.org/rust-team/debcargo ; https://docs.fedoraproject.org/en-US/packaging-guidelines/Rust/
