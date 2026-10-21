//! The session manager: the state and the operations that the command line
//! and the D-Bus API both sit on.
//!
//! Keeping this separate from the two front ends is the point: the CLI and
//! the bus must never disagree about what the daemon is doing, and neither
//! of them should own state that the other one also owns.

use std::sync::{Arc, Mutex};

use aethercast_api::{Diagnostics, Session, StationStatus, INTERFACE, OBJECT_PATH};

// The types the API publishes are the daemon's types too.
pub use aethercast_api::Device;

use crate::discovery::Peer;
use crate::radio::Radio;
use crate::wfd::{self, WfdIe};

/// Build a device from a discovered peer.
pub fn device_from_peer(peer: &Peer) -> Device {
    let wfd = peer.wfd_dev_info.as_ref().map(|info| {
        serde_json::json!({
            "device_type": format!("{}", info.device.device_type),
            "session_available": info.device.session_available,
            "control_port": info.control_port,
            "max_throughput_mbps": info.max_throughput_mbps,
        })
    });
    Device {
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

/// The shared state and operations.
pub struct Manager {
    iface: String,
    radio: Mutex<Option<Radio>>,
    devices: Mutex<Vec<Device>>,
    sessions: Mutex<Vec<Session>>,
    next_session: Mutex<u64>,
    /// The bus connection, so that the daemon can emit signals.
    bus: Mutex<Option<zbus::Connection>>,
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
            bus: Mutex::new(None),
        }
    }

    /// Remember the bus connection, so that signals can be emitted.
    pub fn set_bus(&self, connection: zbus::Connection) {
        *self.bus.lock().unwrap() = Some(connection);
    }

    /// Emit a signal on the bus.
    ///
    /// The body is a single string, which is how the rest of the API
    /// carries structured data.
    pub fn emit_signal(&self, member: &str, body: &str) -> bool {
        let guard = self.bus.lock().unwrap();
        let Some(connection) = guard.as_ref() else {
            return false;
        };
        let message = zbus::Message::signal(OBJECT_PATH, INTERFACE, member)
            .and_then(|builder| builder.build(&body.to_string()));
        match message {
            Ok(message) => {
                let connection = connection.clone();
                // The bus is asynchronous, and a failure to notify is not
                // worth failing the daemon over.
                tokio::spawn(async move {
                    let _ = connection.send(&message).await;
                });
                true
            }
            Err(_) => false,
        }
    }

    /// Search for peers, and announce the ones we have not seen before.
    ///
    /// This is what the daemon runs in the background, so that a client is
    /// told when a sink appears instead of polling for it.
    pub fn search(&self, duration: u32) -> Vec<Device> {
        let peers = self
            .with_radio(|radio| {
                // A sink that filters on the WFD element will not answer
                // without one.
                let ie = wfd::source_ie(crate::rtsp::DEFAULT_CONTROL_PORT, 200);
                let _ = radio.set_wfd_ie(&ie);

                let _ = radio.p2p_find(duration);
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(duration as u64 + 5);
                let mut peers: std::collections::BTreeMap<String, Peer> =
                    std::collections::BTreeMap::new();
                while std::time::Instant::now() < deadline {
                    if let Ok(Some(line)) = radio
                        .supplicant()
                        .poll_event(std::time::Duration::from_millis(500))
                    {
                        if let Some(peer) = Peer::parse_event(&line) {
                            peers.entry(peer.p2p_device_address.clone()).or_insert(peer);
                        }
                    }
                }
                let _ = radio.p2p_stop_find();

                // Fill in the details of the peers we only saw as events.
                if let Ok(addresses) = radio.p2p_peers() {
                    for address in addresses {
                        if let Ok(details) = radio.p2p_peer(&address) {
                            if let Some(peer) = peers.get_mut(&address) {
                                peer.merge_fields(&details);
                            }
                        }
                    }
                }
                peers.into_values().collect::<Vec<Peer>>()
            })
            .unwrap_or_default();

        let devices: Vec<Device> = peers.iter().map(device_from_peer).collect();
        for device in &devices {
            self.announce_device(device);
        }
        devices
    }

    /// Announce a device, if it is new.
    pub fn announce_device(&self, device: &Device) {
        let mut devices = self.devices.lock().unwrap();
        if devices.iter().any(|d| d.id == device.id) {
            return;
        }
        devices.push(device.clone());
        drop(devices);
        if let Ok(json) = serde_json::to_string(device) {
            self.emit_signal("DeviceFound", &json);
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

    /// Replace the device list, without announcing anything.
    pub fn replace_devices(&self, devices: Vec<Device>) {
        *self.devices.lock().unwrap() = devices;
    }

    /// The devices we know about.
    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
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
