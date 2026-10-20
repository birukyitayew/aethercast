//! Wi-Fi Display (WFD) information element handling.
//!
//! A WFD IE is a vendor-specific 802.11 IE: element id `0xDD`, OUI
//! `50-6F-9A`, OUI type `0x0A`, followed by subelements shaped as
//! `id(1) | length(2, big-endian) | body`.
//!
//! `wpa_supplicant` models the *body* of the IE: its `WFD_SUBELEM_SET all
//! <hex>` command takes exactly the concatenation of those subelements, and
//! `WFD_SUBELEM_GET all` returns them again. That is the byte string this
//! module serialises.
//!
//! Subelement ids follow `src/common/ieee802_11_defs.h` in wpa_supplicant
//! 2.10 and the WFD v2.1 specification, table 27.

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

/// Default WFD session management control port (RTSP).
pub const DEFAULT_CONTROL_PORT: u16 = 7236;

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
    #[allow(clippy::wrong_self_convention)]
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
        self.to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
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
#[allow(dead_code)] // `Unknown` is reserved for values the spec does not define.
pub enum DeviceType {
    Source,
    PrimarySink,
    SecondarySink,
    Dual,
    Unknown(u8),
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DeviceType::Source => "source",
            DeviceType::PrimarySink => "primary sink",
            DeviceType::SecondarySink => "secondary sink",
            DeviceType::Dual => "dual",
            DeviceType::Unknown(v) => return write!(f, "unknown({v})"),
        };
        f.write_str(s)
    }
}

/// The 2-byte WFD Device Information field.
///
/// The field is a big-endian 16-bit value, and the specification numbers its
/// bits from the **least** significant end, so bits 0..8 live in the second
/// octet:
///
/// | bits | meaning |
/// | --- | --- |
/// | 1:0 | device type: source / primary sink / secondary sink / dual |
/// | 2 | coupled-sink support at source |
/// | 3 | coupled-sink support at sink |
/// | 5:4 | session availability (`01` = available) |
/// | 6 | WSD (wireless session discovery) available |
/// | 7 | PC — preferred connectivity, i.e. TDLS |
/// | 8 | HDCP 2.x supported |
/// | 9 | 802.1AS time synchronisation supported |
/// | 10 | audio unsupported (source) |
/// | 11 | audio only (sink) |
///
/// This reading is the one that makes GNOME Network Displays' own element
/// decode consistently (`0x0090` → source, session available, TDLS
/// preferred), and it is documented in
/// [`docs/research/01-miracast-wfd.md`] §3.3.
///
/// The WFD protocol version field is **not** decoded here: its bit position
/// could not be confirmed against the specification text, and guessing would
/// silently produce wrong data. The raw word is kept in [`DeviceInfo::raw`]
/// so the layout can be corrected once a real sink element has been captured.
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
    /// The raw 16-bit field, kept so the layout can be revised without
    /// losing information.
    pub raw: u16,
}

impl DeviceInfo {
    /// Parse the device information field.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }
        let raw = u16::from_be_bytes([bytes[0], bytes[1]]);
        Some(Self {
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
            raw,
        })
    }

    /// Build the device information field for a source that is available.
    ///
    /// `prefer_tdls` is false: AetherCast speaks Wi-Fi Direct, not TDLS, and
    /// advertising TDLS preference is exactly the defect the code audit
    /// found in GNOME Network Displays.
    pub fn source_available() -> Self {
        let mut info = Self {
            device_type: DeviceType::Source,
            coupled_sink_at_source: false,
            coupled_sink_at_sink: false,
            session_available: true,
            wsd_available: false,
            prefer_tdls: false,
            hdcp2_supported: false,
            time_sync_supported: false,
            audio_unsupported: false,
            audio_only: false,
            raw: 0,
        };
        info.raw = u16::from_be_bytes(info.to_bytes());
        info
    }

    /// Serialise back to the 2-byte field.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_bytes(&self) -> [u8; 2] {
        let mut raw: u16 = match self.device_type {
            DeviceType::Source => 0b00,
            DeviceType::PrimarySink => 0b01,
            DeviceType::SecondarySink => 0b10,
            DeviceType::Dual => 0b11,
            DeviceType::Unknown(v) => (v & 0b11) as u16,
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
        raw.to_be_bytes()
    }
}

/// The full source-side device information subelement body.
///
/// Six bytes: the 2-byte device information field, the 2-byte session
/// management control port, and the 2-byte maximum throughput in Mbit/s.
/// This is the layout GNOME Network Displays ships and the layout the WFD
/// specification describes for a source.
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

    #[allow(clippy::wrong_self_convention)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.device.to_bytes().to_vec();
        out.extend_from_slice(&self.control_port.to_be_bytes());
        out.extend_from_slice(&self.max_throughput_mbps.to_be_bytes());
        out
    }
}

/// AetherCast's own WFD IE, as a source.
pub fn source_ie(control_port: u16, max_throughput_mbps: u16) -> WfdIe {
    let info = SourceDeviceInfo::new(
        DeviceInfo::source_available(),
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
/// Decodes as: source, session available, **PC/TDLS bit set** — which is
/// incorrect for a P2P-only implementation — control port 7236, 200 Mbit/s.
pub fn gnd_ie() -> WfdIe {
    WfdIe {
        subelements: vec![Subelement {
            id: id::DEVICE_INFO,
            body: vec![0x00, 0x90, 0x1c, 0x44, 0x00, 0xc8],
        }],
    }
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
    fn our_ie_differs_from_gnd_only_in_the_tdls_bit() {
        let ours = source_ie(DEFAULT_CONTROL_PORT, 200);
        let gnd = gnd_ie();
        // Control port and throughput are identical.
        assert_eq!(ours.to_bytes()[5..], gnd.to_bytes()[5..]);
        // Ours clears bit 7 (PC / prefer TDLS) of the device information
        // field, which is the defect the code audit found in GNOME Network
        // Displays. The field is bytes 3..5, bits numbered from the LSB of
        // the low byte.
        let ours_device = u16::from_be_bytes([ours.to_bytes()[3], ours.to_bytes()[4]]);
        let gnd_device = u16::from_be_bytes([gnd.to_bytes()[3], gnd.to_bytes()[4]]);
        assert_eq!(ours_device & (1 << 7), 0);
        assert_eq!(gnd_device & (1 << 7), 1 << 7);
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
    fn decodes_a_sink_device_information_field() {
        // 0x0111: primary sink, session available.
        let info = DeviceInfo::parse(&[0x01, 0x11]).unwrap();
        assert_eq!(info.device_type, DeviceType::PrimarySink);
        assert!(info.session_available);
        assert!(!info.prefer_tdls);
    }

    #[test]
    fn device_info_roundtrips() {
        let info = DeviceInfo::source_available();
        let bytes = info.to_bytes();
        assert_eq!(DeviceInfo::parse(&bytes).unwrap(), info);
    }

    #[test]
    fn rejects_a_truncated_ie() {
        assert!(WfdIe::parse(&[0x00, 0x00, 0x06, 0x01]).is_err());
        assert!(WfdIe::parse(&[0x00]).is_err());
    }
}
