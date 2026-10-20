//! `aethercastd` — the AetherCast session daemon.
//!
//! Milestone 1: radio health, the global WFD information element, and
//! Wi-Fi Direct discovery. The media pipeline and the WFD session
//! negotiation come next.

mod discovery;
mod radio;
mod supplicant;
mod wfd;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::discovery::Peer;
use crate::radio::Radio;
use crate::wfd::{WfdIe, DEFAULT_CONTROL_PORT};

/// How long a scan work item may stay in flight before we call the queue
/// wedged. A normal scan finishes in well under a second.
const WEDGE_THRESHOLD: Duration = Duration::from_secs(20);

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
    let cli = Cli::parse();
    let radio = Radio::open(&cli.iface).context("opening the wpa_supplicant control interface")?;

    match &cli.command {
        Command::Doctor => doctor(&cli, radio),
        Command::Arm(args) => arm(&cli, radio, args),
        Command::Recover => recover(&cli, radio),
        Command::Discover(args) => discover(&cli, radio, args),
    }
}

fn doctor(cli: &Cli, mut radio: Radio) -> Result<()> {
    let status = radio.status()?;
    let work = radio.radio_work()?;
    let wedge = radio.wedge(WEDGE_THRESHOLD)?;
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
        if status.connected { "  [connected]" } else { "  [not connected]" }
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
    match &wfd {
        Some(ie) => {
            println!("wfd element      : armed");
            for sub in &ie.subelements {
                println!(
                    "  - id={} body={}",
                    sub.id,
                    sub.body.iter().map(|b| format!("{b:02x}")).collect::<String>()
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
    let ie = match args.profile {
        Profile::Aethercast => wfd::source_ie(args.port, args.throughput),
        Profile::Gnd => wfd::gnd_ie(),
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

fn discover(cli: &Cli, mut radio: Radio, args: &DiscoverArgs) -> Result<()> {
    // 1. Advertise ourselves before scanning. Without a WFD element in the
    //    probe requests, a strict sink may never answer.
    let ie = wfd::source_ie(DEFAULT_CONTROL_PORT, 200);
    radio.set_wfd_ie(&ie)?;

    // 2. A wedged queue makes every search fail instantly.
    if let Some(wedge) = radio.wedge(WEDGE_THRESHOLD)? {
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
            radio.set_wfd_ie(&ie)?;
        }
    }

    // 3. Search.
    radio.p2p_find(args.duration)?;
    let deadline = Instant::now() + Duration::from_secs(args.duration as u64 + 5);
    let mut peers: BTreeMap<String, Peer> = BTreeMap::new();

    while Instant::now() < deadline {
        match radio.supplicant().poll_event(Duration::from_millis(500)) {
            Ok(Some(line)) => {
                if let Some(peer) = Peer::parse_event(&line) {
                    peers
                        .entry(peer.p2p_device_address.clone())
                        .or_insert(peer);
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("warning: control connection: {e}");
                break;
            }
        }
    }
    radio.p2p_stop_find()?;

    // 4. Fill in details for peers we only saw as events.
    let addresses: Vec<String> = radio.p2p_peers()?;
    for address in addresses {
        if let Ok(details) = radio.p2p_peer(&address) {
            if let Some(peer) = peers.get_mut(&address) {
                peer.merge_fields(&details);
            }
        }
    }

    if cli.json {
        let list: Vec<&Peer> = peers.values().collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }

    if peers.is_empty() {
        println!("no peers found in {}s", args.duration);
        let wedge = radio.wedge(WEDGE_THRESHOLD)?;
        if let Some(w) = wedge {
            println!("  radio: WEDGED — {}", w.explanation);
        } else {
            println!("  radio: healthy, the search ran to completion");
        }
        println!("  hint: make sure the sink is in Miracast / Screen Share mode");
        return Ok(());
    }

    println!("{} peer(s) found in {}s\n", peers.len(), args.duration);
    for peer in peers.values() {
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
                println!("    session     : {}", if info.device.session_available { "available" } else { "unavailable" });
                println!("    control port: {}", info.control_port);
                println!("    max rate    : {} Mbit/s", info.max_throughput_mbps);
                println!("    HDCP 2.x    : {}", info.device.hdcp2_supported);
            }
            None => println!("    wfd role    : (no WFD information element advertised)"),
        }
        if !peer.is_wfd() {
            println!("    note        : this peer is not a Wi-Fi Display device");
        }
        {
        }
        if args.verbose {
            println!("    raw         : {}", peer.raw);
        }
    }
    Ok(())
}
