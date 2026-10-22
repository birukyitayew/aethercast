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

use aethercast_api::{BUS_NAME, CONTRACT, INTERFACE, OBJECT_PATH};

use crate::core::SharedManager;

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
    manager.set_bus(connection.clone());
    connection
        .object_server()
        .at(OBJECT_PATH, ManagerInterface(manager.clone()))
        .await?;
    connection.request_name(BUS_NAME).await?;

    // The daemon watches for sinks in the background, so that a client is
    // told when one appears rather than having to poll.
    let watcher = manager.clone();
    std::thread::spawn(move || loop {
        watcher.search(10);
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    println!("serving {INTERFACE} as {BUS_NAME} at {OBJECT_PATH}");
    // The connection has to stay alive for the service to be reachable.
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aethercast_api::ManagerApiProxy;

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
        // Every method the contract publishes is implemented here, and
        // nothing is implemented that the contract does not mention.
        for method in aethercast_api::CONTRACT_METHODS {
            assert!(
                contract.contains(&method.to_string()),
                "{method} is implemented but not in the contract"
            );
        }
        for signal in aethercast_api::CONTRACT_SIGNALS {
            assert!(
                CONTRACT.contains(signal),
                "{signal} is implemented but not in the contract"
            );
        }
    }

    /// A bus name that no other test uses.
    ///
    /// The tests run at the same time, and a bus name belongs to one
    /// connection at a time.
    fn unique_name(tag: &str) -> String {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // A D-Bus name element may not start with a digit.
        format!("io.github.birukyitayew.aethercast.test.{tag}.n{n}")
    }

    #[tokio::test]
    async fn a_client_is_told_when_a_device_appears() {
        let Ok(connection) = zbus::Connection::session().await else {
            eprintln!("no session bus: skipping");
            return;
        };
        let manager = std::sync::Arc::new(crate::core::Manager::new("wlan0"));
        manager.set_bus(connection.clone());
        connection
            .object_server()
            .at(OBJECT_PATH, ManagerInterface(manager.clone()))
            .await
            .unwrap();
        let name = unique_name("signals");
        connection.request_name(name.as_str()).await.unwrap();

        let proxy = ManagerApiProxy::builder(&connection)
            .destination(name.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut signals = proxy.receive_device_found().await.unwrap();

        // Announcing a device must reach the client.
        let device = crate::core::Device {
            id: "aa:bb:cc:dd:ee:ff".into(),
            name: Some("Test Sink".into()),
            kind: "miracast-sink".into(),
            address: None,
            wfd: None,
        };
        manager.announce_device(&device);

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            use futures_util::StreamExt;
            signals.next().await
        })
        .await
        .expect("the signal did not arrive")
        .expect("the signal stream ended");
        let args = received.args().unwrap();
        assert!(args.device.contains("Test Sink"), "{}", args.device);
    }

    #[tokio::test]
    async fn a_client_is_told_when_a_session_changes() {
        let Ok(connection) = zbus::Connection::session().await else {
            eprintln!("no session bus: skipping");
            return;
        };
        let manager = std::sync::Arc::new(crate::core::Manager::new("wlan0"));
        manager.set_bus(connection.clone());
        connection
            .object_server()
            .at(OBJECT_PATH, ManagerInterface(manager.clone()))
            .await
            .unwrap();
        let name = unique_name("sessions");
        connection.request_name(name.as_str()).await.unwrap();

        let proxy = ManagerApiProxy::builder(&connection)
            .destination(name.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut signals = proxy.receive_session_state_changed().await.unwrap();

        let session = manager.new_session("aa:bb:cc:dd:ee:ff");
        manager.update_session(&session.id, "playing", None);

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            use futures_util::StreamExt;
            signals.next().await
        })
        .await
        .expect("the signal did not arrive")
        .expect("the signal stream ended");
        let args = received.args().unwrap();
        assert_eq!(args.session, session.id);
        assert_eq!(args.state, "playing");
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
        let name = unique_name("api");
        connection.request_name(name.as_str()).await.unwrap();

        let proxy = ManagerApiProxy::builder(&connection)
            .destination(name.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        assert!(!proxy.version().await.unwrap().is_empty());
        assert_eq!(proxy.list_devices().await.unwrap(), "[]");
        assert_eq!(proxy.list_sessions().await.unwrap(), "[]");
        let diagnostics: serde_json::Value =
            serde_json::from_str(&proxy.get_diagnostics().await.unwrap()).unwrap();
        assert_eq!(diagnostics["interface"], "wlan0");
    }
}
