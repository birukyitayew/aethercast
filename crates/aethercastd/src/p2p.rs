//! Wi-Fi Direct group formation.
//!
//! In Miracast the **source** is normally the Group Owner: it finds the
//! sink, provisions it, and forms a group that the sink joins. The group's
//! interface is the one the RTSP server has to listen on, because that is
//! the address the sink reaches us at.
//!
//! The control interface exposes:
//!
//! | Command | Meaning |
//! | --- | --- |
//! | `P2P_GROUP_ADD [freq=N]` | become the Group Owner on our own |
//! | `P2P_GROUP_REMOVE *` | tear the group down |
//! | `P2P_GROUP_MEMBER <addr>` | the details of one member |
//! | `P2P_PROV_DISC <addr> pbc` | check the peer's provisioning method |
//! | `P2P_CONNECT <addr> pbc` | invite the peer into a group |
//!
//! and the events `P2P-GROUP-STARTED`, `P2P-PEER-CONNECTED`,
//! `P2P-GO-NEG-SUCCESS` and `P2P-GROUP-FORMATION-FAILURE`.
//!
//! The names are the strings `wpa_supplicant` puts on the control
//! interface. They are **not** the C macro names from `wpa_ctrl.h`
//! (`P2P_EVENT_GROUP_STARTED` and friends): those macros expand to these
//! strings, and matching the macro name sees nothing at all.
//!
//! `P2P-GROUP-STARTED` is shaped like:
//!
//! ```text
//! <3>P2P-GROUP-STARTED p2p-wlan0-0 GO ssid="Direct-xy" freq=2412
//!     psk=... go_dev_addr=aa:bb:cc:dd:ee:ff
//! ```

/// A parsed `P2P_EVENT_GROUP_STARTED`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupStarted {
    /// The group interface, for example `p2p-wlan0-0`.
    pub interface: String,
    /// Whether we are the Group Owner.
    pub go: bool,
    /// The group's SSID.
    pub ssid: Option<String>,
    /// The operating frequency in MHz.
    pub freq: Option<u32>,
    /// The Group Owner's device address.
    pub go_dev_addr: Option<String>,
}

impl GroupStarted {
    /// Parse a `P2P-GROUP-STARTED` line.
    pub fn parse(line: &str) -> Option<Self> {
        let rest = line.split("P2P-GROUP-STARTED").nth(1)?.trim();
        let mut parts = rest.split_whitespace();
        let interface = parts.next()?.to_string();
        let role = parts.next().unwrap_or("");
        let mut group = GroupStarted {
            interface,
            go: role.eq_ignore_ascii_case("GO"),
            ssid: None,
            freq: None,
            go_dev_addr: None,
        };
        for token in rest.split_whitespace() {
            if let Some(value) = token.strip_prefix("ssid=") {
                group.ssid = Some(value.trim_matches('"').to_string());
            } else if let Some(value) = token.strip_prefix("freq=") {
                group.freq = value.parse().ok();
            } else if let Some(value) = token.strip_prefix("go_dev_addr=") {
                group.go_dev_addr = Some(value.to_string());
            }
        }
        Some(group)
    }
}

/// The kinds of Wi-Fi Direct event the daemon reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A peer was found during discovery.
    DeviceFound(String),
    /// A group was formed.
    GroupStarted(GroupStarted),
    /// A peer joined our group.
    PeerConnected(String),
    /// The group formation failed.
    GroupFormationFailure(String),
    /// The group went away.
    GroupRemoved(String),
    /// A GO negotiation succeeded.
    GoNegotiationSuccess(String),
    /// A GO negotiation failed.
    GoNegotiationFailure(String),
    /// The device search stopped.
    FindStopped,
    /// Anything else.
    Other(String),
}

impl Event {
    /// Classify a raw control-interface event line.
    pub fn parse(line: &str) -> Option<Self> {
        let text = line.trim();
        if !text.starts_with('<') {
            return None;
        }
        let body = text.split_once('>')?.1.trim();
        if body.starts_with("P2P-GROUP-STARTED") {
            return GroupStarted::parse(body).map(Event::GroupStarted);
        }
        if body.starts_with("P2P-DEVICE-FOUND") {
            return Some(Event::DeviceFound(body.to_string()));
        }
        if body.starts_with("P2P-PEER-CONNECTED") {
            return Some(Event::PeerConnected(body.to_string()));
        }
        if body.starts_with("P2P-GROUP-FORMATION-FAILURE") {
            return Some(Event::GroupFormationFailure(body.to_string()));
        }
        if body.starts_with("P2P-GROUP-REMOVED") {
            return Some(Event::GroupRemoved(body.to_string()));
        }
        if body.starts_with("P2P-GO-NEG-SUCCESS") {
            return Some(Event::GoNegotiationSuccess(body.to_string()));
        }
        if body.starts_with("P2P-GO-NEG-FAILURE") {
            return Some(Event::GoNegotiationFailure(body.to_string()));
        }
        if body.starts_with("P2P-FIND-STOPPED") {
            return Some(Event::FindStopped);
        }
        Some(Event::Other(body.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_group_started_event() {
        // The real line `wpa_supplicant` sends, captured from its journal.
        let line = "<3>P2P-GROUP-STARTED p2p-wlan0-0 GO ssid=\"Direct-xy\" freq=2412 psk=abc go_dev_addr=aa:bb:cc:dd:ee:ff";
        let event = Event::parse(line).unwrap();
        let group = match event {
            Event::GroupStarted(group) => group,
            other => panic!("unexpected event: {other:?}"),
        };
        assert_eq!(group.interface, "p2p-wlan0-0");
        assert!(group.go);
        assert_eq!(group.ssid.as_deref(), Some("Direct-xy"));
        assert_eq!(group.freq, Some(2412));
        assert_eq!(group.go_dev_addr.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn parses_a_client_group_started_event() {
        let line = "<3>P2P-GROUP-STARTED p2p-wlan0-0 client ssid=\"Direct-xy\" freq=2412 go_dev_addr=aa:bb:cc:dd:ee:ff";
        let group = GroupStarted::parse(line.split_once('>').unwrap().1).unwrap();
        assert!(!group.go);
    }

    #[test]
    fn the_c_macro_names_are_not_wire_events() {
        // `wpa_ctrl.h` defines these macros, but the strings on the socket
        // use dashes. Matching the macro name is a silent no-op.
        assert!(matches!(
            Event::parse("<1>P2P_EVENT_DEVICE_FOUND aa:bb:cc:dd:ee:ff"),
            Some(Event::Other(_))
        ));
    }

    #[test]
    fn classifies_the_events_the_daemon_cares_about() {
        assert!(matches!(
            Event::parse("<1>P2P-FIND-STOPPED"),
            Some(Event::FindStopped)
        ));
        assert!(matches!(
            Event::parse("<1>P2P-PEER-CONNECTED aa:bb:cc:dd:ee:ff"),
            Some(Event::PeerConnected(_))
        ));
        assert!(matches!(
            Event::parse("<1>P2P-GROUP-FORMATION-FAILURE reason=1"),
            Some(Event::GroupFormationFailure(_))
        ));
        assert!(Event::parse("not an event").is_none());
    }
}
