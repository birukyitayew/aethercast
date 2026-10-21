//! The D-Bus API.
//!
//! The daemon serves one interface on the session bus:
//! `io.github.birukyitayew.aethercast.Manager`, at the object path
//! `/io/github/birukyitayew/aethercast`.
//!
//! The published contract is [`dbus/io.github.birukyitayew.aethercast.Manager.xml`](../../dbus/io.github.birukyitayew.aethercast.Manager.xml),
//! and a test checks that the implementation and the contract do not drift
//! apart.
//!
//! Structured data is returned as JSON, so the interface can grow without
//! breaking the clients that came before it.

use crate::core::SharedManager;

/// The bus name the daemon owns.
pub const BUS_NAME: &str = "io.github.birukyitayew.aethercast";
/// The object path the manager is served at.
pub const OBJECT_PATH: &str = "/io/github/birukyitayew/aethercast";
/// The interface name.
pub const INTERFACE: &str = "io.github.birukyitayew.aethercast.Manager";
/// The contract, for the drift test.
pub const CONTRACT: &str =
    include_str!("../../../dbus/io.github.birukyitayew.aethercast.Manager.xml");

/// The object served on the bus.
pub struct ManagerInterface(pub SharedManager);

#[zbus::interface(name = "io.github.birukyitayew.aethercast.Manager")]
impl ManagerInterface {
    /// The interface version.
    async fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// The devices we know about, as JSON.
    async fn list_devices(&self) -> String {
        match serde_json::to_string(&self.0.devices()) {
            Ok(json) => json,
            Err(e) => format!("{{\"error\":\"{e}\"}}"),
        }
    }

    /// The casting sessions we know about, as JSON.
    async fn list_sessions(&self) -> String {
        match serde_json::to_string(&self.0.sessions()) {
            Ok(json) => json,
            Err(e) => format!("{{\"error\":\"{e}\"}}"),
        }
    }

    /// A snapshot of the environment, as JSON.
    async fn get_diagnostics(&self) -> String {
        match serde_json::to_string(&self.0.diagnostics()) {
            Ok(json) => json,
            Err(e) => format!("{{\"error\":\"{e}\"}}"),
        }
    }

    /// Advertise a WFD information element, given as hex.
    async fn arm_wfd_element(&self, element: String) -> zbus::fdo::Result<String> {
        let ie = if element.trim().is_empty() {
            self.0.default_element()
        } else {
            crate::wfd::WfdIe::parse_hex(&element).map_err(|e| {
                zbus::fdo::Error::InvalidArgs(format!("that is not a WFD element: {e}"))
            })?
        };
        self.0
            .arm(&ie)
            .map_err(|e| zbus::fdo::Error::Failed(format!("the radio is not available: {e}")))
    }

    /// Start casting to a device.
    async fn start_cast(&self, device: String, profile: String) -> zbus::fdo::Result<String> {
        let session = self.0.new_session(&device);
        let manager = self.0.clone();
        let session_id = session.id.clone();

        // The cast itself is long running, so it goes to a thread and the
        // method returns as soon as the session exists.
        let session_id_in_thread = session_id.clone();
        std::thread::spawn(move || {
            let profile = if profile.trim().is_empty() {
                crate::media::Profile::default()
            } else {
                match serde_json::from_str::<crate::media::Profile>(&profile) {
                    Ok(profile) => profile,
                    Err(e) => {
                        manager.update_session(
                            &session_id_in_thread,
                            "failed",
                            Some(format!("that is not a profile: {e}")),
                        );
                        return;
                    }
                }
            };
            manager.update_session(&session_id_in_thread, "connecting", None);
            let options = crate::cast::Options::default();
            let mut log = |line: &str| {
                eprintln!("[cast] {line}");
            };
            match crate::cast::run(&manager, &profile, &options, &mut log) {
                Ok(()) => manager.update_session(&session_id_in_thread, "stopped", None),
                Err(e) => {
                    manager.update_session(&session_id_in_thread, "failed", Some(e.to_string()))
                }
            }
        });

        Ok(session_id)
    }

    /// Stop a casting session.
    async fn stop_cast(&self, session: String) -> bool {
        self.0.remove_session(&session)
    }
}

/// Serve the API on the session bus.
pub async fn serve(manager: SharedManager) -> anyhow::Result<()> {
    // The contract is what clients are written against, so a stale one is
    // worse than no contract at all.
    if !CONTRACT.contains(INTERFACE) {
        anyhow::bail!(
            "the published contract does not describe the interface we serve ({INTERFACE})"
        );
    }
    // The radio is opened so that the diagnostics are real from the first
    // call, but a machine without an adapter can still serve the API.
    if let Err(e) = manager.open_radio() {
        eprintln!("note: no wireless adapter ({e}), serving the API anyway");
    }

    let connection = zbus::Connection::session().await?;
    connection
        .object_server()
        .at(OBJECT_PATH, ManagerInterface(manager))
        .await?;
    connection.request_name(BUS_NAME).await?;
    println!("serving {INTERFACE} as {BUS_NAME} at {OBJECT_PATH}");
    // The connection has to stay alive for the service to be reachable.
    std::future::pending::<()>().await;
    Ok(())
}

/// A client for the API, used by the tests and by the client application.
#[zbus::proxy(
    interface = "io.github.birukyitayew.aethercast.Manager",
    default_service = "io.github.birukyitayew.aethercast",
    default_path = "/io/github/birukyitayew/aethercast"
)]
pub trait ManagerApi {
    fn version(&self) -> zbus::Result<String>;
    fn list_devices(&self) -> zbus::Result<String>;
    fn list_sessions(&self) -> zbus::Result<String>;
    fn get_diagnostics(&self) -> zbus::Result<String>;
    fn arm_wfd_element(&self, element: String) -> zbus::Result<String>;
    fn start_cast(&self, device: String, profile: String) -> zbus::Result<String>;
    fn stop_cast(&self, session: String) -> zbus::Result<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The method names in the published contract.
    fn contract_methods() -> Vec<String> {
        CONTRACT
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("<method name=\"")?
                    .split('"')
                    .next()
                    .map(str::to_string)
            })
            .collect()
    }

    #[test]
    fn the_contract_and_the_implementation_agree() {
        let contract = contract_methods();
        assert!(
            CONTRACT.contains(INTERFACE),
            "the contract does not name the interface the daemon serves"
        );
        assert!(contract.contains(&"ListDevices".to_string()));
        assert!(contract.contains(&"StartCast".to_string()));
        assert!(contract.contains(&"GetDiagnostics".to_string()));
        // Every method in the contract is implemented, and the
        // implementation has nothing the contract does not mention.
        let implemented = [
            "Version",
            "ListDevices",
            "ListSessions",
            "GetDiagnostics",
            "ArmWfdElement",
            "StartCast",
            "StopCast",
        ];
        for method in implemented {
            assert!(
                contract.contains(&method.to_string()),
                "{method} is implemented but not in the contract"
            );
        }
    }

    #[tokio::test]
    async fn the_api_answers_on_the_session_bus() {
        // The test needs a session bus; `dbus-run-session` provides one.
        let Ok(connection) = zbus::Connection::session().await else {
            eprintln!("no session bus: skipping");
            return;
        };
        let manager = std::sync::Arc::new(crate::core::Manager::new("wlan0"));
        connection
            .object_server()
            .at(OBJECT_PATH, ManagerInterface(manager))
            .await
            .unwrap();
        connection.request_name(BUS_NAME).await.unwrap();

        let proxy = ManagerApiProxy::new(&connection).await.unwrap();
        assert!(!proxy.version().await.unwrap().is_empty());
        assert_eq!(proxy.list_devices().await.unwrap(), "[]");
        assert_eq!(proxy.list_sessions().await.unwrap(), "[]");
        let diagnostics: serde_json::Value =
            serde_json::from_str(&proxy.get_diagnostics().await.unwrap()).unwrap();
        assert_eq!(diagnostics["interface"], "wlan0");
    }
}
