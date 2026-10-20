# 04 — Google Cast, AirPlay/AirPlay 2, DLNA/UPnP, and open-source prior art

**Status:** Research complete, pre-implementation. **Date:** 2026-10-20.
**Scope:** Capability analysis of Google Cast (CASTV2), AirPlay/AirPlay 2 and DLNA/UPnP; survey of open-source implementations with licence and maintenance assessment; comparison matrices; a proposed provider-abstraction shape for AetherCast; and a legal-feasibility boundary (no DRM circumvention).

**Evidence labels.** **Verified** = read in a primary source (specification, source code, official documentation) during this pass. **Reported** = third-party claim, not reproduced. **Speculative** = inference or recommendation. Factual claims carry a numbered source `[n]`; full URLs in §13.

**Method and its limits.** I read primary protocol references (OpenAirPlay spec, pyatv's protocol notes, Google's Cast documentation, the UPnP Device Architecture 2.0 specification) and the *source* of the prior-art implementations rather than their READMEs. Repository metadata came from the GitHub API on **2026-10-20**.

**Two verification caveats.** First, this network's GitHub appears to be a partial mirror: `miniupnp/minidlna`, `occupiedpenguin/upmpdcli`, `thecubic/cast`, `google/go-chromecast` and `seppevs/gmrender-resurrect` returned HTTP 404 on 2026-10-20, while `hzeller/gmrender-resurrect` and `azatoth/minidlna` resolved. Where I could not fetch a project I say so rather than guessing. Second, the **DIAL specification** was unreachable on 2026-10-20, so DIAL is described only where running code demonstrates it.

---

## 1. Executive summary

- **Google Cast is the strongest second protocol for AetherCast.** It is a control-plane protocol; the receiver plays the media and holds the DRM client, the sender never touches decryption keys, and the wire format is a small protobuf+JSON envelope over TLS on 8009. Screen mirroring is not public, but **screen-as-video** is: encode the desktop and hand the receiver a URL. (verified [5][6][11][13])
- **AirPlay on Linux is solved for *receivers*, not senders.** UxPlay, RPiPlay, shairport-sync and `airplay2-receiver` are mature receivers; AetherCast needs a *sender*. FairPlay-protected video is a wall open implementations refuse to cross. (verified [26][28][29][31])
- **DLNA/UPnP is a rendering protocol, not a mirroring protocol** — it makes a renderer play a URL you give it. Cheapest to implement, widest device base, natural fallback for DLNA-only TVs. (verified [32][33][34])
- **Licences decide the reuse plan.** UxPlay and RPiPlay are GPLv3; `airplay2-receiver` ships **no licence file** and is not reusable without permission; `shairplay-rust` is LGPL-3.0; pychromecast MIT, catt BSD-2, mkchromecast MIT, go-chromecast Apache-2.0. (verified [13][15][16][17][26][27][28][29][31])
- **Recommendation:** add **Google Cast** after Miracast, **DLNA** third, and treat **AirPlay as receiver-side or partnered, not a v2 target** (§9).

---

## 2. Google Cast (CASTV2)

### 2.1 Roles and topology

A Cast *sender* (AetherCast) discovers receivers over mDNS, opens a control connection, asks the receiver to launch an application, then drives it over a namespace-based message protocol. The receiver runs the decoder and the DRM client; the sender supplies content as a URL the receiver fetches, usually from a local HTTP server. **The sender never receives media keys** — the key structural difference from Miracast, where the source encodes and pushes the elementary stream. (verified [2][3][11][13][17])

### 2.2 Discovery

Receivers advertise the mDNS/DNS-SD service type **`_googlecast._tcp`** (local domain `_googlecast._tcp.local.`). Cast *groups* advertise a sub-service **`_sub._googlecast._tcp.local.`**, which is how a sender distinguishes a group from a physical device; pychromecast filters on exactly that suffix. (verified [7])

TXT keys used in practice: `id` (device UUID, stable identity), `md` (model name, e.g. "Chromecast Ultra"), `fn` (friendly name). pychromecast classifies devices by `md` into `cast` (video+audio), `audio` (Google Home / Nest) and `group` — a hard-coded heuristic, not a protocol feature, so AetherCast will want its own taxonomy. (verified [7][11][13][17])

Cast devices also answer HTTP on **8008** and HTTPS on **8443**. pychromecast's `dial.py` queries `/setup/eureka_info?params=device_info,name` and `?params=multizone` to learn UUID, friendly name, model, cast type and multi-zone membership. `eureka_info` is Google-specific, not standard DIAL. (verified [8])

### 2.3 The CASTV2 transport

The control connection is **TLS to port 8009**. pychromecast uses `ssl.PROTOCOL_TLS_CLIENT` with **`verify_mode = ssl.CERT_NONE`** — the receiver presents a self-signed certificate and clients do not validate it. This is the de-facto behaviour of every open implementation, and a real security consideration (§9). (verified [5])

Each message is framed as a 4-byte big-endian length followed by a protobuf `CastMessage`, whose schema is small and stable: `protocol_version` (only value `CASTV2_1_0`), `source_id` (platform id is `receiver-0`), `destination_id`, `namespace`, `payload_type` (`STRING`/`BINARY`), `payload_utf8` (the JSON message) and `payload_binary`. So the envelope is **protobuf and the payload is JSON** — which is why every open implementation treats messages as JSON dictionaries while the SDK talks about protobuf. The same schema file defines `AuthChallenge`, `AuthResponse` and `DeviceAuthMessage`: the device-authentication handshake (signature algorithm, sender nonce, client auth certificate, CRL). (verified [6]) Naming trap: the namespace constants are `urn:x-cast:com.google.cast.*` while the schema family is `CASTV2`. (verified [5][6][9][10])

### 2.4 Namespaces

| Namespace | Direction | Purpose |
| --- | --- | --- |
| `urn:x-cast:com.google.cast.tp.connection` | both | channel open/close between a sender and a running app |
| `urn:x-cast:com.google.cast.tp.heartbeat` | both | liveness: `PING`/`PONG`, 10 s interval |
| `urn:x-cast:com.google.cast.receiver` | both | `GET_STATUS`, `LAUNCH`, `LAUNCH_ERROR`, `STOP`, volume |
| `urn:x-cast:com.google.cast.media` | both | `LOAD`, `PLAY`, `PAUSE`, `SEEK`, `STOP`, `GET_STATUS`, `VOLUME`, `MEDIA_STATUS` |
| app-specific | sender→app | channels the app declares; the receiver lists them in its status message |

Google's documentation confirms the media namespace string, the **64 KB maximum transport message size**, and that media-namespace messages are JSON with player states `IDLE`/`PLAYING`/`BUFFERING`/`PAUSED`. (verified [2]) The heartbeat is the cheapest liveness check available and every implementation uses it: pychromecast pings every 10 s and treats a missed pong as a dead connection. (verified [9])


### 2.5 Application IDs

Google assigns an application ID when a receiver app is registered in the Cast SDK Developer Console; the **Default Media Receiver** needs no registration and uses a fixed constant. pychromecast's `config.py` lists the well-known IDs, of which the important one is Default Media Receiver `CC1AD845` (others: Backdrop `E8C28D3C`, YouTube `233637DE`, DashCast `84912283`). (verified [11]) Registering your own receiver app requires a Google Cast Developer Console account and a $5 registration fee; publishing makes it available to all Cast devices worldwide. (verified [1])

### 2.6 The media path — what a "cast" actually is

A Cast sender does **not** push an elementary stream. The canonical flow for a local file: the sender runs a **local HTTP server** on a port of its choosing (byte-range capable); opens CASTV2 and sends `LAUNCH` for `CC1AD845`; sends `LOAD` with `contentId` = `http://<sender-ip>:<port>/<file>`; and thereafter sends only control messages while receiving `MEDIA_STATUS` updates. **The receiver fetches, decodes and plays the content itself.**

catt implements exactly this: its `http_server.py` is a `socketserver.TCPServer` on port **45114** serving the file with `bytes=` range handling, and the controller composes `http://{local_ip}:{port}/?loaded_from_catt`. go-chromecast does the same ("creating a http server that will stream the media file to the cast device"). **Ports 8008/8443 belong to the receiver**; the sender's local server port is free. (verified [8][13][17]) For remote content the sender hands over an HTTP(S) URL — catt delegates to `yt-dlp` for hundreds of sites. (verified [14][16])

### 2.7 Screen mirroring on Cast: what is real

Two different things are called "Chromecast screen mirroring":

- **Mirroring *from* a Cast device *to* another screen** (Chrome tab-cast, Google TV) is Google's proprietary sender path, with **no public specification** for a third-party sender and no entry in the Cast release notes. I could not verify a public "Cast Streaming" sender specification; treat any such claim as third-party at best.
- **Casting your desktop to a Cast device as video** is fully achievable by the media path above: capture, encode to something the receiver accepts, serve over HTTP, `LOAD` it into the Default Media Receiver. The **receiver** decodes, so AetherCast is not bound by the Miracast codec constraints. catt uses this to "render websites"; mkchromecast to cast video files. (verified [13][14][16])


**Consequence:** Cast support is not a mirroring protocol in the Miracast sense; it is "push a URL, receiver renders" — architecturally close to DLNA. §6 and §7 account for this.

### 2.8 Codecs and DRM

The Web Receiver player supports **DASH, HLS and Smooth Streaming**, and `canDisplayType` lets the sender query what the device can play. DRM is expressed as a `ContentProtection` with `protectionSystem = WIDEVINE` and a `licenseUrl` — the **receiver** performs the Widevine licence request. (verified [2][3])

That is the DRM boundary: the licence request and key handling live in the receiver's own DRM stack, and a sender that merely names a Widevine-protected manifest never handles a key. **There is no circumvention surface for AetherCast in the Cast path**, provided it never tries to decrypt anything itself. (verified [3])

### 2.9 Grouping / multi-room

Cast groups are first-class device kinds: they appear as `_sub._googlecast._tcp` sub-services and can be driven as one logical target. pychromecast fetches membership through the `multizone` endpoint; mkchromecast supports multi-room group playback. (verified [7][8][16])

### 2.10 Sequence diagram — Cast media cast

```
 AetherCast (sender)                Cast receiver (:8009 TLS)
        |--- mDNS query _googlecast._tcp --->|
        |<-- TXT {id=<uuid>, md=<model> -----|
        |==== TLS connect, port 8009 =======>|
        |--- PING (ns=tp.heartbeat) --------->|
        |<-- PONG ----------------------------|
        |--- LAUNCH app_id=CC1AD845 -------->|
        |<-- LAUNCH / GET_STATUS ------------|
        |   [local HTTP server on :45114]    |
        |--- LOAD {contentId:                |
        |      "http://10.0.0.5:45114/v.mp4",|
        |      mediaType:"video/mp4"} ------->|
        |                                    |--- HTTP GET /v.mp4 --->
        |<-- MEDIA_STATUS {playerState:      |   (sender serves bytes)
        |      BUFFERING} -------------------|
        |--- PLAY -------------------------->|--- decode + output --->
        |<-- MEDIA_STATUS {PLAYING, currentTime, volume} --|
```

## 3. AirPlay / AirPlay 2

### 3.1 Protocol family

AirPlay is a family: AirTunes (2004) → AirTunes v2 (2010) → AirPlay v1 (2010) → AirPlay v2 (2018). Two services are advertised: **`_airplay._tcp`** (photo/video, later audio) and **`_raop._tcp`** (Remote Audio Output Protocol, superseded by the AirPlay service in later versions). (verified [21][22])

### 3.2 Discovery

From the OpenAirPlay specification (verified [22]):

```
name: Apple TV
type: _airplay._tcp
port: 7000
txt:
  deviceid=58:55:CA:1A:E2:88
  features=0x39f7
  model=AppleTV2,1
  srcvers=130.14
```

`_airplay._tcp` TXT keys: `model`, `manufacturer`, `serialNumber`, `fv`, `osvers`, `deviceid`, `features` (32-bit, later 64-bit, as two comma-separated hex words), `pw` (password protected), `acl`, `srcvers`, `flags`, `pk`, `pi`/`psi`/`gid` (pairing and group identifiers), `igl`/`gcgl` (group roles), `protovers`. (verified [22][21])

`_raop._tcp` TXT keys (verified [21][22]):

The `_raop._tcp` TXT record carries `et` (encryption type: `0` unencrypted, `1` RSA/AirPort Express, `3` FairPlay, `4` MFiSAP, `5` FairPlay SAP v2.5), `cn` (codecs: `0` PCM, `1` ALAC, `2` AAC, `3` AAC-ELD, `4` OPUS), `ss`/`sr`/`ch` (sample size / rate / channels), `am` (model), `tp` (transport), `pw` (password protected; triggers `POST /pair-pin-start`), `vn` (version), `md` (metadata: 0 text, 1 artwork, 2 progress), `sv`/`sm` (software volume/mute), `pk` (public key) and `ft`/`sf` (feature/system bitmasks). (verified [21][22])

The `_airplay._tcp` `features` bitfield is the negotiation surface. The decision-relevant bits: 0 Video, 4 VideoHTTPLiveStreams, **7 Screen (mirroring)**, 9 Audio, **2 VideoFairPlay**, 12 FPSAPv2pt5_AES_GCM, 14 Authentication4 (FairPlay auth), 19/20 AudioFormat1/2 ("must be set for AirPlay 2 connection to work"), 38 SupportsCoreUtilsPairingAndEncryption, **40 SupportsBufferedAudio** and **41 SupportsPTP** (both needed for multi-room), 51 SupportsUnifiedPairSetupAndMFi. (verified [25])

### 3.3 AirPlay 1 mirroring

The OpenAirPlay spec is unusually explicit (verified [23][24]):

- Mirroring **does not use the standard AirPlay service**; it connects to an apparently hard-coded **port 7100**, an HTTP server.
- `GET /stream.xml` returns an XML plist with `height`, `width`, `overscanned`, `refreshRate`, `version`.
- `POST /stream` sends a binary property list with `deviceID`, `sessionID`, `version`, `param1` (**72 bytes: the AES key, encrypted with FairPlay**), `param2` (16-byte AES IV), `latencyMs`, `fpsInfo[]`, `timestampInfo[]`. After this the connection is no longer valid HTTP.
- The video is **H.264 over that TCP connection, packetised with a 128-byte header**; **AAC-ELD audio** travels over the AirTunes protocol; the **master clock is NTP-synchronised** (the receiver sends NTP requests to the client on port **7010**).
- `param1`/`param2` are optional — unencrypted mirroring is possible.

This framing is the key fact for AetherCast: **AirPlay mirroring is a receiver-side decoding problem**. Receiving means demuxing a proprietary framing; sending means reproducing it exactly.

### 3.4 AirPlay 1 video/photo (the "don't re-encode" path)

AirPlay *video playback* (as opposed to mirroring) works by the client handing the receiver a **URL**, so the receiver fetches and decodes the content itself and the client never transcodes: "as soon as a client starts a video playback, a standard AirPlay connection is made to send the video URL, and mirroring is stopped. This avoids decoding and re-encoding the video". (verified [23]) UxPlay supports this in a limited form (`-hls`), currently only for YouTube HLS. (verified [26])

### 3.5 RAOP / AirTunes audio

RAOP is RTSP-driven: `OPTIONS` (capability probe), `ANNOUNCE` (SDP with codec and encryption), `SETUP` (RTP ports), `RECORD`, `FLUSH`, `TEARDOWN`, `SET_PARAMETER` (volume), plus `/auth-setup` and `/pair-pin-start`. Codecs are PCM, ALAC, AAC, AAC-ELD and OPUS. (verified [21][22])

### 3.6 AirPlay 2

AirPlay 2 is where the complexity lives (verified [21][28][29]):

- **Pairing** reuses HomeKit Accessory Protocol (HAP): SRP-6a, X25519, Ed25519, with transient (PIN) and non-transient pairings. (verified [28])
- **Encryption**: every channel uses **ChaCha20-Poly1305**, session keys derived with **HKDF** from the HAP shared secret. Blocks are `size (2 bytes, little-endian) || data || auth tag (16 bytes)`. (verified [21])
- **Channels**: a control channel (RTSP), an event channel and a data channel; the data channel carries **MRP (Media Remote Protocol) protobuf** messages — the set the Apple TV remote uses. (verified [21])
- **Multi-room**: buffered audio (bit 40), PTP tight sync (bit 41), 5.1/7.1 multichannel. `airplay2-receiver` handles REALTIME and BUFFERED audio and "output latency compensation for sync with other Airplay receivers", but explicitly **not** accurate audio sync (PTP/NTP pending). (verified [28])
- **Latency**: shairport-sync documents classic AirPlay sources at a latency of around **2.0–2.25 seconds**, AirPlay 2 at around **half a second**. Classic AirPlay uses an NTP variant; AirPlay 2 uses PTP via `nqptp`. (verified [30])

### 3.7 FairPlay: the hard boundary

Here "realistic" and "legal" diverge:

- FairPlay-protected video is an advertised *feature*: `features` bit 2 = `VideoFairPlay`, bit 12 = `FPSAPv2pt5_AES_GCM`, bit 14 = `Authentication4`; `_raop._tcp` `et` = `3` (FairPlay) / `4` (MFiSAP) / `5` (FairPlay SAP v2.5). (verified [25][21])
- The mirroring `POST /stream` payload carries a **FairPlay-encrypted AES key** (`param1`, 72 bytes); producing it requires Apple's FairPlay key-exchange machinery. (verified [24])
- **UxPlay states the boundary plainly**: "Apple video-DRM (as found in 'Apple TV app' content on the client) cannot be decrypted by UxPlay, and the Apple TV app cannot be watched using UxPlay's AirPlay Mirror mode (only the unprotected audio will be streamed, in AAC format)." (verified [26])
- **RPiPlay states the legal uncertainty plainly**: "This project makes use of a third-party GPL library for handling FairPlay. The legal status of that library is unclear. Should you be a representative of Apple and have any objections against the legality of the library and its use in this project, please contact me and I'll take the appropriate steps." (verified [31])
- `airplay2-receiver` implements "FairPlay (v3) authentication and decryption of AES keys", not FairPlay v2, and notes it "may never implement MFi Authentication (requires MFi hardware module)". (verified [28])

**AetherCast rule:** never implement FairPlay key derivation, FairPlay SAP, or any key-offering service. The defensible line is *receive and play content the sender offers without DRM*; refuse FairPlay-protected streams; never mint or relay FairPlay keys. A future product needing FairPlay has one clean route — Apple's MFi programme with the required hardware — out of scope for an open-source desktop app. (speculative, informed by [26][28][31][37])

### 3.8 What is realistic on Linux

| Goal | Feasible? | Evidence |
| --- | --- | --- |
| AirPlay **receiver** for mirroring | **Yes, solved.** UxPlay (GPLv3, ★3235, active) renders iOS/macOS screen mirroring via GStreamer with HW h264/h265 decode. | [26][27] |
| AirPlay 2 **audio receiver** | **Yes.** shairport-sync (★8902, active) plays AirPlay 1 and 2 audio including 24-bit/48 kHz and 5.1/7.1. Apple's own consumer page describes AirPlay 2 as adding multi-room sync and 4K/HDR video to AirPlay. | [30][36] |
| AirPlay **sender** (Linux → AirPlay TV) | **Partial.** pyatv documents the protocols and implements an Apple TV client; mirroring sender support is limited. A sender must implement the RTSP client side, the pairing handshake and the mirroring framing. | [20][21] |
| AirPlay 2 **multi-room** | **Partial.** `airplay2-receiver` implements "some multi-room features"; UxPlay says multi-room audio "is not supported: use shairport-sync for that". | [26][28] |
| Play **FairPlay-protected** video | **No.** §3.7. | [26][31] |

Note the direction reversal: Linux's AirPlay strength is **receiving**. AetherCast's product is a **sender**. That is the crux of the AirPlay risk (§9).

---

## 4. DLNA / UPnP

### 4.1 Architecture

UPnP is defined by the **UPnP Device Architecture** specification (v2.0 PDF at `upnp.org`, fetched 2026-10-20) in three layers (verified [32][33][34]):

**Discovery — SSDP**: devices announce and are found by multicast search over **UDP port 1900**, advertising e.g. `urn:schemas-upnp-org:device:MediaRenderer:1`. **Description — HTTP**: the device serves an XML *device description* listing services, `SCPDURL`s and the control URL. **Control — SOAP**: control points invoke actions over SOAP/HTTP against `/upnp/control/<serviceType>/<serviceId>` and subscribe to events (GENA) for state variables such as `TransportState`.

gmrender-resurrect is a clean implementation of exactly this, built on **libupnp** (`UpnpInit2`, `UpnpEnableWebserver`, `UpnpAddVirtualDir("/upnp")`, `UpnpSendAdvertisement`). (verified [32][33])

### 4.2 Services

Service type URNs are stable and confirmed in gmrender-resurrect's source (verified [33]):

The service URNs are stable and confirmed in gmrender-resurrect's source (verified [33]): `urn:schemas-upnp-org:service:AVTransport:1` (`SetAVTransportURI`, `Play`, `Pause`, `Stop`, `Seek`, `GetPositionInfo`, `GetTransportInfo`), `urn:schemas-upnp-org:service:RenderingControl:1` (volume, mute, channel), `urn:schemas-upnp-org:service:ConnectionManager:1` (prepare-for-connection) and `urn:schemas-upnp-org:service:ContentDirectory:1` (`Browse`, `Search` over a content hierarchy), on a `urn:schemas-upnp-org:device:MediaRenderer:1` device. gmrender-resurrect implements the first three but **not** ContentDirectory — it plays whatever URL the control point sets.

### 4.3 Media rendering vs. screen mirroring

DLNA/UPnP-AV is **media rendering only**. `SetAVTransportURI` takes a URL and the renderer pulls it. The AVTransport/RenderingControl service set has no mechanism for pushing a live encoded elementary stream to a renderer, and none for accepting a screen. (verified [33][34])

So DLNA is excellent for **"play this file/URL on that TV"** and **useless for screen mirroring** without an out-of-band extension that DLNA-certified devices will not accept. The base specification has no DRM mechanism, so "DRM" here reduces to whether the URL points at protected content and whether the renderer can handle it. (verified [34]; DRM specifics unverified)

### 4.4 DIAL

DIAL (Discovery and Launch) is the UPnP-adjacent protocol Google and Microsoft specified to launch apps on TVs; Chromecast implements it, which is why "cast to TV" and "DLNA to the same TV" overlap. **I could not reach the DIAL specification on 2026-10-20**, so I can only report what running code shows: pychromecast queries Google's `eureka_info` endpoints, not the standard DIAL `apps` endpoint. (verified [8]; DIAL spec itself **not verified**)

### 4.5 When DLNA is useful, and its limits

**Useful when:** the TV advertises DLNA but neither Cast nor AirPlay (common on budget TVs); the content is a file or URL the renderer can fetch; you want *zero* encoder work; or you are targeting DLNA/DMR receivers on the LAN as "cast my desktop as video" targets via the same URL-push trick as Cast. (verified [15][32][35])

**Limits:** no mirroring; little remote control beyond RenderingControl; renderer codec support is unknown until playback fails; UPnP is LAN-only, unauthenticated and unencrypted — a security footnote given hostile control points on the same LAN. (verified [34])

---

## 5. Open-source prior art

Repository metadata below is from the GitHub API on **2026-10-20**.

### 5.1 Google Cast

| Project | Licence | Last push | Reuse assessment |
| --- | --- | --- | --- |
| [pychromecast](https://github.com/home-assistant-libs/pychromecast) (Python) | MIT | 2026-10-20 | The reference implementation; cleanest layering (TLS+framing, controllers, mDNS, DIAL). **Best source of truth; MIT ⇒ reusable.** [5][6][7][8][9][10][11][12] |
| [catt](https://github.com/skorokithakis/catt) (Python) | BSD-2-Clause | 2026-09-24 | The best *product*: CLI + local HTTP media server (port 45114) + pychromecast + `yt-dlp`. Reusable under BSD-2. [13][14] |
| [mkchromecast](https://github.com/muammar/mkchromecast) (Python) | MIT | 2026-10-13 | Tray app casting Linux audio (`parec`/PulseAudio/ALSA) and video files to Cast **and Sonos**. [16] |
| [go-chromecast](https://github.com/vishen/go-chromecast) (Go) | Apache-2.0 | 2026-10-12 | CLI for Cast devices and Home groups; local-file playback via its own HTTP server; `ffmpeg` fallback. [17] |
| [castnow](https://github.com/xat/castnow) (Node.js) | MIT | 2022-12-06 | **Archived.** The author abandoned Cast because his new TV supported DLNA directly — a signal that DLNA-only TVs are common. [15] |
| [rust-cast](https://github.com/azasypkin/rust-cast) (Rust, `rust_cast` crate) | MIT | 2025-12-30 | Rust Cast library, on crates.io as `rust_cast`. **Most reusable Cast library for a Rust project.** [18][39] |
| [oxicast](https://github.com/denniskribl/oxicast) (Rust) | Apache-2.0 | 2026-05-27 | Async (tokio) Cast client. Very new — watch, don't depend. [19][39] |

### 5.2 AirPlay

| Project | Licence | Last push | Reuse assessment |
| --- | --- | --- | --- |
| [UxPlay](https://github.com/FDH2/UxPlay) (C) | **GPL-3.0** | 2026-10-20 | Most complete AirPlay receiver: mirroring + ALAC audio + HLS, internal mDNS, BLE discovery beacon, GStreamer HW decode. **GPL-3.0 ⇒ not linkable into a permissive AetherCast.** [26][27] |
| [RPiPlay](https://github.com/FD-/RPiPlay) (C++) | **GPL-3.0** | 2023-04-14 | AirPlay mirroring server for the Pi; origin of UxPlay's mirroring code. Carries the FairPlay legal caveat. **Last push 2023 ⇒ frozen.** [31] |
| [shairport-sync](https://github.com/mikebrady/shairport-sync) (C) | per-file; `COPYING` defers to source files | 2026-10-19 | The reference AirPlay **audio** receiver, AirPlay 1 and 2, PTP via `nqptp`. Very actively maintained. [30] |
| [airplay2-receiver](https://github.com/openairplay/airplay2-receiver) (Python) | **none — no LICENSE file** | 2026-06-29 | Most complete public AirPlay 2 receiver: HAP pairing, FairPlay v3 key decryption, buffered audio, multichannel. **Default copyright ⇒ not licensed for reuse.** The canonical reference shairport-sync credits. [28] |
| [shairplay-rust](https://github.com/metaneutrons/shairplay-rust) (Rust) | **LGPL-3.0** | 2026-09-11 | Clean-room Rust AirPlay 1 + 2 receiver library; `#![forbid(unsafe_code)]`. **Pre-1.0, tiny adoption, friendliest licence for a permissive AetherCast.** [29][39] |
| [pcairplay](https://github.com/gbulog/pcairplay) (PowerShell) | MIT | 2026-07-24 | Windows-focused iPhone→PC mirroring. MIT. [39] |
| [pyatv](https://github.com/postlund/pyatv) (Python) | MIT | 2026-08-14 | AirPlay **client** library; its protocol notes are the best free public prose on AirPlay 2 internals. MIT; excellent *sender-side* reference. [20][21] |
| [node-chromecast](https://github.com/thecubic/cast) (Node.js) | MIT | — | Widely cited but **not reachable from this network on 2026-10-20**; licence not re-verified. (not verified) |

### 5.3 DLNA / UPnP

| Project | Licence | Last push | Notes |
| --- | --- | --- | --- |
| [gmrender-resurrect](https://github.com/hzeller/gmrender-resurrect) (C) | GPL-2.0 | 2026-02-13 | Headless UPnP/DLNA **renderer** on libupnp + GStreamer. The canonical "turn a box into a DLNA TV" project. [32][33] |
| [MiniDLNA](https://minidlna.com/) (C) | GPL (per its LICENCE file) | 2020-11-16 (mirror) | DLNA **server**. Upstream `miniupnp/minidlna` **not reachable** from this network on 2026-10-20; `azatoth/minidlna` mirrors the SourceForge tree. [35] |
| [upmpdcli](https://github.com/occupiedpenguin/upmpdcli) (C++) | GPL-2.0 (per mirrors) | — | UPnP front-end for MPD — a **control point** exposing a music daemon as a renderer. Upstream not reachable here. (partially verified [39]) |
| [crab-dlna](https://github.com/gabrielmagno/crab-dlna) (Rust) | MIT **or** Apache-2.0 (dual) | 2024-09-04 | Minimal UPnP/DLNA media streamer in Rust; dual-licensed ⇒ **most permissive DLNA option for AetherCast**. [39] |
| [dlnacast](https://github.com/xat/dlnacast) (Node.js) | MIT | 2024-04-15 | "Cast local media to your TV through UPnP/DLNA" — the DLNA analogue of catt, and the pointer castnow's author gave for AetherCast's use case. [39] |

### 5.4 Licence compatibility for AetherCast

**Reusable as-is** (attribution and notice clauses): MIT/BSD-2/Apache-2.0 — pychromecast, catt, mkchromecast, go-chromecast, rust-cast, oxicast, pyatv, dlnacast, crab-dlna. [13][14][15][16][17][18][19][20][39] **Reusable with care**: LGPL-3.0 shairplay-rust, if dynamically linked with relinkability preserved; not if copied into the binary. [29] **GPL-2.0/GPL-3.0** — UxPlay, RPiPlay, gmrender-resurrect, shairport-sync, MiniDLNA, upmpdcli — usable only if AetherCast is GPL, otherwise clean-room reimplementation. [27][30][31][32][33][35] **No licence** — airplay2-receiver: default copyright applies, so there is no permission to copy or redistribute. Read it for understanding; do not reuse it. [28]

## 6. Comparison matrix

| Dimension | Miracast / WFD | Google Cast | AirPlay / AirPlay 2 | DLNA / UPnP |
| --- | --- | --- | --- | --- |
| Discovery | Wi-Fi Direct GO Negotiation; no IP-level discovery (note 01) | mDNS `_googlecast._tcp` (+ `_sub._googlecast._tcp` for groups); also answers on 8008/8443 [7][8] | mDNS `_airplay._tcp` + `_raop._tcp` [22] | SSDP over UDP 1900 [32][34] |
| Direction | Source pushes elementary stream | Sender pushes **commands**; receiver **pulls** content | Sender pushes stream (mirror) or URL (video) | Control point pushes URL; renderer pulls |
| Screen mirroring | **Yes, native** | **No public spec**; screen-as-video via URL push [13][16] | **Yes** (AP1: H.264/TCP + 128-byte header) | **No** |
| Media casting | Limited | **Yes, first-class** (`LOAD` + Default Media Receiver `CC1AD845`) [2][11] | Yes (video via URL; audio via RAOP) | Yes (`SetAVTransportURI`) |
| Audio | AAC-LC in the TS; LPCM mandatory | Receiver's own audio (Cast audio devices) | ALAC / AAC / AAC-ELD / OPUS / PCM; multichannel in AP2 [21][28] | Receiver's own |
| DRM | HDCP 2.2 link protection | Widevine **on the receiver**; sender never sees keys [3] | **FairPlay** — open implementations do not decrypt protected video [26][31] | None in the base spec |
| Typical latency | ~100–250 ms (note 03) | Seconds (receiver buffers, pulls, renders) | AP1 ≈ 2.0–2.25 s; AP2 ≈ 0.5 s [30] | Seconds (playback start-up) |
| Encoder work at the sender | **Required** (H.264 CBP, 1 slice/picture) | Only when turning the screen into a file | Only for mirroring, and then it is the *sender's* problem in AP, not ours | **None** |
| Authentication | none in WFD itself | device-auth handshake (cert + signature + CRL in the proto schema) [6] | HAP pairing (SRP-6a), PIN or persistent; ChaCha20-Poly1305 channels [21][28] | **None** (LAN, plaintext SOAP) |
| Legal risk | Low | Low–Medium (proprietary protocol; Cast SDK ToS binds SDK use, not the wire protocol) [4] | **High** (FairPlay, MFi) | Low |

## 7. Proposed provider abstraction for AetherCast

### 7.1 The shape of the problem

The four protocols occupy **three different transport models**. **Push-stream** (Miracast, AirPlay mirroring): the source owns encode, mux, pacing and the media plane; AetherCast is the *server*. **Control-plane / pull** (Google Cast, DLNA): the source owns *what* to play and *where* it lives; the receiver owns decode, DRM and rendering; AetherCast is a *control client* plus, sometimes, a small HTTP origin.

An interface built around "here is an encoded stream, push it" fits Miracast and AirPlay and fits Cast and DLNA not at all. The interface must be built around **intent** ("play this thing on that device"), with a separate notion of *who renders*.

### 7.2 Proposed core interfaces

```rust
trait DeviceProvider {
    fn id(&self) -> &str;                    // "miracast", "cast", "airplay", "dlna"
    async fn discover(&self) -> Vec<Device>;
    fn start(&self) -> Result<()>;           // start mDNS / SSDP / P2P listeners
    fn stop(&self);
}

struct Device {
    provider: ProviderId,
    stable_id: String,          // uuid / deviceid / UDN
    display_name: String,
    model: Option<String>,
    transport: TransportKind,   // WifiDirect | LanIp
    endpoint: Endpoint,         // provider-specific handle, opaque to the core
    capabilities: Capabilities,
    group: Option<GroupRef>,
}

struct Capabilities {
    mirroring: MirrorSupport,   // None | ScreenStream | ScreenAsVideo | Url
    media: MediaSupport,        // None | UrlOnly | PushStream
    audio: AudioSupport,        // None | ReceiverAudio
    drm: DrmSupport,            // ReceiverHandled | None | Unsupported
    max_video: Option<VideoCaps>,
    remote_control: bool,
    grouping: bool,
}

trait CastSession {
    fn state(&self) -> SessionState;
    async fn launch(&mut self, target: &Device) -> Result<()>;
    async fn play(&mut self, request: PlayRequest) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn set_volume(&mut self, v: f32) -> Result<()>;
    fn events(&self) -> impl Stream<Item = SessionEvent>;
}

enum PlayRequest {
    PushStream { pipeline: EncodedSource, params: PushParams }, // provider owns encode+mux
    PlayUrl { url: Url, mime: String, metadata: Meta },        // provider owns nothing
    LaunchApp { app_id: String, args: Option<Value> },         // e.g. Cast's CC1AD845
}
```

### 7.3 What must stay provider-specific

- **The wire protocol and media transport** — protobuf/JSON for Cast, RTSP+plist for AirPlay, SOAP+SSDP for DLNA, RTP/TS over RTSP for Miracast. Cast and DLNA have *no* sender-side media transport; AirPlay and Miracast have everything.
- **Discovery semantics** — mDNS service type + TXT keys vs. SSDP `ST` + description XML vs. Wi-Fi Direct peer negotiation.
- **Authentication** — Cast device-auth, AirPlay HAP pairing, DLNA none, Miracast none.
- **Remote control and DRM posture** — Cast has a rich control plane and delegates to the receiver's Widevine; DLNA has AVTransport state; Miracast has UIBC; AirPlay has MRP but is blocked at FairPlay.

### 7.4 What must be shared

- The **capture + encode** pipeline from note 03 — consumed only by the push-stream providers.
- **A local HTTP origin** (the `catt` pattern) — needed by Cast, DLNA and AirPlay-video, and the same component in all three.
- **Device list UI, favourites, groups**, and the session state machine from note 05.
- **A single "what should I play?" model** — the `PlayRequest` enum above.

### 7.5 The one abstraction that breaks: "mirroring"

Miracast mirroring is a *continuous encoded stream*; Cast/DLNA "mirroring" is *a URL pointing at a file or an HTTP server*. The UI must not promise "mirroring" when the provider is serving a file: you can pause and scrub a file, you cannot scrub a live screen capture, and a buffering receiver will drift.

**Recommendation:** expose two distinct intents — **"Share screen"** (push-stream providers only) and **"Cast"** (URL/origin providers) — and label a screen-capture cast as *casting a video of your screen*, with a "this is not live" warning when the origin is a growing file. (speculative)

---

## 8. Legal feasibility summary

| Area | Position | Basis |
| --- | --- | --- |
| Miracast/WFD, DLNA/UPnP | Both fully implementable from published specifications (note 01, §4). HDCP is a link-protection boundary to respect, not circumvent; the DLNA base spec has no DRM mechanism. | note 01 [32][34] |
| Google Cast | The wire protocol is reverse-engineered, not published; open implementations are permissively licensed and long-standing. Using Google's *Cast SDK* is subject to the Cast ToS; using the protocol directly is what pychromecast, catt, mkchromecast and go-chromecast do. **AetherCast should not ship Google's SDK binaries** — it should implement the protocol. | [1][4][13][15][16][17] |
| Cast DRM | The sender never handles keys; the receiver performs the Widevine licence request. **No circumvention needed and none should be attempted.** | [3] |
| AirPlay mirroring (unencrypted) | Implementable; open-source receivers have done it for a decade. | [21][23][26] |
| AirPlay + FairPlay | **Blocked.** Key derivation is Apple's proprietary and cryptographically gated; open projects disclaim protected video and one carries an unresolved legal caveat. **AetherCast must not implement FairPlay.** | [26][28][31] |
| AirPlay + MFi | A *hardware* AirPlay receiver needs Apple's MFi programme and MFi authentication hardware; a software-only desktop app is not an MFi accessory. | [28][37] |
| Patents | H.264/HEVC/AAC/ALAC encode and decode sit in patent pools; shipping codecs in a distributed product attracts royalty obligations regardless of protocol (note 03). Avoid *shipping* codecs — depend on system/GStreamer/FFmpeg. | [40] |

## 9. Recommendation: what to add after Miracast

**Add Google Cast next.** In order of weight:

- **User value.** Chromecast / Google TV / Google Home are the most common casting targets alongside AirPlay; one provider covers video casting, audio-only targets and multi-room grouping. (verified [7][11][16])
- **Effort.** The protocol is small — TLS to 8009, a 7-field protobuf envelope, JSON payloads, four namespaces, a heartbeat and an app-launch handshake — so a working client is a few hundred lines. **It is the cheapest provider that is not "push a URL".** (verified [5][6][9][10])
- **Risk.** Lowest of the three: no DRM in the sender path, no key derivation, no certification requirement, and a permissive-licence prior art pool reusable outright. (verified [3][13][16][17])
- **Architectural fit.** It forces the provider model to distinguish "who renders" — the distinction Miracast alone would never have forced.


**Then DLNA/UPnP**: lowest effort, widest non-Cast/non-AirPlay device base (castnow's author abandoned Cast because his new TV only did DLNA), and media-rendering only, so it slots in as a clean "play a URL" provider needing little more than a SOAP client and an SSDP listener. (verified [15][32][35])

**Then AirPlay — as a receiver-side or partnered project, not an AetherCast provider.** The realistic Linux AirPlay work is *receiving* (UxPlay, shairport-sync), already solved, well-licensed (GPL/LGPL) and actively maintained. If AetherCast ever needs to *send* to AirPlay (Linux → Apple TV), the realistic order is: **AirPlay audio first** via the RAOP client side (much simpler than mirroring, with real user value — Linux → HomePod / AirPlay speakers); then **mirroring sender** as a research project with an explicit no-FairPlay policy; and possibly the **LGPL-3.0** Rust receiver library (`shairplay-rust`) as a *separate optional component*, since LGPL and the provider boundary do not mix cleanly. (verified [29])

**Defer** AirPlay 2 multi-room: both reference implementations flag incomplete sync and unsupported multi-room. (verified [26][28])

---

## 10. Bottom line for AetherCast

Google Cast should be provider #2, implemented as a **control-plane provider** — AetherCast launches the Default Media Receiver and serves content from a local HTTP origin — **not** as a push-stream provider. That keeps the provider interface honest: Miracast and AirPlay mirroring own the media plane; Cast and DLNA do not.

The most important design consequence is to stop treating "mirroring" as one concept. The UI and the provider interface should distinguish **streaming a live encoded surface to a peer** (Miracast, AirPlay mirroring) from **asking a device to render something you host** (Cast, DLNA, AirPlay video). Everything else — discovery shell, session state machine, device list, favourites — is genuinely shared.

On legality: **the Cast and DLNA paths are clean** and AetherCast should ship both without hesitation. **The AirPlay path has a hard line at FairPlay**, and that line must be in the contributor guidance *before* any AirPlay work starts, because the best public AirPlay 2 reference implements precisely the part that must not be copied. Licences are the second line: MIT/BSD/Apache prior art exists for Cast and (via `crab-dlna`/`dlnacast`) for DLNA; the AirPlay prior art is GPL or unlicensed, so AirPlay must be clean-room or delegated.


---

## 11. Open questions

1. **Does the Cast protocol have a stable public spec?** Google's docs cover the *SDK*, not the wire protocol, and the group/multizone endpoints (`eureka_info`) are undocumented. Is there an official "Cast Streaming" sender specification third parties may implement? (unresolved — `developers.google.com/cast/docs/cast_streaming` returned 404 on 2026-10-20)
2. **What is the Miracast → Cast crossover?** If AetherCast encodes once for Miracast (H.264 CBP, MPEG-TS), can the same elementary stream be served to a Cast receiver over HTTP? Cast wants a container it can demux (MP4/HLS/DASH), so probably a *second* mux. Needs measurement.
3. **Cast receiver-side latency and drift.** A Cast receiver pulling a growing file from an HTTP origin — what does it do when the file has no fixed duration, and does it need `Content-Length`? (no verified source; needs an experiment on real hardware)
4. **AirPlay sender feasibility.** Is there a maintained open-source AirPlay *sender* that can push iOS screen content to a third-party receiver, or would AetherCast have to be the receiver (UxPlay) and interop with it? (no sender-side prior art found in this pass)
5. **DLNA client coverage and DIAL.** Which real TVs in 2026 still ship DLNA-only? `dlnacast` and castnow's README suggest a meaningful population, but no market data was gathered. And what does DIAL mandate for app launching on DMRs? (spec unreachable on 2026-10-20)
6. **Cast audio-only targets and licence decision.** Should AetherCast cast *system audio* to a Cast audio device (the mkchromecast use case [16]), and does that fit the same `PlayRequest` model? And will AetherCast be permissively licensed (MIT/Apache) or GPL? The answer changes which AirPlay/DLNA prior art can be reused at all.
---

## 12. Risks

1. **The Cast protocol is not published.** Everything here is reverse-engineered; a Google protocol change breaks senders silently — as it did for the group/multizone handling pychromecast had to add. (reported [7])

2. **AirPlay's strength is on the wrong side of the product.** Linux's AirPlay ecosystem is receivers; AetherCast is a sender, and a mirroring *sender* has thin open prior art. (verified [20][21]; risk assessment speculative)
3. **FairPlay is a legal tripwire.** Even *documenting* or *reusing* FairPlay key handling would put AetherCast in the same category as RPiPlay's "legal status … is unclear" note — this must be written project policy. (verified [26][31])
4. **Licence contamination.** UxPlay/RPiPlay/gmrender are GPL: reading them for protocol understanding is fine, copying is not. The `airplay2-receiver` no-licence trap is easy to fall into because it is the best public AirPlay 2 reference. (verified [27][28][29][31])
5. **LAN security.** UPnP is unauthenticated; Cast's TLS is not certificate-verified by convention; AirPlay pairing is strongest of the three. Bind discovery to the active LAN and do not expose an HTTP origin beyond it. (verified [5][34]; recommendation speculative)
6. **"Mirroring" semantics diverge.** One "Mirror" button across providers yields three different experiences; Cast device types are also inferred from the `md` model string, a heuristic that breaks with new hardware. (speculative, §7.5)

---

## 13. Sources

**Google Cast (official)**
1. Cast — Registration: https://developers.google.com/cast/docs/registration
2. Cast — Media Playback Messages: https://developers.google.com/cast/docs/media/messages
3. Cast — Web Receiver Player Streaming Protocols: https://developers.google.com/cast/docs/media/streaming_protocols
4. Cast — Terms of Service: https://developers.google.com/cast/docs/terms

**Google Cast implementations**
5. pychromecast `socket_client.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/socket_client.py
6. pychromecast `generated/cast_channel_pb2.pyi`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/generated/cast_channel_pb2.pyi
7. pychromecast `discovery.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/discovery.py
8. pychromecast `dial.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/dial.py
9. pychromecast `controllers/heartbeat.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/controllers/heartbeat.py
10. pychromecast `controllers/receiver.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/controllers/receiver.py
11. pychromecast `config.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/config.py
12. pychromecast `controllers/media.py`: https://github.com/home-assistant-libs/pychromecast/blob/master/pychromecast/controllers/media.py
13. catt: https://github.com/skorokithakis/catt
14. catt README: https://github.com/skorokithakis/catt/blob/master/README.md
15. castnow: https://github.com/xat/castnow
16. mkchromecast: https://github.com/muammar/mkchromecast
17. go-chromecast: https://github.com/vishen/go-chromecast
18. rust-cast: https://github.com/azasypkin/rust-cast
19. oxicast: https://github.com/denniskribl/oxicast

**AirPlay**
20. pyatv: https://github.com/postlund/pyatv
21. pyatv protocol notes: https://pyatv.dev/documentation/protocols/
22. OpenAirPlay spec — Service Discovery: https://github.com/openairplay/airplay-spec/blob/master/src/service_discovery.md
23. OpenAirPlay spec — Screen Mirroring: https://github.com/openairplay/airplay-spec/blob/master/src/screen_mirroring/README.md
24. OpenAirPlay spec — mirroring HTTP requests: https://github.com/openairplay/airplay-spec/blob/master/src/screen_mirroring/http_requests.md
25. OpenAirPlay spec — Features bitfield: https://github.com/openairplay/airplay-spec/blob/master/src/features.md
26. UxPlay README: https://github.com/FDH2/UxPlay/blob/master/README.md
27. UxPlay LICENSE: https://github.com/FDH2/UxPlay/blob/master/LICENSE
28. openairplay/airplay2-receiver: https://github.com/openairplay/airplay2-receiver
29. shairplay-rust: https://github.com/metaneutrons/shairplay-rust
30. shairport-sync README: https://github.com/mikebrady/shairport-sync/blob/master/README.md
31. RPiPlay README: https://github.com/FD-/RPiPlay/blob/master/README.md
36. Apple — AirPlay: https://www.apple.com/airplay/
37. Apple — MFi programme: https://developer.apple.com/mfi/

**DLNA / UPnP**
32. gmrender-resurrect README: https://github.com/hzeller/gmrender-resurrect
33. gmrender-resurrect source: https://github.com/hzeller/gmrender-resurrect/tree/master/src
34. UPnP Device Architecture 2.0 (PDF): https://upnp.org/specs/arch/UPnP-arch-DeviceArchitecture-v2.0.pdf
35. MiniDLNA: https://github.com/azatoth/minidlna and https://minidlna.com/

**Other**
40. Via Licensing — AV patent pools: https://www.via-la.com/
39. GitHub REST API and crates.io API (`https://crates.io/api/v1/crates/{name}`), queried 2026-10-20 for every project in §5.

**Not retrievable on 2026-10-20 (stated rather than guessed)**
- DIAL protocol specification — `http://www.dial-multicast.org/specification.html` (connection failure).
- `thecubic/cast` (node-chromecast), `miniupnp/minidlna`, `occupiedpenguin/upmpdcli`, `google/go-chromecast`, `seppevs/gmrender-resurrect` — HTTP 404 from both the GitHub API and web UI on this network.
- Apple's AirPlay developer documentation and FairPlay Streaming documentation — JavaScript-only pages with no server-rendered text.
