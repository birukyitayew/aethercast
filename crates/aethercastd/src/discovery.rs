//! Peer discovery: turning `wpa_supplicant` output into typed peers.

use crate::wfd::SourceDeviceInfo;

/// A Wi-Fi Direct peer that may be a Miracast sink.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Peer {
    /// The peer's P2P device address.
    pub p2p_device_address: String,
    /// The peer's infrastructure address, when it has one.
    pub device_address: Option<String>,
    /// A human-readable name, when the peer advertises one.
    pub name: Option<String>,
    /// The frequency the peer is operating on, when known.
    pub src_freq: Option<u32>,
    /// The WFD device information the peer advertised, when present.
    pub wfd_dev_info: Option<SourceDeviceInfo>,
    /// The raw line the peer was parsed from.
    pub raw: String,
}

impl Peer {
    /// Parse a `P2P_EVENT_DEVICE_FOUND` event.
    ///
    /// The event is shaped like
    /// `<level>P2P_EVENT_DEVICE_FOUND <source address> p2p_dev_addr=<p2p device
    /// address> pri_dev_type=... name='...' ... wfd_dev_info=0x...`, where the
    /// bare address is the address the frame came from and `p2p_dev_addr` is
    /// the peer's Wi-Fi Direct device address.
    pub fn parse_event(line: &str) -> Option<Self> {
        if !line.contains("P2P_EVENT_DEVICE_FOUND") {
            return None;
        }
        let mut peer = Peer {
            raw: line.to_string(),
            ..Default::default()
        };

        // The first token after the event name is the source address.
        let after = line.split_once("P2P_EVENT_DEVICE_FOUND")?.1;
        if let Some(token) = after.split_whitespace().next() {
            if token.contains(':') {
                peer.device_address = Some(token.to_string());
            }
        }

        for (key, value) in parse_key_values(after) {
            match key.as_str() {
                "p2p_dev_addr" | "p2p_device_addr" => peer.p2p_device_address = value,
                "device_address" | "src_addr" => peer.device_address = Some(value),
                "name" | "device_name" => peer.name = Some(value),
                "src_freq" => peer.src_freq = value.parse().ok(),
                "wfd_dev_info" => {
                    peer.wfd_dev_info = parse_wfd_dev_info(&value);
                }
                _ => {}
            }
        }
        if peer.p2p_device_address.is_empty() {
            return None;
        }
        Some(peer)
    }

    /// Merge additional fields from `P2P_PEER <addr>` output into this peer.
    pub fn merge_fields(&mut self, text: &str) {
        for (key, value) in parse_key_values(text) {
            match key.as_str() {
                "p2p_device_address" | "p2p_dev_addr" => {
                    if self.p2p_device_address.is_empty() {
                        self.p2p_device_address = value;
                    }
                }
                "device_address" => self.device_address = Some(value),
                "name" | "device_name" => self.name = Some(value),
                "src_freq" => self.src_freq = value.parse().ok(),
                "wfd_dev_info" | "wfd_ies" if self.wfd_dev_info.is_none() => {
                    self.wfd_dev_info = parse_wfd_dev_info(&value);
                }
                _ => {}
            }
        }
    }

    /// True when the peer advertised a WFD information element.
    pub fn is_wfd(&self) -> bool {
        self.wfd_dev_info.is_some()
    }
}

/// Parse the hex payload of a `wfd_dev_info=` field.
///
/// `wpa_supplicant` reports the *body* of the device information subelement,
/// which is the 2-byte device information field followed by the 2-byte
/// control port and the 2-byte maximum throughput.
fn parse_wfd_dev_info(value: &str) -> Option<SourceDeviceInfo> {
    let hex = value.trim().trim_start_matches("0x");
    if hex.is_empty() {
        return None;
    }
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&hex[i..i + 2], 16).ok()?);
    }
    SourceDeviceInfo::parse(&bytes)
}

/// Parse `key=value` pairs out of a `wpa_supplicant` text line.
///
/// Values may be quoted with single quotes and contain spaces, as in
/// `name='Living Room TV'`.
pub fn parse_key_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        // Find the start of a `key=`.
        while i < bytes.len() && bytes[i] == ' ' {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != '=' && bytes[i] != ' ' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != '=' {
            // Not a key=value pair: skip to the next space.
            while i < bytes.len() && bytes[i] != ' ' {
                i += 1;
            }
            continue;
        }
        let key: String = bytes[key_start..i].iter().collect();
        i += 1; // skip '='
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == '\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != '\'' {
                value.push(bytes[i]);
                i += 1;
            }
            i += 1; // skip closing quote
        } else {
            while i < bytes.len() && bytes[i] != ' ' {
                value.push(bytes[i]);
                i += 1;
            }
        }
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_values_with_quotes() {
        let pairs = parse_key_values(
            "P2P_EVENT_DEVICE_FOUND p2p_dev_addr=aa:bb:cc:dd:ee:ff name='Living Room TV' src_freq=2412",
        );
        assert_eq!(
            pairs[0],
            ("p2p_dev_addr".into(), "aa:bb:cc:dd:ee:ff".into())
        );
        assert_eq!(pairs[1], ("name".into(), "Living Room TV".into()));
        assert_eq!(pairs[2], ("src_freq".into(), "2412".into()));
    }

    #[test]
    fn parses_a_device_found_event() {
        let line = "<1>P2P_EVENT_DEVICE_FOUND aa:bb:cc:dd:ee:ff p2p_dev_addr=11:22:33:44:55:66 pri_dev_type=11-0050F204-1 name='TV' config_methods=0x18 dev_capab=0x21 group_capab=0x1b wfd_dev_info=0x01111c4400c8";
        let peer = Peer::parse_event(line).unwrap();
        assert_eq!(peer.device_address.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
        assert_eq!(peer.p2p_device_address, "11:22:33:44:55:66");
        assert_eq!(peer.name.as_deref(), Some("TV"));
        // The event itself does not carry the operating frequency; that
        // comes from the P2P_PEER detail output.
        assert_eq!(peer.src_freq, None);
        let info = peer.wfd_dev_info.unwrap();
        assert_eq!(info.device.device_type, crate::wfd::DeviceType::PrimarySink);
        assert_eq!(info.control_port, 7236);
    }

    #[test]
    fn ignores_events_that_are_not_device_found() {
        assert!(Peer::parse_event("<1>P2P-FIND-STOPPED").is_none());
    }

    #[test]
    fn parses_wfd_info_with_and_without_the_prefix() {
        assert!(parse_wfd_dev_info("0x1190").is_some());
        assert!(parse_wfd_dev_info("1190").is_some());
        assert!(parse_wfd_dev_info("").is_none());
    }
}
