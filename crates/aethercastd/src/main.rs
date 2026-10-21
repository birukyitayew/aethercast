//! `aethercastd` — the AetherCast session daemon.
//!
//! Milestone 1: radio health, the global WFD information element, and
//! Wi-Fi Direct discovery. The media pipeline and the WFD session
//! negotiation come next.

mod cast;
mod core;
mod dbus;
mod discovery;
mod hw;
mod media;
mod p2p;
mod radio;
mod rtsp;
mod supplicant;
mod wfd;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use gstreamer::prelude::*;

use crate::discovery::Peer;
use crate::media::Encoder;
use crate::radio::Radio;
use crate::wfd::{WfdIe, DEFAULT_CONTROL_PORT};

#[derive(Parser, Debug)]
#[command(
    name = "aethercastd",
    version,
    about = "AetherCast: Miracast (Wi-Fi Display) source for Linux",
    long_about = "AetherCast session daemon.\n\n\
                  Milestone 1 covers radio health, the global WFD information \
                  element, and Wi-Fi Direct peer discovery."
)]
struct Cli {
    /// Wireless interface to control.
    #[arg(long, global = true, default_value = "wlan0")]
    iface: String,

    /// Emit machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Report radio health: station state, radio-work queue, WFD element.
    Doctor,
    /// Set the global WFD information element used in discovery probes.
    Arm(ArmArgs),
    /// Flush a wedged radio-work queue and wait for the station to return.
    Recover,
    /// Search for Wi-Fi Direct peers and report Miracast sinks.
    Discover(DiscoverArgs),
    /// Inspect and exercise the media pipeline.
    #[command(subcommand)]
    Media(MediaCommand),
    /// Run the WFD RTSP handshake against a sink.
    #[command(subcommand)]
    Rtsp(RtspCommand),
    /// Wi-Fi Direct group formation.
    #[command(subcommand)]
    P2p(P2pCommand),
    /// Work with WFD information elements.
    #[command(subcommand)]
    Wfd(WfdCommand),
    /// Cast to a sink: run the WFD handshake and stream to it.
    Cast(CastArgs),
    /// Serve the D-Bus API on the session bus.
    Daemon,
}

#[derive(clap::Args, Debug)]
struct CastArgs {
    /// The RTSP control port to listen on.
    #[arg(long, default_value_t = rtsp::DEFAULT_CONTROL_PORT)]
    port: u16,
    /// How long to wait for a sink to connect, in seconds.
    #[arg(long, default_value_t = 60)]
    wait: u32,
    /// How long to stream for, in seconds.
    #[arg(long, default_value_t = 30)]
    seconds: u32,
    /// Width in pixels.
    #[arg(long, default_value_t = 640)]
    width: i32,
    /// Height in pixels.
    #[arg(long, default_value_t = 480)]
    height: i32,
    /// Frames per second. Zero keeps the capture rate.
    #[arg(long, default_value_t = 30)]
    framerate: i32,
    /// Bitrate in kbit/s.
    #[arg(long, default_value_t = 2000)]
    bitrate: u32,
    /// Also capture the system audio mix.
    #[arg(long)]
    audio: bool,
    /// The PulseAudio source to capture. Use the monitor of the default
    /// sink for the system mix.
    #[arg(long, default_value = "default")]
    audio_device: String,
    /// Form a Wi-Fi Direct group and serve the session on it.
    #[arg(long)]
    group: bool,
    /// Use a generated test pattern instead of capturing the screen. This
    /// makes the cast testable without a display.
    #[arg(long)]
    test_pattern: bool,
    /// The frequency for the group, in MHz. On a single-radio adapter the
    /// group has to share the station's channel.
    #[arg(long)]
    freq: Option<u32>,
}

#[derive(Subcommand, Debug)]
enum WfdCommand {
    /// Decode a WFD information element body, under every candidate layout.
    Decode(WfdDecodeArgs),
}

#[derive(clap::Args, Debug)]
struct WfdDecodeArgs {
    /// The element body, as hex. This is what a sink reports in its
    /// `wfd_dev_info` field.
    hex: String,
}

#[derive(Subcommand, Debug)]
enum P2pCommand {
    /// Become the Group Owner and wait for a sink to join.
    Group(P2pGroupArgs),
    /// Invite a peer into a group we own.
    Connect(P2pConnectArgs),
    /// Stay discoverable and wait for a sink to connect to us.
    Listen(P2pListenArgs),
}

#[derive(clap::Args, Debug)]
struct P2pListenArgs {
    /// How long to stay discoverable, in seconds.
    #[arg(long, default_value_t = 120)]
    seconds: u32,
}

#[derive(clap::Args, Debug)]
struct P2pGroupArgs {
    /// The frequency for the group, in MHz. On a single-radio adapter the
    /// group has to share the station's channel.
    #[arg(long)]
    freq: Option<u32>,
    /// How long to wait for a member, in seconds.
    #[arg(long, default_value_t = 60)]
    wait: u32,
}

#[derive(clap::Args, Debug)]
struct P2pConnectArgs {
    /// The peer's P2P device address.
    #[arg(long)]
    peer: String,
    /// How long to wait for the group, in seconds.
    #[arg(long, default_value_t = 30)]
    wait: u32,
}

#[derive(Subcommand, Debug)]
enum RtspCommand {
    /// Listen for a sink and run the WFD handshake with it.
    Serve(RtspServeArgs),
}

#[derive(clap::Args, Debug)]
struct RtspServeArgs {
    /// The control port to listen on.
    #[arg(long, default_value_t = rtsp::DEFAULT_CONTROL_PORT)]
    port: u16,
    /// Stop after this many seconds.
    #[arg(long, default_value_t = 60)]
    seconds: u32,
}

#[derive(Subcommand, Debug)]
enum MediaCommand {
    /// Report the available encoders and the elements the pipeline needs.
    Probe,
    /// Run the pipeline into an MPEG-TS file and validate it.
    Test(MediaTestArgs),
}

#[derive(clap::Args, Debug)]
struct MediaTestArgs {
    /// Where to write the MPEG-TS file.
    #[arg(long, default_value = "/tmp/aethercast-test.ts")]
    out: String,
    /// How long to run, in seconds.
    #[arg(long, default_value_t = 5)]
    seconds: u32,
    /// Width in pixels.
    #[arg(long, default_value_t = 640)]
    width: i32,
    /// Height in pixels.
    #[arg(long, default_value_t = 480)]
    height: i32,
    /// Frames per second. Zero keeps the capture rate.
    #[arg(long, default_value_t = 30)]
    framerate: i32,
    /// Bitrate in kbit/s.
    #[arg(long, default_value_t = 2000)]
    bitrate: u32,
    /// Encoder to use.
    #[arg(long, value_enum, default_value_t = Encoder::Auto)]
    encoder: Encoder,
    /// Also capture the system audio mix.
    #[arg(long)]
    audio: bool,
    /// The PulseAudio source to capture. Use the monitor of the default
    /// sink for the system mix.
    #[arg(long, default_value = "default")]
    audio_device: String,
    /// Send RTP over UDP to this host:port instead of writing a file.
    #[arg(long)]
    udp: Option<String>,
    /// Use a generated test pattern instead of capturing the screen.
    #[arg(long)]
    test_pattern: bool,
}

#[derive(clap::Args, Debug)]
struct ArmArgs {
    /// Which WFD element to advertise.
    #[arg(long, value_enum, default_value_t = Profile::Aethercast)]
    profile: Profile,
    /// Session management control port.
    #[arg(long, default_value_t = DEFAULT_CONTROL_PORT)]
    port: u16,
    /// Maximum throughput to advertise, in Mbit/s.
    #[arg(long, default_value_t = 200)]
    throughput: u16,
    /// Advertise this exact device information field, as two hex bytes.
    /// Use it to copy what a sink itself advertises.
    #[arg(long)]
    device_info: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Profile {
    /// AetherCast's own element: source, session available, P2P, no TDLS.
    Aethercast,
    /// The element GNOME Network Displays 0.99.0 ships, for comparison.
    Gnd,
}

#[derive(clap::Args, Debug)]
struct DiscoverArgs {
    /// How long to search, in seconds.
    #[arg(long, default_value_t = 30)]
    duration: u32,
    /// Flush a wedged radio-work queue before searching.
    #[arg(long)]
    recover: bool,
    /// Also print the raw field dump of every peer.
    #[arg(long)]
    verbose: bool,
}

fn main() -> Result<()> {
    // GStreamer has to be initialised before any element is looked up.
    gstreamer::init().ok();
    let cli = Cli::parse();

    // The radio is opened only by the commands that need it. Serving a
    // session and running the media pipeline need no wireless adapter at
    // all, which is what makes them testable on a build server.
    match &cli.command {
        Command::Doctor => doctor(&cli, open_radio(&cli)?),
        Command::Arm(args) => arm(&cli, open_radio(&cli)?, args),
        Command::Recover => recover(&cli, open_radio(&cli)?),
        Command::Discover(args) => {
            let manager = core::Manager::new(&cli.iface);
            manager.open_radio().map_err(|e| anyhow::anyhow!("{e}"))?;
            discover(&cli, &manager, args)
        }
        Command::Media(MediaCommand::Probe) => media_probe(&cli),
        Command::Media(MediaCommand::Test(args)) => media_test(&cli, args),
        Command::Rtsp(RtspCommand::Serve(args)) => rtsp_serve(&cli, args),
        Command::P2p(P2pCommand::Group(args)) => p2p_group(&cli, open_radio(&cli)?, args),
        Command::P2p(P2pCommand::Connect(args)) => p2p_connect(&cli, open_radio(&cli)?, args),
        Command::P2p(P2pCommand::Listen(args)) => p2p_listen(&cli, open_radio(&cli)?, args),
        Command::Wfd(WfdCommand::Decode(args)) => wfd_decode(&cli, args),
        Command::Cast(args) => cast(&cli, args),
        Command::Daemon => {
            let manager = std::sync::Arc::new(core::Manager::new(&cli.iface));
            // The runtime is only needed for the bus, and the bus is only
            // needed by the daemon.
            tokio::runtime::Runtime::new()?.block_on(dbus::serve(manager))
        }
    }
}

/// Open the wireless radio.
fn open_radio(cli: &Cli) -> Result<Radio> {
    Radio::open(&cli.iface).with_context(|| {
        format!(
            "opening the wpa_supplicant control interface for {}",
            cli.iface
        )
    })
}

/// Cast to a sink.
///
/// The sink connects to our RTSP control port, we negotiate with it, and
/// then the media pipeline starts streaming to the RTP port it asked for.
fn cast(cli: &Cli, args: &CastArgs) -> Result<()> {
    let manager = core::Manager::new(&cli.iface);
    let profile = media::Profile {
        width: args.width,
        height: args.height,
        framerate: args.framerate,
        bitrate_bps: args.bitrate * 1000,
        encoder: media::Encoder::Auto,
        audio: args.audio,
        audio_device: args.audio_device.clone(),
        test_pattern: args.test_pattern,
    };
    let options = cast::Options {
        port: args.port,
        wait: args.wait,
        seconds: args.seconds,
        group: args.group,
        freq: args.freq,
    };
    let mut log = |line: &str| {
        if cli.json {
            println!("{}", serde_json::json!({ "event": line }));
        } else {
            println!("{line}");
        }
    };
    cast::run(&manager, &profile, &options, &mut log)
}

/// Decode a WFD information element, under both candidate bit layouts.
fn wfd_decode(cli: &Cli, args: &WfdDecodeArgs) -> Result<()> {
    let hex = args.hex.trim().trim_start_matches("0x");
    let mut bytes = Vec::new();
    for pair in hex.as_bytes().chunks(2) {
        if pair.len() < 2 {
            anyhow::bail!("the hex string has an odd length");
        }
        bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    if bytes.is_empty() {
        anyhow::bail!("no bytes to decode");
    }
    // Accept either a whole element body (id, length, body) or the raw
    // device information field, and pick out the field to decode.
    let field: Vec<u8> = match wfd::WfdIe::parse(&bytes) {
        Ok(element) => match element.get(wfd::id::DEVICE_INFO) {
            Some(body) => body.to_vec(),
            None => bytes.clone(),
        },
        Err(_) => bytes.clone(),
    };
    let layouts = wfd::decode_layouts(&field);
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "hex": hex,
                "layouts": layouts.iter().map(|(layout, info)| serde_json::json!({
                    "layout": format!("{layout}"),
                    "device_type": format!("{}", info.device_type),
                    "session_available": info.session_available,
                    "prefer_tdls": info.prefer_tdls,
                    "wsd_available": info.wsd_available,
                    "hdcp2_supported": info.hdcp2_supported,
                    "raw": format!("0x{:04x}", info.to_raw()),
                })).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }
    println!("element: {} ({} bytes)", hex, bytes.len());
    println!(
        "device information field: {}",
        field.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    for (layout, info) in layouts {
        println!("  {layout}");
        println!("    device type      : {}", info.device_type);
        println!("    session available: {}", info.session_available);
        println!("    TDLS preferred   : {}", info.prefer_tdls);
        println!("    WSD available    : {}", info.wsd_available);
        println!("    HDCP 2.x         : {}", info.hdcp2_supported);
        println!("    raw              : 0x{:04x}", info.to_raw());
    }
    Ok(())
}

/// Stay discoverable, the way a Miracast source has to, and wait for a sink
/// to connect.
fn p2p_listen(cli: &Cli, mut radio: Radio, args: &P2pListenArgs) -> Result<()> {
    let say = |line: &str| {
        if cli.json {
            println!("{}", serde_json::json!({ "event": line }));
        } else {
            println!("{line}");
        }
    };
    // A sink that filters on the WFD information element will not answer
    // us without one.
    let ie = wfd::source_ie(rtsp::DEFAULT_CONTROL_PORT, 200);
    radio.set_wfd_ie(&ie)?;
    say(&format!(
        "WFD element armed, staying discoverable for {}s",
        args.seconds
    ));

    radio.p2p_listen()?;
    say("listening: a sink can now find us");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.seconds as u64);
    let mut announced = Vec::new();
    while std::time::Instant::now() < deadline {
        // A group interface with an address means a sink has joined.
        for interface in radio.p2p_group_interfaces() {
            if announced.contains(&interface) {
                continue;
            }
            if let Some(address) = Radio::interface_address(&interface) {
                announced.push(interface.clone());
                println!("a sink joined on {interface} ({address})");
                println!("the RTSP server must listen on {address}:7236");
            }
        }
        // Report whatever the supplicant tells us.
        if let Ok(Some(line)) = radio
            .supplicant()
            .poll_event(std::time::Duration::from_millis(200))
        {
            if let Some(event) = p2p::Event::parse(&line) {
                match event {
                    p2p::Event::DeviceFound(text) => {
                        println!("device found: {text}");
                    }
                    p2p::Event::Other(text) if text.contains("P2P-") => {
                        println!("event: {text}");
                    }
                    _ => {}
                }
            }
        }
    }
    say("the listen window closed");
    Ok(())
}

/// Become the Group Owner and wait for a sink to join.
fn p2p_group(cli: &Cli, mut radio: Radio, args: &P2pGroupArgs) -> Result<()> {
    let say = |line: &str| {
        if cli.json {
            println!("{}", serde_json::json!({ "event": line }));
        } else {
            println!("{line}");
        }
    };
    // The list has to be taken *before* the group is created, otherwise
    // the new interface is already in it.
    let before = radio.p2p_group_interfaces();
    radio.p2p_group_add(args.freq)?;
    say(&format!("group requested (freq: {:?})", args.freq));
    // The group interface is the reliable signal: the group-start event
    // goes to the supplicant's log, not to the control socket.
    let interface = radio
        .wait_for_group_interface(&before, std::time::Duration::from_secs(args.wait as u64))?;
    let group = radio.wait_for_group(std::time::Duration::from_secs(3))?;
    match (interface, group) {
        (Some((interface, address)), _) => {
            println!("group started");
            println!("  interface   : {interface}");
            println!("  address     : {address}");
            println!("  the RTSP server must listen on {address}:7236");
        }
        (None, Some(group)) => {
            println!("group started on {}", group.interface);
        }
        (None, None) => {
            println!("no group formed within {}s", args.wait);
            let work = radio.radio_work()?;
            println!("  radio-work queue: {} item(s)", work.len());
            for item in work.iter().take(4) {
                println!(
                    "    - {} started={} age={:.0}s",
                    item.kind,
                    if item.started { "yes" } else { "no" },
                    item.age_secs
                );
            }
        }
    }
    Ok(())
}

/// Invite a peer into a group we own.
fn p2p_connect(cli: &Cli, mut radio: Radio, args: &P2pConnectArgs) -> Result<()> {
    let say = |line: &str| {
        if cli.json {
            println!("{}", serde_json::json!({ "event": line }));
        } else {
            println!("{line}");
        }
    };
    say(&format!("provisioning {}", args.peer));
    match radio.p2p_prov_disc(&args.peer) {
        Ok(text) => say(&format!("  {text}")),
        Err(e) => say(&format!("  provisioning discovery failed: {e}")),
    }
    radio.p2p_connect(&args.peer)?;
    say(&format!("group requested for {}", args.peer));
    let group = radio.wait_for_group(std::time::Duration::from_secs(args.wait as u64))?;
    match group {
        Some(group) => {
            println!("group started on {}", group.interface);
            if let Ok(members) = radio.p2p_peers() {
                for member in members {
                    say(&format!("  member: {member}"));
                    if let Ok(details) = radio.p2p_group_member(&member) {
                        say(&format!("    {details}"));
                    }
                }
            }
        }
        None => say(&format!("no group formed within {}s", args.wait)),
    }
    Ok(())
}

/// Run the WFD RTSP handshake against the first sink that connects.
fn rtsp_serve(cli: &Cli, args: &RtspServeArgs) -> Result<()> {
    let server = rtsp::Server::bind(args.port)
        .map_err(|e| anyhow::anyhow!("binding port {} failed: {e}", args.port))?;
    println!("listening for a sink on port {}", server.port());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.seconds as u64);
    let mut log = |line: &str| {
        if cli.json {
            println!("{}", serde_json::json!({ "event": line }));
        } else {
            println!("{line}");
        }
    };

    // The accept has no timeout of its own, so the deadline is checked
    // between connections.
    server.set_nonblocking(true).ok();
    loop {
        if std::time::Instant::now() >= deadline {
            log("no sink connected before the deadline");
            return Ok(());
        }
        match server.handle_once(&mut log) {
            Ok(session) => {
                log(&format!("handshake reached {:?}", session.stage));
                if let Some(caps) = &session.capabilities {
                    log(&format!("sink capabilities: {caps:?}"));
                }
                return Ok(());
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
                return Err(e.into());
            }
        }
    }
}

/// Report what the media pipeline can use on this machine.
fn media_probe(cli: &Cli) -> Result<()> {
    let encoders = media::available_encoders();
    let elements = [
        "ximagesrc",
        "videoconvert",
        "queue",
        "h264parse",
        "mpegtsmux",
        "rtpmp2tpay",
        "udpsink",
        "filesink",
        "pulsesrc",
        "avenc_aac",
    ];
    let mut missing = Vec::new();
    for element in elements {
        if gstreamer::ElementFactory::find(element).is_none() {
            missing.push(element);
        }
    }
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "encoders": encoders.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>(),
                "missing_elements": missing,
                "pids": {
                    "pmt": media::PID_PMT,
                    "pcr": media::PID_PCR,
                    "video": media::PID_VIDEO,
                    "audio": media::PID_AUDIO,
                },
                "rtp": { "payload_type": media::RTP_PAYLOAD_TYPE, "mtu": media::RTP_MTU },
            }))?
        );
        return Ok(());
    }
    println!("encoders installed  :");
    for encoder in &encoders {
        println!("  - {encoder:?}");
    }
    if encoders.is_empty() {
        println!("  (none — install gstreamer1.0-plugins-ugly for x264enc)");
    }
    println!(
        "missing elements    : {}",
        if missing.is_empty() {
            "none".to_string()
        } else {
            missing.join(", ")
        }
    );
    println!(
        "wfd pids            : pmt=0x{:04x} pcr=0x{:04x} video=0x{:04x} audio=0x{:04x}",
        media::PID_PMT,
        media::PID_PCR,
        media::PID_VIDEO,
        media::PID_AUDIO
    );
    println!(
        "rtp                 : payload type {}, mtu {}",
        media::RTP_PAYLOAD_TYPE,
        media::RTP_MTU
    );
    Ok(())
}

/// Run the pipeline into a file so the encode and the mux can be validated
/// without a sink.
fn media_test(cli: &Cli, args: &MediaTestArgs) -> Result<()> {
    let profile = media::Profile {
        width: args.width,
        height: args.height,
        framerate: args.framerate,
        bitrate_bps: args.bitrate * 1000,
        encoder: args.encoder,
        audio: args.audio,
        audio_device: args.audio_device.clone(),
        test_pattern: args.test_pattern,
    };
    // A UDP destination exercises the RTP transport; a file keeps bare
    // MPEG-TS so the stream can be validated with ffprobe.
    let sink = match &args.udp {
        Some(target) => {
            let (host, port) = target
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("--udp expects host:port"))?;
            media::Sink::Udp {
                host: host.to_string(),
                port: port.parse()?,
            }
        }
        None => media::Sink::File(args.out.clone()),
    };
    let built = media::build(&profile, &sink)?;

    let bus = built.pipeline.bus().expect("the pipeline has a bus");
    if let Err(e) = built.pipeline.set_state(gstreamer::State::Playing) {
        // The bus carries the reason, and a bare "failed to change its
        // state" tells nobody anything.
        let mut detail = String::new();
        while let Some(message) = bus.timed_pop(gstreamer::ClockTime::ZERO) {
            if let gstreamer::MessageView::Error(err) = message.view() {
                detail = format!("{err}\n{}", err.debug().unwrap_or_default());
                break;
            }
        }
        built.pipeline.set_state(gstreamer::State::Null).ok();
        anyhow::bail!("the pipeline would not start: {e}\n{detail}");
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.seconds as u64);
    while std::time::Instant::now() < deadline {
        if let Some(message) = bus.timed_pop(gstreamer::ClockTime::from_mseconds(200)) {
            match message.view() {
                gstreamer::MessageView::Error(err) => {
                    let debug = err.debug().unwrap_or_default();
                    built.pipeline.set_state(gstreamer::State::Null).ok();
                    anyhow::bail!("pipeline error: {}\n{}", err.error(), debug);
                }
                gstreamer::MessageView::Eos(_) => break,
                _ => {}
            }
        }
    }
    built.pipeline.set_state(gstreamer::State::Null).ok();

    // In UDP mode there is no file: the report must not claim otherwise.
    let size = std::fs::metadata(&args.out).map(|m| m.len()).unwrap_or(0);
    let streaming = args.udp.is_some();
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "file": args.out,
                "bytes": size,
                "encoder": format!("{:?}", built.encoder),
                "encoder_element": built.encoder_element,
                "profile": {
                    "width": profile.width,
                    "height": profile.height,
                    "framerate": profile.framerate,
                    "bitrate_bps": profile.bitrate_bps,
                },
            }))?
        );
    } else {
        if streaming {
            println!(
                "streamed to {} (encoder: {} / {:?})",
                args.udp.as_deref().unwrap_or("-"),
                built.encoder_element,
                built.encoder
            );
        } else {
            println!(
                "wrote {} bytes to {} (encoder: {} / {:?})",
                size, args.out, built.encoder_element, built.encoder
            );
        }
    }
    if !streaming && size == 0 {
        anyhow::bail!("the pipeline produced no data");
    }
    Ok(())
}

fn doctor(cli: &Cli, mut radio: Radio) -> Result<()> {
    let status = radio.status()?;
    let work = radio.radio_work()?;
    let wedge = radio.wedge(radio::WEDGE_THRESHOLD)?;
    let wfd = radio.wfd_ie()?;

    if cli.json {
        let report = serde_json::json!({
            "iface": cli.iface,
            "station": status,
            "radio_work_items": work.len(),
            "radio_work": work,
            "wedged": wedge.is_some(),
            "wedge": wedge,
            "wfd_ie": wfd.as_ref().map(|ie| ie.to_hex()),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("interface        : {}", radio.iface());
    println!(
        "station          : {}{}{}",
        status.ssid.as_deref().unwrap_or("-"),
        status
            .bssid
            .as_deref()
            .map(|b| format!(" ({b})"))
            .unwrap_or_default(),
        if status.connected {
            "  [connected]"
        } else {
            "  [not connected]"
        }
    );
    println!(
        "channel          : {}",
        status
            .freq
            .map(|f| format!("{f} MHz"))
            .unwrap_or_else(|| "-".into())
    );
    println!("radio-work queue : {} item(s)", work.len());
    for item in work.iter().take(5) {
        println!(
            "  - {:<12} started={} age={:.0}s",
            item.kind,
            if item.started { "yes" } else { "no " },
            item.age_secs
        );
    }
    if work.len() > 5 {
        println!("  … and {} more", work.len() - 5);
    }
    match &wedge {
        Some(w) => println!("radio health     : WEDGED — {}", w.explanation),
        None => println!("radio health     : ok"),
    }
    let adapter = hw::Adapter::detect();
    println!(
        "adapter          : {} ({})",
        if adapter.phy.is_empty() {
            "unknown"
        } else {
            &adapter.phy
        },
        adapter.summary()
    );
    if let Some(limitation) = adapter.limitation() {
        println!("  limitation     : {limitation}");
    }
    match &wfd {
        Some(ie) => {
            println!("wfd element      : armed");
            for sub in &ie.subelements {
                println!(
                    "  - id={} body={}",
                    sub.id,
                    sub.body
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                );
            }
            if let Some(info) = ie.device_info() {
                println!(
                    "  device type     : {}  session available: {}  TDLS preferred: {}",
                    info.device_type, info.session_available, info.prefer_tdls
                );
            }
        }
        None => println!("wfd element      : not armed (run `aethercastd arm`)"),
    }
    Ok(())
}

fn arm(cli: &Cli, mut radio: Radio, args: &ArmArgs) -> Result<()> {
    let ie = match &args.device_info {
        // The interop escape hatch: advertise exactly these two bytes.
        Some(hex) => {
            let hex = hex.trim().trim_start_matches("0x");
            if hex.len() != 4 {
                anyhow::bail!("--device-info needs exactly two hex bytes, for example 1190");
            }
            let mut bytes = [0u8; 2];
            for (i, pair) in hex.as_bytes().chunks(2).enumerate() {
                bytes[i] = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
            }
            wfd::source_ie_with_device_info(bytes, args.port, args.throughput)
        }
        None => match args.profile {
            Profile::Aethercast => wfd::source_ie(args.port, args.throughput),
            Profile::Gnd => wfd::gnd_ie(),
        },
    };
    radio.set_wfd_ie(&ie)?;
    let readback = radio.wfd_ie()?.unwrap_or_else(WfdIe::default);
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "armed": readback.to_hex(),
                "profile": format!("{:?}", args.profile).to_lowercase(),
            }))?
        );
    } else {
        println!("armed WFD element: {}", readback.to_hex());
        if let Some(info) = readback.device_info() {
            println!(
                "  device type: {}  session available: {}  TDLS preferred: {}",
                info.device_type, info.session_available, info.prefer_tdls
            );
        }
    }
    Ok(())
}

fn recover(cli: &Cli, mut radio: Radio) -> Result<()> {
    let recovery = radio.recover()?;
    let stale = radio.remove_stale_group_interfaces()?;
    if !stale.is_empty() {
        println!("removed stale group interface(s): {}", stale.join(", "));
    } else {
        let leftover = radio.p2p_group_interfaces();
        if !leftover.is_empty() {
            let names = leftover.join(", ");
            println!(
                "a stale group interface is left over and needs removing with root:\n  sudo iw dev {names} del"
            );
        }
    }
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&recovery)?);
    } else {
        println!(
            "radio-work items : {} -> {}",
            recovery.work_items_before, recovery.work_items_after
        );
        println!(
            "station          : {} (after {} ms)",
            if recovery.station_connected {
                "reconnected"
            } else {
                "NOT reconnected"
            },
            recovery.reconnected_after_ms
        );
    }
    Ok(())
}

fn discover(cli: &Cli, manager: &core::Manager, args: &DiscoverArgs) -> Result<()> {
    // The whole search runs with the radio, and the manager records what
    // it found so that the D-Bus API can report the same list.
    let peers = match manager.with_radio(|radio| -> Result<Vec<Peer>> {
        // 1. Advertise ourselves before scanning. Without a WFD element in
        //    the probe requests, a strict sink may never answer.
        let ie = wfd::source_ie(DEFAULT_CONTROL_PORT, 200);
        if let Err(e) = radio.set_wfd_ie(&ie) {
            return Err(e.into());
        }

        // 2. A wedged queue makes every search fail instantly.
        let wedge = radio.wedge(radio::WEDGE_THRESHOLD)?;
        if let Some(wedge) = wedge {
            eprintln!("warning: {}", wedge.explanation);
            if args.recover {
                eprintln!("recovering: flushing the radio-work queue");
                let recovery = radio.recover()?;
                eprintln!(
                    "recovered: {} -> {} work items, station {}",
                    recovery.work_items_before,
                    recovery.work_items_after,
                    if recovery.station_connected {
                        "reconnected"
                    } else {
                        "not reconnected"
                    }
                );
                if let Err(e) = radio.set_wfd_ie(&ie) {
                    return Err(e.into());
                }
            }
        }

        // 3. Search.
        if let Err(e) = radio.p2p_find(args.duration) {
            return Err(e.into());
        }
        let deadline = Instant::now() + Duration::from_secs(args.duration as u64 + 5);
        let mut peers: BTreeMap<String, Peer> = BTreeMap::new();

        while Instant::now() < deadline {
            match radio.supplicant().poll_event(Duration::from_millis(500)) {
                Ok(Some(line)) => {
                    if let Some(peer) = Peer::parse_event(&line) {
                        peers.entry(peer.p2p_device_address.clone()).or_insert(peer);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("warning: control connection: {e}");
                    break;
                }
            }
        }
        if let Err(e) = radio.p2p_stop_find() {
            return Err(e.into());
        }

        // 4. Fill in details for peers we only saw as events.
        let addresses = radio.p2p_peers()?;
        for address in addresses {
            if let Ok(details) = radio.p2p_peer(&address) {
                if let Some(peer) = peers.get_mut(&address) {
                    peer.merge_fields(&details);
                }
            }
        }
        Ok(peers.values().cloned().collect::<Vec<Peer>>())
    }) {
        Some(result) => result?,
        None => anyhow::bail!("the radio is not available"),
    };

    let devices: Vec<core::Device> = peers.iter().map(core::Device::from_peer).collect();
    manager.set_devices(devices);

    if cli.json {
        let list: Vec<&Peer> = peers.iter().collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }

    if peers.is_empty() {
        println!("no peers found in {}s", args.duration);
        let wedge = manager
            .with_radio(|radio| radio.wedge(radio::WEDGE_THRESHOLD))
            .and_then(|result| result.ok().flatten());
        if let Some(w) = wedge {
            println!("  radio: WEDGED — {}", w.explanation);
        } else {
            println!("  radio: healthy, the search ran to completion");
        }
        // The two most common reasons, named explicitly.
        let adapter = hw::Adapter::detect();
        if let Some(limitation) = adapter.limitation() {
            println!("  adapter: {limitation}");
        }
        println!("  sink: make sure it is in Miracast / Screen Share mode, and that it advertises on a band this adapter can hear");
        return Ok(());
    }

    println!("{} peer(s) found in {}s\n", peers.len(), args.duration);
    for peer in &peers {
        println!("  {}", peer.p2p_device_address);
        if let Some(name) = &peer.name {
            println!("    name        : {name}");
        }
        if let Some(freq) = peer.src_freq {
            println!("    channel     : {freq} MHz");
        }
        match &peer.wfd_dev_info {
            Some(info) => {
                println!("    wfd role    : {}", info.device.device_type);
                println!(
                    "    session     : {}",
                    if info.device.session_available {
                        "available"
                    } else {
                        "unavailable"
                    }
                );
                println!("    control port: {}", info.control_port);
                println!("    max rate    : {} Mbit/s", info.max_throughput_mbps);
                println!("    HDCP 2.x    : {}", info.device.hdcp2_supported);
            }
            None => println!("    wfd role    : (no WFD information element advertised)"),
        }
        if args.verbose {
            println!("    raw         : {}", peer.raw);
        }
    }
    Ok(())
}
