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
    /// Parse a `P2P-DEVICE-FOUND` event.
    ///
    /// The event is shaped like
    /// `<level>P2P-DEVICE-FOUND <source address> p2p_dev_addr=<p2p device
    /// address> pri_dev_type=... name='...' ... wfd_dev_info=0x...`, where the
    /// bare address is the address the frame came from and `p2p_dev_addr` is
    /// the peer's Wi-Fi Direct device address.
    ///
    /// The event name is the string on the wire, not the `wpa_ctrl.h` macro
    /// name (`P2P_EVENT_DEVICE_FOUND`). The macro expands to this string; a
    /// parser that matches the macro name never sees an event.
    pub fn parse_event(line: &str) -> Option<Self> {
        if !line.contains("P2P-DEVICE-FOUND") {
            return None;
        }
        let mut peer = Peer {
            raw: line.to_string(),
            ..Default::default()
        };

        // The first token after the event name is the source address.
        let after = line.split_once("P2P-DEVICE-FOUND")?.1;
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
    ///
    /// That output is line-oriented and unquoted (`device_name=Living Room
    /// TV`), so the value is everything after the first `=` on the line, not
    /// the next whitespace-delimited token. Fields are named `listen_freq`
    /// and `oper_freq` here, and the WFD element is the whole subelement list
    /// (`wfd_subelems`), not the device-information body the event carries.
    pub fn merge_fields(&mut self, text: &str) {
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('\'');
            match key {
                "p2p_device_address" | "p2p_dev_addr" => {
                    if self.p2p_device_address.is_empty() {
                        self.p2p_device_address = value.to_string();
                    }
                }
                "device_address" => self.device_address = Some(value.to_string()),
                "name" | "device_name" if !value.is_empty() => {
                    self.name = Some(value.to_string());
                }
                "listen_freq" | "src_freq" => {
                    if let Ok(freq) = value.parse::<u32>() {
                        if freq > 0 {
                            self.src_freq = Some(freq);
                        }
                    }
                }
                // The operating channel is a fallback for devices that are
                // already in a group and do not report a listen channel.
                "oper_freq" if self.src_freq.is_none() => {
                    if let Ok(freq) = value.parse::<u32>() {
                        if freq > 0 {
                            self.src_freq = Some(freq);
                        }
                    }
                }
                "wfd_subelems" if self.wfd_dev_info.is_none() => {
                    self.wfd_dev_info = parse_wfd_subelems(value);
                }
                "wfd_dev_info" if self.wfd_dev_info.is_none() => {
                    self.wfd_dev_info = parse_wfd_dev_info(value);
                }
                _ => {}
            }
        }
    }

    /// True when the peer advertised a WFD information element.
    pub fn is_wfd(&self) -> bool {
        self.wfd_dev_info.is_some()
    }

    /// Build a peer from a `P2P_PEER <address>` document.
    ///
    /// This is the fallback that does not depend on events: the supplicant's
    /// peer table is populated by its own scan, and `P2P_PEER` returns it even
    /// when the event for that peer was missed. A peer the supplicant knows
    /// about must appear in our list.
    pub fn from_details(address: &str, details: &str) -> Self {
        let mut peer = Peer {
            p2p_device_address: address.to_string(),
            raw: details.to_string(),
            ..Default::default()
        };
        peer.merge_fields(details);
        peer
    }
}

/// Decode a hex payload, with or without a `0x` prefix.
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let hex = value.trim().trim_start_matches("0x");
    if hex.is_empty() || !hex.is_ascii() || hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// Parse the hex payload of a `wfd_dev_info=` field.
///
/// `wpa_supplicant` reports the *body* of the device information subelement,
/// which is the 2-byte device information field followed by the optional
/// 2-byte control port and 2-byte maximum throughput.
fn parse_wfd_dev_info(value: &str) -> Option<SourceDeviceInfo> {
    SourceDeviceInfo::parse(&decode_hex(value)?)
}

/// Parse a `wfd_subelems=` field from a `P2P_PEER` document.
///
/// That field is the whole WFD information element payload: a sequence of
/// `[subelement id][length][body]` records. Subelement 0 is the device
/// information. The event's `wfd_dev_info` field is that subelement's body
/// alone, which is why the two need different parsers — reading the list as
/// if it were the body would take the subelement id and length for the
/// device-information field.
fn parse_wfd_subelems(value: &str) -> Option<SourceDeviceInfo> {
    let bytes = decode_hex(value)?;
    let mut i = 0;
    while i + 2 <= bytes.len() {
        let id = bytes[i];
        let len = bytes[i + 1] as usize;
        let start = i + 2;
        let end = start.checked_add(len)?;
        if end > bytes.len() {
            return None;
        }
        if id == 0 {
            return SourceDeviceInfo::parse(&bytes[start..end]);
        }
        i = end;
    }
    None
}

/// Parse `key=value` pairs out of a single-line `wpa_supplicant` event.
///
/// Values may be quoted with single quotes and contain spaces, as in
/// `name='Living Room TV'`. Separators are any whitespace, so a document with
/// one `key=value` per line also parses; callers that need unquoted values
/// containing spaces (the `P2P_PEER` document) split the lines themselves.
pub fn parse_key_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        // Find the start of a `key=`.
        while i < bytes.len() && bytes[i].is_whitespace() {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != '=' && !bytes[i].is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != '=' {
            // Not a key=value pair: skip to the next separator.
            while i < bytes.len() && !bytes[i].is_whitespace() {
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
            while i < bytes.len() && !bytes[i].is_whitespace() {
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
        // The real line `wpa_supplicant` sends: dashes, not the C macro name.
        let line = "<1>P2P-DEVICE-FOUND aa:bb:cc:dd:ee:ff p2p_dev_addr=11:22:33:44:55:66 pri_dev_type=11-0050F204-1 name='TV' config_methods=0x18 dev_capab=0x21 group_capab=0x1b wfd_dev_info=0x01111c4400c8";
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
    fn the_macro_spelling_is_not_an_event() {
        let line = "<1>P2P_EVENT_DEVICE_FOUND aa:bb:cc:dd:ee:ff p2p_dev_addr=11:22:33:44:55:66";
        assert!(Peer::parse_event(line).is_none());
    }

    #[test]
    fn key_values_can_span_lines() {
        let details = "11:22:33:44:55:66\npriority=0\nname='Living Room TV'\nsrc_freq=2462\nwfd_dev_info=0x01111c4400c8\n";
        let pairs = parse_key_values(details);
        assert!(pairs.contains(&("name".into(), "Living Room TV".into())));
        assert!(pairs.contains(&("src_freq".into(), "2462".into())));
        assert!(pairs.contains(&("wfd_dev_info".into(), "0x01111c4400c8".into())));
    }

    #[test]
    fn a_peer_is_built_from_the_enumerated_address_alone() {
        let peer = Peer::from_details("11:22:33:44:55:66", "");
        assert_eq!(peer.p2p_device_address, "11:22:33:44:55:66");
        assert!(!peer.is_wfd());
    }

    #[test]
    fn a_peer_document_fills_in_its_details() {
        // The real shape of `P2P_PEER <addr>`: one field per line, an
        // unquoted `device_name`, `listen_freq`, and the whole WFD
        // subelement list rather than the event's device-info body.
        let details = "11:22:33:44:55:66\n\
                       pri_dev_type=11-0050F204-1\n\
                       device_name=Living Room TV\n\
                       config_methods=0x18\n\
                       listen_freq=2462\n\
                       wfd_subelems=000601111c4400c8\n";
        let peer = Peer::from_details("11:22:33:44:55:66", details);
        assert_eq!(peer.name.as_deref(), Some("Living Room TV"));
        assert_eq!(peer.src_freq, Some(2462));
        assert!(peer.is_wfd());
        let info = peer.wfd_dev_info.unwrap();
        assert_eq!(info.device.device_type, crate::wfd::DeviceType::PrimarySink);
        assert_eq!(info.control_port, 7236);
        assert_eq!(info.max_throughput_mbps, 200);
    }

    #[test]
    fn the_subelement_list_is_not_read_as_the_device_information_body() {
        // A naive parser takes the first four bytes (00 06 01 11) for the
        // device-information field and loses the role; the list has to be
        // walked.
        let info = parse_wfd_subelems("000601111c4400c8").unwrap();
        assert_eq!(info.device.device_type, crate::wfd::DeviceType::PrimarySink);
        assert_eq!(info.control_port, 7236);
    }

    #[test]
    fn the_device_information_subelement_is_found_after_another_one() {
        // Subelement 1 (associated BSSID), then subelement 0.
        let info = parse_wfd_subelems("0106aabbccddeeff000601111c4400c8").unwrap();
        assert_eq!(info.control_port, 7236);
    }

    #[test]
    fn a_truncated_subelement_list_is_rejected() {
        assert!(parse_wfd_subelems("000a0111").is_none());
        assert!(parse_wfd_subelems("zz").is_none());
    }

    #[test]
    fn the_listen_channel_wins_over_the_operating_channel() {
        let peer = Peer::from_details("11:22:33:44:55:66", "oper_freq=2437\nlisten_freq=2462\n");
        assert_eq!(peer.src_freq, Some(2462));
    }

    #[test]
    fn the_operating_channel_is_a_fallback() {
        let peer = Peer::from_details("11:22:33:44:55:66", "oper_freq=2437\n");
        assert_eq!(peer.src_freq, Some(2437));
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
