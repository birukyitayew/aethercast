//! The AetherCast API.
//!
//! This crate is the contract: the daemon serves it on the session bus and
//! the client reads it, and both sides use these definitions. That is the
//! only way to be sure the two cannot drift apart.
//!
//! The published contract is the XML file next to the repository root,
//! [`dbus/io.github.birukyitayew.aethercast.Manager.xml`](../../dbus/io.github.birukyitayew.aethercast.Manager.xml).
//! The daemon checks the contract against what it serves, and refuses to
//! start if they disagree.

use serde::{Deserialize, Serialize};

/// The bus name the daemon owns.
pub const BUS_NAME: &str = "io.github.birukyitayew.aethercast";
/// The object path the manager is served at.
pub const OBJECT_PATH: &str = "/io/github/birukyitayew/aethercast";
/// The interface name.
pub const INTERFACE: &str = "io.github.birukyitayew.aethercast.Manager";
/// The published contract.
pub const CONTRACT: &str =
    include_str!("../../../dbus/io.github.birukyitayew.aethercast.Manager.xml");

/// The H.264 encoder to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Encoder {
    /// Pick the best available encoder.
    Auto,
    /// VA-API, through `vah264enc` or the legacy `vaapih264enc`.
    Vaapi,
    /// x264 — the only software encoder measured fast enough for 720p30.
    X264,
    /// OpenH264 — Cisco's encoder, present in every distribution.
    OpenH264,
}

/// A capture and encode profile.
///
/// This is the profile as it travels over the bus, so that the client and
/// the daemon agree on what a cast looks like.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub width: i32,
    pub height: i32,
    pub framerate: i32,
    /// Bitrate in bits per second.
    pub bitrate_bps: u32,
    pub encoder: Encoder,
    /// Capture the system audio mix as well.
    pub audio: bool,
    /// Use a generated test pattern instead of capturing the screen.
    ///
    /// This is what makes the pipeline testable on a machine with no
    /// display, such as a build server.
    pub test_pattern: bool,
    /// The PulseAudio source to capture. The system mix is the *monitor* of
    /// the default sink, for example
    /// `alsa_output.pci-0000_00_1f.3.analog-stereo.monitor`.
    pub audio_device: String,
}

impl Default for Profile {
    fn default() -> Self {
        // 640x480p30 is the profile the reference machine can actually
        // sustain: OpenH264 was measured at 0.32-0.81x realtime for 720p30
        // on this CPU, which pushed the PCR gap to 133 ms, past the 100 ms
        // the specification allows. 640x480 is also the format WFD
        // mandates as a minimum, so nothing is given up.
        Self {
            width: 640,
            height: 480,
            // Zero means "keep the capture rate".
            framerate: 0,
            // 2 Mbit/s is comfortable for 720p30 on a 2.4 GHz link.
            bitrate_bps: 2_000_000,
            encoder: Encoder::Auto,
            audio: false,
            audio_device: "default".to_string(),
            test_pattern: false,
        }
    }
}

/// A sink or source we know about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// A stable identifier: the P2P device address.
    pub id: String,
    /// The name the peer advertised, if any.
    pub name: Option<String>,
    /// What the peer is: a Miracast sink, a Chromecast device, and so on.
    pub kind: String,
    /// The peer's infrastructure address.
    pub address: Option<String>,
    /// The WFD information the peer advertised.
    pub wfd: Option<serde_json::Value>,
}

impl Device {
    /// The name to show, falling back to something the user can recognise.
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| self.id.clone())
    }

    /// Whether the peer said it can take a session right now.
    pub fn is_available(&self) -> bool {
        self.wfd
            .as_ref()
            .and_then(|wfd| wfd.get("session_available"))
            .and_then(|available| available.as_bool())
            .unwrap_or(false)
    }
}

/// The state of a casting session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// The session identifier.
    pub id: String,
    /// The device the session is for.
    pub device: String,
    /// One of: connecting, playing, stopped, failed.
    pub state: String,
    /// Why, when there is something to say.
    pub detail: Option<String>,
}

/// Station association state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StationStatus {
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub freq: Option<u32>,
    pub wpa_state: Option<String>,
    pub connected: bool,
}

/// A snapshot of everything the daemon knows about its environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostics {
    /// The wireless interface the daemon drives.
    pub interface: String,
    /// The station the interface is associated with.
    pub station: StationStatus,
    /// How many work items are in the radio queue.
    pub radio_work_items: usize,
    /// Whether the radio looks healthy.
    pub radio_healthy: bool,
    /// Set when the radio-work queue is wedged, with the reason.
    pub wedge: Option<String>,
    /// What the adapter can and cannot do.
    pub adapter: serde_json::Value,
    /// The global WFD information element, in hex.
    pub wfd_element: Option<String>,
}

impl Diagnostics {
    /// The adapter's limitation, if it has one.
    ///
    /// This is the sentence that explains why a sink was not found, and
    /// it is the reason the diagnostics are worth showing at all.
    pub fn limitation(&self) -> Option<String> {
        self.adapter
            .get("limitation")
            .and_then(|limitation| limitation.as_str())
            .map(str::to_string)
            .filter(|limitation| !limitation.trim().is_empty())
    }

    /// The bands the adapter can hear.
    pub fn bands(&self) -> Option<String> {
        self.adapter
            .get("bands")
            .and_then(|bands| bands.as_str())
            .map(str::to_string)
    }
}

/// The client for the API.
///
/// The daemon serves this interface on the session bus. The methods that
/// return structured data return JSON, so that the interface can grow
/// without breaking the clients that came before it.
#[zbus::proxy(
    interface = "io.github.birukyitayew.aethercast.Manager",
    default_service = "io.github.birukyitayew.aethercast",
    default_path = "/io/github/birukyitayew/aethercast"
)]
pub trait ManagerApi {
    /// The interface version.
    fn version(&self) -> zbus::Result<String>;

    /// The devices we know about, as JSON.
    fn list_devices(&self) -> zbus::Result<String>;

    /// The casting sessions we know about, as JSON.
    fn list_sessions(&self) -> zbus::Result<String>;

    /// A snapshot of the environment, as JSON.
    fn get_diagnostics(&self) -> zbus::Result<String>;

    /// Advertise a WFD information element, given as hex.
    fn arm_wfd_element(&self, element: String) -> zbus::Result<String>;

    /// Start casting to a device, and return the session identifier.
    fn start_cast(&self, device: String, profile: String) -> zbus::Result<String>;

    /// Stop a casting session.
    fn stop_cast(&self, session: String) -> zbus::Result<bool>;

    /// A sink was discovered. The argument is the device, as JSON.
    #[zbus(signal)]
    fn device_found(&self, device: String) -> zbus::Result<()>;

    /// A session changed state.
    #[zbus(signal)]
    fn session_state_changed(&self, session: String, state: String) -> zbus::Result<()>;
}

/// The methods the contract publishes.
///
/// The daemon compares this with what it serves, so that a contract that
/// has drifted is a failure rather than a surprise.
pub const CONTRACT_METHODS: &[&str] = &[
    "Version",
    "ListDevices",
    "ListSessions",
    "GetDiagnostics",
    "ArmWfdElement",
    "StartCast",
    "StopCast",
];

/// The signals the contract publishes.
pub const CONTRACT_SIGNALS: &[&str] = &["DeviceFound", "SessionStateChanged"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contract_publishes_what_the_api_offers() {
        for method in CONTRACT_METHODS {
            assert!(
                CONTRACT.contains(method),
                "{method} is part of the API but not of the contract"
            );
        }
        for signal in CONTRACT_SIGNALS {
            assert!(
                CONTRACT.contains(signal),
                "{signal} is part of the API but not of the contract"
            );
        }
        assert!(
            CONTRACT.contains(INTERFACE),
            "the contract does not name the interface"
        );
    }

    #[test]
    fn a_device_falls_back_to_its_address_for_a_name() {
        let device = Device {
            id: "aa:bb:cc:dd:ee:ff".into(),
            name: None,
            kind: "miracast-sink".into(),
            address: None,
            wfd: None,
        };
        assert_eq!(device.display_name(), "aa:bb:cc:dd:ee:ff");
        assert!(!device.is_available());
    }
}
