//! The session manager: the state and the operations that the command line
//! and the D-Bus API both sit on.
//!
//! Keeping this separate from the two front ends is the point: the CLI and
//! the bus must never disagree about what the daemon is doing, and neither
//! of them should own state that the other one also owns.

use std::sync::{Arc, Mutex};

use crate::discovery::Peer;
use crate::radio::{Radio, StationStatus};
use crate::wfd::{self, WfdIe};

/// A sink or source we know about.
#[derive(Debug, Clone, serde::Serialize)]
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
    /// Build a device from a discovered peer.
    pub fn from_peer(peer: &Peer) -> Self {
        let wfd = peer.wfd_dev_info.as_ref().map(|info| {
            serde_json::json!({
                "device_type": format!("{}", info.device.device_type),
                "session_available": info.device.session_available,
                "control_port": info.control_port,
                "max_throughput_mbps": info.max_throughput_mbps,
            })
        });
        Self {
            id: peer.p2p_device_address.clone(),
            name: peer.name.clone(),
            kind: if peer.is_wfd() {
                "miracast-sink".to_string()
            } else {
                "wifi-direct-peer".to_string()
            },
            address: peer.device_address.clone(),
            wfd,
        }
    }
}

/// The state of a casting session.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Session {
    pub id: String,
    pub device: String,
    /// One of: connecting, playing, stopped, failed.
    pub state: String,
    pub detail: Option<String>,
}

/// A snapshot of everything the daemon knows about its environment.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostics {
    pub interface: String,
    pub station: StationStatus,
    pub radio_work_items: usize,
    pub radio_healthy: bool,
    /// Set when the radio-work queue is wedged, with the reason.
    pub wedge: Option<String>,
    pub adapter: serde_json::Value,
    /// The global WFD information element, in hex.
    pub wfd_element: Option<String>,
}

/// The shared state and operations.
pub struct Manager {
    iface: String,
    radio: Mutex<Option<Radio>>,
    devices: Mutex<Vec<Device>>,
    sessions: Mutex<Vec<Session>>,
    next_session: Mutex<u64>,
}

impl Manager {
    /// Create a manager for an interface.
    pub fn new(iface: &str) -> Self {
        Self {
            iface: iface.to_string(),
            radio: Mutex::new(None),
            devices: Mutex::new(Vec::new()),
            sessions: Mutex::new(Vec::new()),
            next_session: Mutex::new(1),
        }
    }

    /// The interface this manager drives.
    pub fn interface(&self) -> &str {
        &self.iface
    }

    /// Open the radio, remembering it for later calls.
    ///
    /// The radio is optional: a machine with no adapter can still serve a
    /// session to a sink that already found it.
    pub fn open_radio(&self) -> Result<(), String> {
        let mut radio = self.radio.lock().unwrap();
        if radio.is_none() {
            *radio = Some(Radio::open(&self.iface).map_err(|e| e.to_string())?);
        }
        Ok(())
    }

    /// Run something with the radio, if there is one.
    pub fn with_radio<T>(&self, f: impl FnOnce(&mut Radio) -> T) -> Option<T> {
        let mut guard = self.radio.lock().unwrap();
        guard.as_mut().map(f)
    }

    /// Arm the global WFD information element.
    pub fn arm(&self, element: &WfdIe) -> Result<String, String> {
        self.open_radio()?;
        match self.with_radio(|radio| radio.set_wfd_ie(element).map_err(|e| e.to_string())) {
            Some(Ok(())) => Ok(element.to_hex()),
            Some(Err(e)) => Err(e),
            None => Err("the radio is not available".to_string()),
        }
    }

    /// The devices we know about.
    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
    }

    /// Remember a set of discovered devices, replacing the previous list.
    pub fn set_devices(&self, devices: Vec<Device>) {
        *self.devices.lock().unwrap() = devices;
    }

    /// The sessions we know about.
    pub fn sessions(&self) -> Vec<Session> {
        self.sessions.lock().unwrap().clone()
    }

    /// Record a new session and return its identifier.
    pub fn new_session(&self, device: &str) -> Session {
        let mut counter = self.next_session.lock().unwrap();
        let id = format!("session-{}", *counter);
        *counter += 1;
        let session = Session {
            id: id.clone(),
            device: device.to_string(),
            state: "connecting".to_string(),
            detail: None,
        };
        self.sessions.lock().unwrap().push(session.clone());
        session
    }

    /// Update a session's state.
    pub fn update_session(&self, id: &str, state: &str, detail: Option<String>) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.iter_mut().find(|s| s.id == id) {
            session.state = state.to_string();
            session.detail = detail;
        }
    }

    /// Remove a session.
    pub fn remove_session(&self, id: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|s| s.id != id);
        sessions.len() != before
    }

    /// A snapshot of the environment.
    pub fn diagnostics(&self) -> Diagnostics {
        let adapter = crate::hw::Adapter::detect();
        let mut station = StationStatus::default();
        let mut radio_work_items = 0;
        let mut wedge = None;
        let mut wfd_element = None;

        self.with_radio(|radio| {
            if let Ok(status) = radio.status() {
                station = status;
            }
            if let Ok(work) = radio.radio_work() {
                radio_work_items = work.len();
            }
            if let Ok(Some(found)) = radio.wedge(crate::radio::WEDGE_THRESHOLD) {
                wedge = Some(found.explanation);
            }
            if let Ok(Some(element)) = radio.wfd_ie() {
                wfd_element = Some(element.to_hex());
            }
        });

        Diagnostics {
            interface: self.iface.clone(),
            station,
            radio_work_items,
            radio_healthy: wedge.is_none(),
            wedge,
            adapter: serde_json::json!({
                "phy": adapter.phy,
                "bands": adapter.band_names(),
                "ranges": adapter.ranges(),
                "limitation": adapter.limitation(),
            }),
            wfd_element,
        }
    }

    /// The default element AetherCast advertises.
    pub fn default_element(&self) -> WfdIe {
        wfd::source_ie(crate::rtsp::DEFAULT_CONTROL_PORT, 200)
    }
}

/// A manager shared between the front ends.
pub type SharedManager = Arc<Manager>;
