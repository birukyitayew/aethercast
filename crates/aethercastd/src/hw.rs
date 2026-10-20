//! What the wireless adapter can actually do.
//!
//! A Wi-Fi Direct sink chooses its listen channel, and a sink that
//! listens on 5 GHz is invisible to a 2.4 GHz-only adapter — no amount of
//! correct protocol handling will reach it. The reference machine's
//! adapter (QCA9565/AR9565) is Band 1 only, so AetherCast has to be
//! able to say so plainly instead of showing an empty list.
//!
//! The band list is read from the kernel through `iw`, which reports the
//! bands the phy supports. A future version should query nl80211
//! directly so the daemon has no external dependency, but the interface
//! here is deliberately narrow so that swap is a one-file change.

use std::process::Command;

/// The bands a phy can use, in GHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    /// 2.4 GHz.
    TwoPointFour,
    /// 5 GHz.
    Five,
    /// 6 GHz.
    Six,
}

impl Band {
    /// The frequency range in MHz.
    pub fn range_mhz(self) -> (u32, u32) {
        match self {
            Band::TwoPointFour => (2400, 2500),
            Band::Five => (4900, 5900),
            Band::Six => (5925, 7125),
        }
    }

    /// The human readable name.
    pub fn name(self) -> &'static str {
        match self {
            Band::TwoPointFour => "2.4 GHz",
            Band::Five => "5 GHz",
            Band::Six => "6 GHz",
        }
    }
}

/// What the adapter supports.
#[derive(Debug, Clone, Default)]
pub struct Adapter {
    /// The phy name, for example `phy0`.
    pub phy: String,
    /// The bands the phy supports.
    pub bands: Vec<Band>,
}

impl Adapter {
    /// Detect the adapter's capabilities.
    pub fn detect() -> Self {
        let mut adapter = Adapter::default();
        // `iw phy info` is not a valid invocation; `iw phy` prints the
        // full information for every phy on the machine.
        let Ok(output) = Command::new("iw").arg("phy").output() else {
            return adapter;
        };
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Wiphy ") {
                adapter.phy = trimmed.trim_start_matches("Wiphy ").trim().to_string();
            }
            if let Some(rest) = trimmed.strip_prefix("Band ") {
                let number: u32 = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                let band = match number {
                    1 => Band::TwoPointFour,
                    2 => Band::Five,
                    3 => Band::Six,
                    _ => continue,
                };
                if !adapter.bands.contains(&band) {
                    adapter.bands.push(band);
                }
            }
        }
        adapter.bands.sort();
        adapter
    }

    /// True when the adapter can use 5 GHz.
    pub fn supports_five_ghz(&self) -> bool {
        self.bands.contains(&Band::Five)
    }

    /// True when the adapter can use 6 GHz.
    pub fn supports_six_ghz(&self) -> bool {
        self.bands.contains(&Band::Six)
    }

    /// The bands as a readable list.
    pub fn band_names(&self) -> String {
        if self.bands.is_empty() {
            return "unknown".to_string();
        }
        self.bands
            .iter()
            .map(|b| b.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The warning to show when nothing was found.
    ///
    /// A 2.4 GHz-only adapter cannot hear a sink that advertises on
    /// 5 GHz, and that is the single most common reason a perfectly good
    /// Miracast sink never appears.
    pub fn limitation(&self) -> Option<String> {
        if self.bands.is_empty() {
            return Some(
                "the adapter's bands could not be read, so band-limited sinks cannot be ruled out"
                    .to_string(),
            );
        }
        if self.supports_five_ghz() {
            return None;
        }
        Some(format!(
            "this adapter is 2.4 GHz only ({}), so a sink that advertises on 5 GHz cannot be seen",
            self.band_names()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_point_four_only_adapter_reports_the_limitation() {
        let adapter = Adapter {
            phy: "phy0".into(),
            bands: vec![Band::TwoPointFour],
        };
        assert!(!adapter.supports_five_ghz());
        let limitation = adapter.limitation().unwrap();
        assert!(limitation.contains("2.4 GHz only"), "{limitation}");
        assert!(limitation.contains("5 GHz"), "{limitation}");
    }

    #[test]
    fn a_dual_band_adapter_has_no_limitation() {
        let adapter = Adapter {
            phy: "phy0".into(),
            bands: vec![Band::TwoPointFour, Band::Five],
        };
        assert!(adapter.supports_five_ghz());
        assert!(adapter.limitation().is_none());
    }

    #[test]
    fn an_unknown_adapter_is_still_reported_as_unknown() {
        let adapter = Adapter::default();
        assert!(adapter.limitation().is_some());
        assert_eq!(adapter.band_names(), "unknown");
    }
}
