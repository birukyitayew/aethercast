//! Talking to the daemon.
//!
//! The window never blocks on the bus: a worker thread holds the
//! connection and sends what it learns back over a channel, and the window
//! picks it up on a timer. The same arrangement carries the commands the
//! user gives, so a call that the daemon does not answer cannot freeze
//! the interface.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use aethercast_api::{Device, Diagnostics, ManagerApiProxy, Profile, Session};
use futures_util::StreamExt;

/// Something the daemon told us.
#[derive(Debug)]
pub enum Event {
    /// The daemon answered.
    Connected,
    /// The devices we know about.
    Devices(Vec<Device>),
    /// The sessions we know about.
    Sessions(Vec<Session>),
    /// A snapshot of the environment.
    Diagnostics(Diagnostics),
    /// A sink was discovered.
    DeviceFound(Device),
    /// A session changed state.
    SessionStateChanged { session: String, state: String },
    /// The daemon is not answering.
    Error(String),
}

/// Something the user asked for.
#[derive(Debug)]
pub enum Command {
    /// Cast to a device.
    Cast { device: String, profile: Profile },
    /// Stop a session.
    Stop { session: String },
    /// Ask for the state again.
    Refresh,
}

/// The connection to the daemon.
pub struct Daemon {
    events: Receiver<Event>,
    commands: Sender<Command>,
}

/// A handle that a widget can hold.
///
/// The widgets live longer than any one call, and the bus connection
/// belongs to the worker, so they are given a way to ask for something
/// rather than the connection itself.
#[derive(Clone)]
pub struct Commander(Sender<Command>);

impl Commander {
    /// Ask the daemon to cast to a device.
    pub fn cast(&self, device: &str, profile: Profile) {
        let _ = self.0.send(Command::Cast {
            device: device.to_string(),
            profile,
        });
    }

    /// Ask the daemon to stop a session.
    pub fn stop(&self, session: &str) {
        let _ = self.0.send(Command::Stop {
            session: session.to_string(),
        });
    }

    /// Ask the daemon for the state again.
    pub fn refresh(&self) {
        let _ = self.0.send(Command::Refresh);
    }
}

impl Daemon {
    /// Start the worker.
    pub fn start() -> Self {
        let (event_tx, event_rx) = channel();
        let (command_tx, command_rx) = channel();
        std::thread::Builder::new()
            .name("aethercast-bus".into())
            .spawn(move || worker(event_tx, command_rx))
            .expect("the bus worker has to start");
        Self {
            events: event_rx,
            commands: command_tx,
        }
    }

    /// The next event, if one has arrived.
    pub fn next_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    /// A handle that widgets can hold.
    pub fn commander(&self) -> Commander {
        Commander(self.commands.clone())
    }
}

/// The worker: one connection, one loop.
fn worker(events: Sender<Event>, commands: Receiver<Command>) {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = events.send(Event::Error(format!(
                "the network runtime did not start: {e}"
            )));
            return;
        }
    };

    runtime.block_on(async move {
        loop {
            match connect().await {
                Ok(link) => {
                    let Link {
                        proxy,
                        mut devices_found,
                        mut sessions_changed,
                    } = link;
                    let _ = events.send(Event::Connected);
                    refresh(&proxy, &events).await;

                    let mut ticker = tokio::time::interval(Duration::from_secs(2));
                    loop {
                        tokio::select! {
                            Some(signal) = devices_found.next() => {
                                match signal.args() {
                                    Ok(args) => {
                                        if let Ok(device) =
                                            serde_json::from_str::<Device>(&args.device)
                                        {
                                            let _ = events.send(Event::DeviceFound(device));
                                        }
                                    }
                                    Err(e) => {
                                        let _ = events.send(Event::Error(format!(
                                            "a device signal could not be read: {e}"
                                        )));
                                    }
                                }
                            }
                            Some(signal) = sessions_changed.next() => {
                                match signal.args() {
                                    Ok(args) => {
                                        let _ = events.send(Event::SessionStateChanged {
                                            session: args.session,
                                            state: args.state,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = events.send(Event::Error(format!(
                                            "a session signal could not be read: {e}"
                                        )));
                                    }
                                }
                            }
                            _ = ticker.tick() => {
                                refresh(&proxy, &events).await;
                            }
                            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                                while let Ok(command) = commands.try_recv() {
                                    handle(command, &proxy, &events).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = events.send(Event::Error(format!("the daemon is not answering: {e}")));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });
}

/// A live connection to the daemon.
struct Link {
    proxy: ManagerApiProxy<'static>,
    devices_found: aethercast_api::DeviceFoundStream<'static>,
    sessions_changed: aethercast_api::SessionStateChangedStream<'static>,
}

/// Connect to the daemon and subscribe to its signals.
async fn connect() -> Result<Link, zbus::Error> {
    let connection = zbus::Connection::session().await?;
    let proxy = ManagerApiProxy::new(&connection).await?;
    let devices_found = proxy.receive_device_found().await?;
    let sessions_changed = proxy.receive_session_state_changed().await?;
    Ok(Link {
        proxy,
        devices_found,
        sessions_changed,
    })
}

/// Read the state again.
async fn refresh(proxy: &ManagerApiProxy<'_>, events: &Sender<Event>) {
    if let Ok(json) = proxy.list_devices().await {
        match serde_json::from_str::<Vec<Device>>(&json) {
            Ok(devices) => {
                let _ = events.send(Event::Devices(devices));
            }
            Err(e) => {
                let _ = events.send(Event::Error(format!(
                    "the device list could not be read: {e}"
                )));
            }
        }
    }
    if let Ok(json) = proxy.list_sessions().await {
        if let Ok(sessions) = serde_json::from_str::<Vec<Session>>(&json) {
            let _ = events.send(Event::Sessions(sessions));
        }
    }
    if let Ok(json) = proxy.get_diagnostics().await {
        match serde_json::from_str::<Diagnostics>(&json) {
            Ok(diagnostics) => {
                let _ = events.send(Event::Diagnostics(diagnostics));
            }
            Err(e) => {
                let _ = events.send(Event::Error(format!(
                    "the diagnostics could not be read: {e}"
                )));
            }
        }
    }
}

/// Carry out a command from the user.
async fn handle(command: Command, proxy: &ManagerApiProxy<'_>, events: &Sender<Event>) {
    match command {
        Command::Refresh => refresh(proxy, events).await,
        Command::Cast { device, profile } => {
            let profile = match serde_json::to_string(&profile) {
                Ok(profile) => profile,
                Err(e) => {
                    let _ =
                        events.send(Event::Error(format!("the profile could not be sent: {e}")));
                    return;
                }
            };
            match proxy.start_cast(device, profile).await {
                Ok(session) => {
                    let _ = events.send(Event::SessionStateChanged {
                        session,
                        state: "connecting".to_string(),
                    });
                }
                Err(e) => {
                    let _ = events.send(Event::Error(format!("the cast did not start: {e}")));
                }
            }
        }
        Command::Stop { session } => {
            if let Err(e) = proxy.stop_cast(session).await {
                let _ = events.send(Event::Error(format!("the session did not stop: {e}")));
            }
        }
    }
}
