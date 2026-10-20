//! The Wi-Fi Display information element, and the device information field.
//!
//! The field is two bytes, and **the two readings of the specification
//! disagree about which octet holds the low bits** — so the same bytes mean
//! different things to different implementations. AetherCast therefore
//! decodes under both readings ([`decode_layouts`]) and advertises a
//! **symmetric** value, so that no reading can be more wrong than another.
//!
//! The bits this module interprets are:
//!
//! | bits | meaning |
//! | --- | --- |
//! | 1:0 | device type: source, primary sink, secondary sink, dual |
//! | 2 | coupled-sink support at source |
//! | 3 | coupled-sink support at sink |
//! | 5:4 | session availability (`01` = available) |
//! | 6 | WSD (wireless session discovery) available |
//! | 7 | PC — preferred connectivity, i.e. TDLS |
//! | 8 | HDCP 2.x supported |
//! | 9 | 802.1AS time synchronisation supported |
//! | 10 | audio unsupported (source) |
//! | 11 | audio only (sink) |
//!
//! Bits 12 and above are not interpreted. One of them is set anyway — see
//! [`DeviceInfo::aethercast_source`] — so that the two readings of the field
//! cannot disagree about whether a session is available.

use std::fmt;

/// WFD subelement identifiers.
///
/// The full table is kept as a reference for the parts of the specification
/// AetherCast will implement next; not every id is used yet.
#[allow(dead_code)]
pub mod id {
    pub const DEVICE_INFO: u8 = 0;
    pub const ASSOCIATED_BSSID: u8 = 1;
    pub const AUDIO_FORMATS: u8 = 2;
    pub const VIDEO_FORMATS: u8 = 3;
    pub const VIDEO_FORMATS_3D: u8 = 4;
    pub const CONTENT_PROTECTION: u8 = 5;
    pub const COUPLED_SINK: u8 = 6;
    pub const EXTENDED_CAPABILITY: u8 = 7;
    pub const LOCAL_IP_ADDRESS: u8 = 8;
    pub const SESSION_INFO: u8 = 9;
    pub const MAC_INFO: u8 = 10;
    pub const R2_DEVICE_INFO: u8 = 11;
}

/// The default WFD session management control port (RTSP).
pub const DEFAULT_CONTROL_PORT: u16 = 7236;

/// The bits this module interprets.
const KNOWN_BITS: u16 = 0b0000_1111_1111_1111;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("WFD IE is truncated at offset {offset}")]
    Truncated { offset: usize },
    #[error("WFD subelement {id} claims {len} bytes but only {available} remain")]
    LengthMismatch {
        id: u8,
        len: usize,
        available: usize,
    },
    #[error("invalid hex in WFD IE: {0}")]
    Hex(#[from] std::num::ParseIntError),
    #[error("odd-length hex string")]
    OddLength,
}

/// A single WFD subelement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subelement {
    pub id: u8,
    pub body: Vec<u8>,
}

/// The body of a WFD IE: an ordered list of subelements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WfdIe {
    pub subelements: Vec<Subelement>,
}

impl WfdIe {
    /// Parse the body of a WFD IE.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut subelements = Vec::new();
        let mut offset = 0usize;
        while offset < bytes.len() {
            if offset + 3 > bytes.len() {
                return Err(Error::Truncated { offset });
            }
            let id = bytes[offset];
            let len = u16::from_be_bytes([bytes[offset + 1], bytes[offset + 2]]) as usize;
            let start = offset + 3;
            let end = start + len;
            if end > bytes.len() {
                return Err(Error::LengthMismatch {
                    id,
                    len,
                    available: bytes.len() - start,
                });
            }
            subelements.push(Subelement {
                id,
                body: bytes[start..end].to_vec(),
            });
            offset = end;
        }
        Ok(Self { subelements })
    }

    /// Serialise back to the byte string `wpa_supplicant` expects.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for sub in &self.subelements {
            out.push(sub.id);
            out.extend_from_slice(&(sub.body.len() as u16).to_be_bytes());
            out.extend_from_slice(&sub.body);
        }
        out
    }

    /// The body of the first subelement with `id`.
    pub fn get(&self, id: u8) -> Option<&[u8]> {
        self.subelements
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.body.as_slice())
    }

    /// Parse a hex string as produced by `WFD_SUBELEM_GET all`.
    pub fn parse_hex(hex: &str) -> Result<Self, Error> {
        let hex = hex.trim();
        if hex.is_empty() {
            return Ok(Self::default());
        }
        if hex.len() % 2 != 0 {
            return Err(Error::OddLength);
        }
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for i in (0..hex.len()).step_by(2) {
            bytes.push(u8::from_str_radix(&hex[i..i + 2], 16)?);
        }
        Self::parse(&bytes)
    }

    /// Render as the hex string `WFD_SUBELEM_SET all` expects.
    pub fn to_hex(&self) -> String {
        self.to_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The device information this IE carries, if any.
    pub fn device_info(&self) -> Option<DeviceInfo> {
        let body = self.get(id::DEVICE_INFO)?;
        DeviceInfo::parse(body)
    }
}

impl fmt::Display for WfdIe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for sub in &self.subelements {
            writeln!(
                f,
                "  id={} len={} body={}",
                sub.id,
                sub.body.len(),
                sub.body
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )?;
        }
        Ok(())
    }
}

/// The role a WFD device plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum DeviceType {
    Source,
    PrimarySink,
    SecondarySink,
    Dual,
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DeviceType::Source => "source",
            DeviceType::PrimarySink => "primary sink",
            DeviceType::SecondarySink => "secondary sink",
            DeviceType::Dual => "dual",
        };
        f.write_str(s)
    }
}

/// The 2-byte WFD Device Information field.
///
/// `uninterpreted` holds the bits this module does not decode. They are
/// preserved so that a field can be read and written again without losing
/// anything, and so that a symmetric field can be expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct DeviceInfo {
    pub device_type: DeviceType,
    pub coupled_sink_at_source: bool,
    pub coupled_sink_at_sink: bool,
    pub session_available: bool,
    pub wsd_available: bool,
    pub prefer_tdls: bool,
    pub hdcp2_supported: bool,
    pub time_sync_supported: bool,
    pub audio_unsupported: bool,
    pub audio_only: bool,
    /// Bits this module does not interpret, preserved verbatim.
    pub uninterpreted: u16,
}

impl DeviceInfo {
    /// The field AetherCast advertises: `0x1010`.
    ///
    /// Symmetric on purpose. The two readings of the specification place the
    /// low bits in different octets, and a source that claims "no session
    /// available" under the reading the sink happens to use is simply
    /// ignored. Setting the same value in both octets means every reading
    /// sees the same thing: a source, session available, no TDLS
    /// preference, no HDCP claim, no WSD claim.
    ///
    /// The high bit of the high octet is set to make the two octets equal;
    /// it is in the range this module does not interpret, and a sink that
    /// ignores uninterpreted bits — which is the common case — is unaffected.
    pub const AETHERCAST: [u8; 2] = [0x10, 0x10];

    /// The device information field AetherCast advertises.
    pub fn aethercast_source() -> Self {
        // The constant is the source of truth, and the decoder is exercised
        // to produce it.
        Self::from_raw(u16::from_be_bytes(Self::AETHERCAST))
    }

    /// Parse the device information field.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }
        let raw = u16::from_be_bytes([bytes[0], bytes[1]]);
        Some(Self::from_raw(raw))
    }

    /// Interpret a raw 16-bit field.
    pub fn from_raw(raw: u16) -> Self {
        Self {
            device_type: match raw & 0b11 {
                0b00 => DeviceType::Source,
                0b01 => DeviceType::PrimarySink,
                0b10 => DeviceType::SecondarySink,
                _ => DeviceType::Dual,
            },
            coupled_sink_at_source: raw & (1 << 2) != 0,
            coupled_sink_at_sink: raw & (1 << 3) != 0,
            session_available: (raw >> 4) & 0b11 == 0b01,
            wsd_available: raw & (1 << 6) != 0,
            prefer_tdls: raw & (1 << 7) != 0,
            hdcp2_supported: raw & (1 << 8) != 0,
            time_sync_supported: raw & (1 << 9) != 0,
            audio_unsupported: raw & (1 << 10) != 0,
            audio_only: raw & (1 << 11) != 0,
            uninterpreted: raw & !KNOWN_BITS,
        }
    }

    /// The raw 16-bit field.
    pub fn to_raw(self) -> u16 {
        let mut raw: u16 = match self.device_type {
            DeviceType::Source => 0b00,
            DeviceType::PrimarySink => 0b01,
            DeviceType::SecondarySink => 0b10,
            DeviceType::Dual => 0b11,
        };
        if self.coupled_sink_at_source {
            raw |= 1 << 2;
        }
        if self.coupled_sink_at_sink {
            raw |= 1 << 3;
        }
        if self.session_available {
            raw |= 0b01 << 4;
        }
        if self.wsd_available {
            raw |= 1 << 6;
        }
        if self.prefer_tdls {
            raw |= 1 << 7;
        }
        if self.hdcp2_supported {
            raw |= 1 << 8;
        }
        if self.time_sync_supported {
            raw |= 1 << 9;
        }
        if self.audio_unsupported {
            raw |= 1 << 10;
        }
        if self.audio_only {
            raw |= 1 << 11;
        }
        raw | self.uninterpreted
    }

    /// Serialise back to the 2-byte field.
    pub fn to_bytes(self) -> [u8; 2] {
        self.to_raw().to_be_bytes()
    }
}

/// The full source-side device information subelement body.
///
/// Six bytes: the 2-byte device information field, the 2-byte session
/// management control port, and the 2-byte maximum throughput in Mbit/s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SourceDeviceInfo {
    pub device: DeviceInfo,
    pub control_port: u16,
    pub max_throughput_mbps: u16,
}

impl SourceDeviceInfo {
    pub fn new(device: DeviceInfo, control_port: u16, max_throughput_mbps: u16) -> Self {
        Self {
            device,
            control_port,
            max_throughput_mbps,
        }
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let device = DeviceInfo::parse(bytes)?;
        let control_port = if bytes.len() >= 4 {
            u16::from_be_bytes([bytes[2], bytes[3]])
        } else {
            DEFAULT_CONTROL_PORT
        };
        let max_throughput_mbps = if bytes.len() >= 6 {
            u16::from_be_bytes([bytes[4], bytes[5]])
        } else {
            0
        };
        Some(Self {
            device,
            control_port,
            max_throughput_mbps,
        })
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = self.device.to_bytes().to_vec();
        out.extend_from_slice(&self.control_port.to_be_bytes());
        out.extend_from_slice(&self.max_throughput_mbps.to_be_bytes());
        out
    }
}

/// Which bit layout a device information field uses.
///
/// The Wi-Fi Display specification numbers the bits of the 2-byte field from
/// the least significant end, but the two readings that circulate disagree
/// about *which octet* the low bits live in. Rather than guess, AetherCast
/// decodes **both** and lets a real sink's information element settle it: a
/// sink must say "primary sink" and "session available", so only one of the
/// two readings can be right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Layout {
    /// Bits 0-7 live in the low octet.
    LsbInLowOctet,
    /// Bits 0-7 live in the high octet.
    LsbInHighOctet,
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Layout::LsbInLowOctet => "bits 0-7 in the low octet",
            Layout::LsbInHighOctet => "bits 0-7 in the high octet",
        })
    }
}

/// Decode a device information field under both candidate layouts.
pub fn decode_layouts(bytes: &[u8]) -> Vec<(Layout, DeviceInfo)> {
    let mut out = Vec::new();
    if bytes.len() >= 2 {
        out.push((Layout::LsbInLowOctet, decode_with(bytes, true)));
        out.push((Layout::LsbInHighOctet, decode_with(bytes, false)));
    }
    out
}

/// Decode under one layout: `low_octet` says whether bits 0-7 are the low
/// octet.
fn decode_with(bytes: &[u8], low_octet: bool) -> DeviceInfo {
    let raw = if low_octet {
        u16::from_be_bytes([bytes[0], bytes[1]])
    } else {
        u16::from_be_bytes([bytes[1], bytes[0]])
    };
    DeviceInfo::from_raw(raw)
}

/// Build a source WFD IE with a raw device information field.
///
/// This is the escape hatch for interop: when a sink rejects our element,
/// we can advertise exactly the same two bytes the sink itself uses.
pub fn source_ie_with_device_info(
    device_info: [u8; 2],
    control_port: u16,
    throughput: u16,
) -> WfdIe {
    let mut body = device_info.to_vec();
    body.extend_from_slice(&control_port.to_be_bytes());
    body.extend_from_slice(&throughput.to_be_bytes());
    WfdIe {
        subelements: vec![Subelement {
            id: id::DEVICE_INFO,
            body,
        }],
    }
}

/// AetherCast's own WFD IE, as a source.
pub fn source_ie(control_port: u16, max_throughput_mbps: u16) -> WfdIe {
    let info = SourceDeviceInfo::new(
        DeviceInfo::aethercast_source(),
        control_port,
        max_throughput_mbps,
    );
    WfdIe {
        subelements: vec![Subelement {
            id: id::DEVICE_INFO,
            body: info.to_bytes(),
        }],
    }
}

/// The WFD IE that GNOME Network Displays 0.99.0 ships, kept for comparison.
///
/// Decodes as: source, session available, **PC/TDLS preferred** — which is
/// incorrect for a P2P-only implementation.
pub fn gnd_ie() -> WfdIe {
    source_ie_with_device_info([0x00, 0x90], 7236, 200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_a_wfd_ie_body() {
        let ie = source_ie(DEFAULT_CONTROL_PORT, 200);
        let bytes = ie.to_bytes();
        // subelement 0, length 6, body
        assert_eq!(bytes[0], 0x00);
        assert_eq!(&bytes[1..3], &[0x00, 0x06]);
        assert_eq!(bytes.len(), 9);
        let parsed = WfdIe::parse(&bytes).unwrap();
        assert_eq!(parsed, ie);
        assert_eq!(WfdIe::parse_hex(&ie.to_hex()).unwrap(), ie);
    }

    #[test]
    fn our_element_reads_the_same_under_both_layouts() {
        let ours = source_ie(DEFAULT_CONTROL_PORT, 200);
        let field = &ours.to_bytes()[3..5];
        assert_eq!(field, &DeviceInfo::AETHERCAST);
        let layouts = decode_layouts(field);
        let (_, first) = &layouts[0];
        let (_, second) = &layouts[1];
        assert_eq!(first, second, "the two readings must agree");
        assert_eq!(first.device_type, DeviceType::Source);
        assert!(first.session_available);
        assert!(!first.prefer_tdls);
    }

    #[test]
    fn gnd_s_element_does_not_read_the_same_under_both_layouts() {
        let layouts = decode_layouts(&[0x00, 0x90]);
        assert_ne!(layouts[0].1, layouts[1].1);
        // Under the specification's drawing it claims no session.
        assert!(!layouts[1].1.session_available);
    }

    #[test]
    fn our_ie_differs_from_gnd_only_in_the_device_information_field() {
        let ours = source_ie(DEFAULT_CONTROL_PORT, 200);
        let gnd = gnd_ie();
        // The control port and throughput are identical.
        assert_eq!(ours.to_bytes()[5..], gnd.to_bytes()[5..]);
        assert_eq!(&ours.to_bytes()[3..5], &DeviceInfo::AETHERCAST);
        assert_eq!(&gnd.to_bytes()[3..5], &[0x00, 0x90]);
    }

    #[test]
    fn decodes_the_gnd_ie() {
        let info = SourceDeviceInfo::parse(&gnd_ie().to_bytes()[3..]).unwrap();
        assert_eq!(info.device.device_type, DeviceType::Source);
        assert!(info.device.session_available);
        assert!(info.device.prefer_tdls, "GND advertises TDLS preference");
        assert_eq!(info.control_port, 7236);
        assert_eq!(info.max_throughput_mbps, 200);
    }

    #[test]
    fn a_raw_device_information_field_can_be_advertised() {
        let ie = source_ie_with_device_info([0x11, 0x90], 7236, 200);
        assert_eq!(ie.to_hex(), "00000611901c4400c8");
        let parsed = WfdIe::parse(&ie.to_bytes()).unwrap();
        assert_eq!(parsed.device_info().unwrap().to_raw(), 0x1190);
    }

    #[test]
    fn the_uninterpreted_bits_survive_a_roundtrip() {
        let info = DeviceInfo::aethercast_source();
        let bytes = info.to_bytes();
        assert_eq!(DeviceInfo::parse(&bytes).unwrap(), info);
    }

    #[test]
    fn rejects_a_truncated_ie() {
        assert!(WfdIe::parse(&[0x00, 0x00, 0x06, 0x01]).is_err());
        assert!(WfdIe::parse(&[0x00]).is_err());
    }
}
