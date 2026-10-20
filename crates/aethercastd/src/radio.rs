//! Radio health, Wi-Fi Direct control, and the WFD information element.
//!
//! This module owns the two things that decide whether a cast can even start
//! on a given machine:
//!
//! 1. **The global WFD information element.** NetworkManager has no API for
//!    it, so the daemon sets it directly on `wpa_supplicant` with
//!    `WFD_SUBELEM_SET`. Without it our P2P discovery probe requests carry
//!    no WFD IE, and a strict Miracast sink is entitled to ignore them.
//!
//! 2. **The radio-work queue.** `wpa_supplicant` serialises scans through
//!    it. If a scan never completes, the queue wedges: every later request
//!    is rejected instantly and the queue grows without bound as
//!    NetworkManager keeps queuing its periodic scan. The symptom users see
//!    is "no sinks found" forever.

use std::time::{Duration, Instant};

use crate::supplicant::Supplicant;
use crate::wfd::WfdIe;

/// Errors raised by the radio layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Supplicant(#[from] crate::supplicant::Error),
    #[error(transparent)]
    Wfd(#[from] crate::wfd::Error),
}

/// A radio-work item that is wedged.
/// One entry in the `wpa_supplicant` radio-work queue.
///
/// The control interface prints entries as
/// `type@iface:freq:started:age.seconds.microseconds`, where `started` is 1
/// while the work is in flight.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RadioWork {
    pub kind: String,
    pub iface: String,
    pub freq: u32,
    pub started: bool,
    pub age_secs: f64,
}

impl RadioWork {
    /// Parse one line of `RADIO_WORK show` output.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let mut parts = line.split(':');
        let head = parts.next()?;
        let (kind, iface) = head.split_once('@')?;
        let freq = parts.next()?.parse().ok()?;
        let started = parts.next()? == "1";
        let age = parts.next()?;
        let age_secs = age
            .split_once('.')
            .map(|(s, frac)| format!("{s}.{frac}"))
            .unwrap_or_else(|| age.to_string())
            .parse()
            .unwrap_or(0.0);
        Some(Self {
            kind: kind.to_string(),
            iface: iface.to_string(),
            freq,
            started,
            age_secs,
        })
    }
}

/// Station association state.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct StationStatus {
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub freq: Option<u32>,
    pub wpa_state: Option<String>,
    pub connected: bool,
}

/// What a recovery pass did.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Recovery {
    pub work_items_before: usize,
    pub work_items_after: usize,
    pub station_connected: bool,
    pub reconnected_after_ms: u64,
}

/// A radio-work item that is wedged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Wedge {
    pub work: RadioWork,
    pub explanation: String,
}

/// Result type for the radio layer.
pub type RadioResult<T> = std::result::Result<T, Error>;

/// Control over the Wi-Fi radio and its Wi-Fi Direct state.
pub struct Radio {
    sup: Supplicant,
    iface: String,
}

impl Radio {
    /// Open the control interface of `iface`.
    pub fn open(iface: &str) -> crate::supplicant::Result<Self> {
        Ok(Self {
            sup: Supplicant::connect(iface)?,
            iface: iface.to_string(),
        })
    }

    /// The interface this radio belongs to.
    pub fn iface(&self) -> &str {
        &self.iface
    }

    /// Access to the underlying control connection, for event polling.
    pub fn supplicant(&mut self) -> &mut Supplicant {
        &mut self.sup
    }

    /// Read the global WFD information element.
    pub fn wfd_ie(&mut self) -> RadioResult<Option<WfdIe>> {
        let hex = self.sup.request("WFD_SUBELEM_GET all")?;
        if hex.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(WfdIe::parse_hex(&hex)?))
    }

    /// Set the global WFD information element.
    pub fn set_wfd_ie(&mut self, ie: &WfdIe) -> RadioResult<()> {
        self.sup.request(&format!("WFD_SUBELEM_SET all {}", ie.to_hex()))?;
        Ok(())
    }

    /// The Wi-Fi Direct group interfaces that currently exist.
    ///
    /// The group interface is how the daemon learns that a group formed:
    /// the `P2P-GROUP-STARTED` event is written to the supplicant's log
    /// rather than to the control socket, because the group interface is
    /// an internal device with no control socket of its own. Watching the
    /// network interfaces is unprivileged and does not depend on how the
    /// supplicant chooses to publish its events.
    pub fn p2p_group_interfaces(&self) -> Vec<String> {
        let mut interfaces = Vec::new();
        let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
            return interfaces;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("p2p-") {
                interfaces.push(name);
            }
        }
        interfaces.sort();
        interfaces
    }

    /// The IPv4 address of an interface.
    pub fn interface_address(interface: &str) -> Option<std::net::Ipv4Addr> {
        if_addrs::get_if_addrs()
            .ok()?
            .into_iter()
            .find(|addr| addr.name == interface)
            .and_then(|addr| match addr.addr {
                if_addrs::IfAddr::V4(v4) => Some(v4.ip),
                _ => None,
            })
    }

    /// Wait for a new Wi-Fi Direct group interface to appear.
    ///
    /// Returns the interface name and its address once both are known.
    pub fn wait_for_group_interface(
        &mut self,
        before: &[String],
        timeout: std::time::Duration,
    ) -> RadioResult<Option<(String, std::net::Ipv4Addr)>> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            for interface in self.p2p_group_interfaces() {
                if before.contains(&interface) {
                    continue;
                }
                if let Some(address) = Self::interface_address(&interface) {
                    return Ok(Some((interface, address)));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Ok(None)
    }

    /// Wait for a Wi-Fi Direct group to form, collecting the events.
    ///
    /// Returns the group when it starts, or `None` if the deadline passes
    /// or the formation fails.
    pub fn wait_for_group(
        &mut self,
        timeout: std::time::Duration,
    ) -> RadioResult<Option<crate::p2p::GroupStarted>> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            match self.sup.poll_event(std::time::Duration::from_millis(500)) {
                Ok(Some(line)) => match crate::p2p::Event::parse(&line) {
                    Some(crate::p2p::Event::GroupStarted(group)) => {
                        return Ok(Some(group));
                    }
                    Some(crate::p2p::Event::GroupFormationFailure(_)) => {
                        return Ok(None);
                    }
                    _ => {}
                },
                Ok(None) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    /// The radio-work queue.
    pub fn radio_work(&mut self) -> RadioResult<Vec<RadioWork>> {
        let out = self.sup.request("RADIO_WORK show")?;
        Ok(out.lines().filter_map(RadioWork::parse).collect())
    }

    /// Detect a wedged queue: a scan work item that started and never
    /// finished.
    pub fn wedge(&mut self, threshold: Duration) -> RadioResult<Option<Wedge>> {
        let threshold = threshold.as_secs_f64();
        for work in self.radio_work()? {
            if work.started && work.age_secs > threshold {
                let explanation = format!(
                    "a {:?} work item has been in flight for {:.0}s without completing; \
                     every later scan request is rejected with 'Reject scan trigger since \
                     one is already pending'",
                    work.kind, work.age_secs
                );
                return Ok(Some(Wedge { work, explanation }));
            }
        }
        Ok(None)
    }

    /// Station association status.
    pub fn status(&mut self) -> RadioResult<StationStatus> {
        let out = self.sup.request("STATUS")?;
        let mut status = StationStatus::default();
        // The status document is one `key=value` per line.
        for line in out.lines() {
            for (key, value) in super::discovery::parse_key_values(line) {
                match key.as_str() {
                    "ssid" => status.ssid = Some(value),
                    "bssid" => status.bssid = Some(value),
                    "freq" => status.freq = value.parse().ok(),
                    "wpa_state" => {
                        status.connected = value == "COMPLETED";
                        status.wpa_state = Some(value);
                    }
                    _ => {}
                }
            }
        }
        Ok(status)
    }

    /// Flush the radio-work queue and wait for the station to come back.
    ///
    /// The flush aborts the stuck scan but also drops the association;
    /// NetworkManager reactivates the connection on its own, which normally
    /// takes a few seconds.
    pub fn recover(&mut self) -> RadioResult<Recovery> {
        let before = self.radio_work()?.len();
        self.sup.request("FLUSH")?;

        let start = Instant::now();
        let mut connected = false;
        while start.elapsed() < Duration::from_secs(30) {
            if let Ok(status) = self.status() {
                if status.connected {
                    connected = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let after = self.radio_work()?.len();
        Ok(Recovery {
            work_items_before: before,
            work_items_after: after,
            station_connected: connected,
            reconnected_after_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Start a Wi-Fi Direct device search.
    pub fn p2p_find(&mut self, seconds: u32) -> RadioResult<()> {
        self.sup.request(&format!("P2P_FIND {seconds}"))?;
        Ok(())
    }

    /// Stop a Wi-Fi Direct device search.
    pub fn p2p_stop_find(&mut self) -> RadioResult<()> {
        self.sup.request("P2P_STOP_FIND")?;
        Ok(())
    }

    /// Known Wi-Fi Direct peer addresses.
    ///
    /// The control interface has no `P2P_PEERS` command; peers are walked
    /// with `P2P_PEER FIRST` and then `P2P_PEER NEXT-<address>`, exactly as
    /// `wpa_cli p2p_peers` does.
    pub fn p2p_peers(&mut self) -> RadioResult<Vec<String>> {
        let mut peers = Vec::new();
        let mut selector = "FIRST".to_string();
        loop {
            let reply = match self.sup.request(&format!("P2P_PEER {selector}")) {
                Ok(reply) => reply,
                // The handler answers FAIL once the list is exhausted.
                Err(crate::supplicant::Error::CommandFailed { .. }) => break,
                Err(e) => return Err(e.into()),
            };
            let address = reply.lines().next().unwrap_or("").trim().to_string();
            if address.is_empty() {
                break;
            }
            peers.push(address.clone());
            selector = format!("NEXT-{address}");
            if peers.len() > 64 {
                break;
            }
        }
        Ok(peers)
    }

    /// Become the Group Owner on our own, optionally on a given frequency.
    ///
    /// The frequency matters on a single-radio adapter: a group that
    /// cannot share the station's channel fails with `-2` (channel
    /// unavailability).
    pub fn p2p_group_add(&mut self, freq: Option<u32>) -> RadioResult<()> {
        let command = match freq {
            Some(freq) => format!("P2P_GROUP_ADD freq={freq}"),
            None => "P2P_GROUP_ADD".to_string(),
        };
        self.sup.request(&command)?;
        Ok(())
    }

    /// Stay discoverable as a Wi-Fi Direct device.
    ///
    /// This is the correct shape for a Miracast source: the **sink**
    /// initiates the connection, and wpa_supplicant answers the group
    /// negotiation, making us the Group Owner with the sink attached. An
    /// autonomous group (`P2P_GROUP_ADD`) is the opposite — it is removed
    /// again as soon as it has no client.
    pub fn p2p_listen(&mut self) -> RadioResult<()> {
        self.sup.request("P2P_LISTEN")?;
        Ok(())
    }

    /// Remove leftover Wi-Fi Direct group interfaces.
    ///
    /// Tearing a group down can leave the kernel network interface behind
    /// on this driver, and the *next* group creation then fails with
    /// "Name not unique on network". Deleting the netdev needs
    /// CAP_NET_ADMIN, so this is best effort and the caller has to report
    /// what it could not do.
    pub fn remove_stale_group_interfaces(&self) -> RadioResult<Vec<String>> {
        let mut removed = Vec::new();
        for interface in self.p2p_group_interfaces() {
            let deleted = std::process::Command::new("iw")
                .arg("dev")
                .arg(&interface)
                .arg("del")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if let Ok(status) = deleted {
                if status.success() {
                    removed.push(interface);
                }
            }
        }
        Ok(removed)
    }

    /// Tear the group down.
    pub fn p2p_group_remove(&mut self) -> RadioResult<()> {
        self.sup.request("P2P_GROUP_REMOVE *")?;
        Ok(())
    }

    /// The details of one group member.
    pub fn p2p_group_member(&mut self, address: &str) -> RadioResult<String> {
        Ok(self.sup.request(&format!("P2P_GROUP_MEMBER {address}"))?)
    }

    /// Check the peer's provisioning method.
    pub fn p2p_prov_disc(&mut self, address: &str) -> RadioResult<String> {
        Ok(self.sup.request(&format!("P2P_PROV_DISC {address} pbc"))?)
    }

    /// Invite a peer into a group we own.
    pub fn p2p_connect(&mut self, address: &str) -> RadioResult<()> {
        self.sup.request(&format!("P2P_CONNECT {address} pbc"))?;
        Ok(())
    }

    /// Details for one peer.
    ///
    /// The reply is a document: the peer's P2P device address on the first
    /// line, then `key=value` lines.
    pub fn p2p_peer(&mut self, address: &str) -> RadioResult<String> {
        Ok(self.sup.request(&format!("P2P_PEER {address}"))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_radio_work_line() {
        let work = RadioWork::parse("p2p-scan@wlan0:0:1:12737.225102").unwrap();
        assert_eq!(work.kind, "p2p-scan");
        assert_eq!(work.iface, "wlan0");
        assert_eq!(work.freq, 0);
        assert!(work.started);
        assert!((work.age_secs - 12737.225102).abs() < 0.001);
    }

    #[test]
    fn parses_a_queued_scan() {
        let work = RadioWork::parse("scan@wlan0:0:0:12736.815721").unwrap();
        assert_eq!(work.kind, "scan");
        assert!(!work.started);
    }

    #[test]
    fn ignores_garbage() {
        assert!(RadioWork::parse("").is_none());
        assert!(RadioWork::parse("not-a-work-line").is_none());
    }
}
