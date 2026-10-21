//! `aethercast` — the AetherCast client.
//!
//! The window shows what the daemon knows: the sinks it can see, the
//! sessions it is running, and — the part that matters — *why* nothing
//! was found. An empty list with no explanation is the thing this
//! application exists to replace.

mod daemon;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use aethercast_api::{Device, Diagnostics, Profile, Session};
use daemon::{Commander, Daemon, Event};
use gtk4 as gtk;
use libadwaita as adw;
// The libadwaita prelude brings the GTK one with it.
use libadwaita::prelude::*;

fn main() -> glib::ExitCode {
    adw::init().expect("libadwaita has to initialise");

    // The application ID cannot be the same string as the daemon's bus
    // name: the daemon owns `io.github.birukyitayew.aethercast` and serves
    // the API at `/io/github/birukyitayew/aethercast`, which is the very
    // path a GApplication derives from its own ID. Two things cannot own
    // the same name or export at the same path.
    let app = adw::Application::builder()
        .application_id("io.github.birukyitayew.aethercast.Client")
        .build();

    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("AetherCast")
            .default_width(760)
            .default_height(620)
            .build();
        let ui = Ui::new(&window);
        ui.start();
        window.present();
    });

    app.run()
}

/// What the window is showing.
#[derive(Default)]
struct State {
    devices: Vec<Device>,
    sessions: Vec<Session>,
    diagnostics: Option<Diagnostics>,
    /// Which session belongs to which device.
    session_of_device: HashMap<String, String>,
    /// The last error, if any.
    error: Option<String>,
}

/// The window.
struct Ui {
    daemon: Daemon,
    commander: Commander,
    banner: adw::Banner,
    device_stack: gtk::Stack,
    empty: adw::StatusPage,
    device_list: gtk::ListBox,
    session_group: adw::PreferencesGroup,
    session_list: gtk::ListBox,
    adapter_row: adw::ActionRow,
    radio_row: adw::ActionRow,
    station_row: adw::ActionRow,
    element_row: adw::ActionRow,
    state: RefCell<State>,
}

impl Ui {
    /// Build the window.
    fn new(window: &adw::ApplicationWindow) -> Rc<Self> {
        let daemon = Daemon::start();
        let commander = daemon.commander();

        // The banner is for things the user has to know about: the daemon
        // is not running, or a cast failed.
        let banner = adw::Banner::builder()
            .title("The daemon is not running")
            .button_label("Start it")
            .build();
        banner.set_revealed(false);

        // The devices, or the reason there are none.
        let device_list = gtk::ListBox::new();
        device_list.set_selection_mode(gtk::SelectionMode::None);
        device_list.add_css_class("boxed-list");

        // The empty state says *why* there is nothing to cast to, so the
        // description is filled in from the diagnostics.
        let empty = adw::StatusPage::builder()
            .icon_name("video-display-symbolic")
            .title("No sinks yet")
            .description("Looking for Miracast sinks.")
            .build();

        let device_stack = gtk::Stack::new();
        device_stack.add_named(&device_list, Some("list"));
        device_stack.add_named(&empty, Some("empty"));
        device_stack.set_visible_child_name("empty");

        let device_group = adw::PreferencesGroup::builder()
            .title("Devices")
            .description("The sinks this machine can see")
            .build();
        device_group.add(&device_stack);

        // The sessions.
        let session_list = gtk::ListBox::new();
        session_list.set_selection_mode(gtk::SelectionMode::None);
        session_list.add_css_class("boxed-list");
        let session_group = adw::PreferencesGroup::builder()
            .title("Sessions")
            .description("What is being cast, and to where")
            .build();
        session_group.add(&session_list);
        session_group.set_visible(false);

        // The diagnostics.
        let adapter_row = row("Adapter", "");
        let radio_row = row("Radio", "");
        let station_row = row("Station", "");
        let element_row = row("WFD element", "");
        let diagnostics_group = adw::PreferencesGroup::builder()
            .title("Diagnostics")
            .description("Why a cast can or cannot start")
            .build();
        diagnostics_group.add(&adapter_row);
        diagnostics_group.add(&radio_row);
        diagnostics_group.add(&station_row);
        diagnostics_group.add(&element_row);

        let page = adw::PreferencesPage::new();
        page.add(&device_group);
        page.add(&session_group);
        page.add(&diagnostics_group);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&page)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&banner);
        content.append(&scroller);

        let header = adw::HeaderBar::new();
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Look for sinks again"));
        header.pack_end(&refresh);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));

        // An AdwApplicationWindow has a content of its own, and does not
        // take a child the way a plain window does.
        window.set_content(Some(&toolbar));

        let ui = Rc::new(Self {
            daemon,
            commander,
            banner,
            device_stack,
            empty,
            device_list,
            session_group,
            session_list,
            adapter_row,
            radio_row,
            station_row,
            element_row,
            state: RefCell::new(State::default()),
        });

        // The refresh button asks the daemon for the state again.
        {
            let ui = ui.clone();
            refresh.connect_clicked(move |_| ui.commander.refresh());
        }

        // Starting the daemon is a normal thing to want to do, so the
        // banner offers it.
        {
            let ui = ui.clone();
            let banner = ui.banner.clone();
            banner.connect_button_clicked(move |_| {
                match std::process::Command::new("aethercastd")
                    .arg("daemon")
                    .spawn()
                {
                    Ok(_) => {
                        ui.state.borrow_mut().error = None;
                        ui.update_banner();
                    }
                    Err(e) => {
                        ui.state.borrow_mut().error =
                            Some(format!("the daemon could not be started: {e}"));
                        ui.update_banner();
                    }
                }
            });
        }

        ui
    }

    /// Start listening for the daemon.
    fn start(self: &Rc<Self>) {
        // The window picks the events up on a timer, so that nothing the
        // bus does can block the interface.
        let ui = self.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            while let Some(event) = ui.daemon.next_event() {
                ui.handle(event);
            }
            glib::ControlFlow::Continue
        });
    }

    /// Deal with something the daemon told us.
    fn handle(&self, event: Event) {
        {
            let mut state = self.state.borrow_mut();
            match event {
                Event::Connected => state.error = None,
                Event::Devices(devices) => state.devices = devices,
                Event::Sessions(sessions) => {
                    state.session_of_device = sessions
                        .iter()
                        .map(|session| (session.device.clone(), session.id.clone()))
                        .collect();
                    state.sessions = sessions;
                }
                Event::Diagnostics(diagnostics) => state.diagnostics = Some(diagnostics),
                Event::DeviceFound(device) => {
                    if !state.devices.iter().any(|known| known.id == device.id) {
                        state.devices.push(device);
                    }
                }
                Event::SessionStateChanged {
                    session,
                    state: new_state,
                } => {
                    match state.sessions.iter_mut().find(|known| known.id == session) {
                        Some(known) => known.state = new_state.clone(),
                        None => state.sessions.push(Session {
                            id: session.clone(),
                            device: String::new(),
                            state: new_state.clone(),
                            detail: None,
                        }),
                    }
                    if new_state == "failed" {
                        state.error = Some("the cast failed".to_string());
                    }
                }
                Event::Error(message) => state.error = Some(message),
            }
        }
        self.rebuild();
    }

    /// Draw everything again.
    fn rebuild(&self) {
        let state = self.state.borrow();

        // The devices.
        while let Some(child) = self.device_list.first_child() {
            self.device_list.remove(&child);
        }
        for device in &state.devices {
            self.device_list.append(&self.device_row(device, &state));
        }
        let has_devices = !state.devices.is_empty();
        self.device_stack
            .set_visible_child_name(if has_devices { "list" } else { "empty" });

        // The sessions.
        while let Some(child) = self.session_list.first_child() {
            self.session_list.remove(&child);
        }
        for session in &state.sessions {
            self.session_list.append(&self.session_row(session));
        }
        self.session_group.set_visible(!state.sessions.is_empty());

        // The diagnostics.
        if let Some(diagnostics) = &state.diagnostics {
            let bands = diagnostics
                .bands()
                .unwrap_or_else(|| "unknown band".to_string());
            let limitation = diagnostics.limitation();
            let adapter = match &limitation {
                Some(limitation) => format!("{bands} — {limitation}"),
                None => bands,
            };
            self.adapter_row.set_subtitle(&adapter);

            let radio = match &diagnostics.wedge {
                Some(wedge) => format!("wedged — {wedge}"),
                None if diagnostics.radio_healthy => format!(
                    "healthy, {} work item(s) queued",
                    diagnostics.radio_work_items
                ),
                None => "not healthy".to_string(),
            };
            self.radio_row.set_subtitle(&radio);

            let station = &diagnostics.station;
            let station_text = match (&station.ssid, station.connected) {
                (Some(ssid), true) => match station.freq {
                    Some(freq) => format!("{ssid} on {freq} MHz"),
                    None => ssid.clone(),
                },
                (Some(ssid), false) => format!("{ssid} — not associated"),
                (None, _) => "not associated".to_string(),
            };
            self.station_row.set_subtitle(&station_text);

            let element = diagnostics
                .wfd_element
                .clone()
                .unwrap_or_else(|| "not armed".to_string());
            self.element_row.set_subtitle(&element);

            // The empty state says *why* there is nothing to cast to.
            if !has_devices {
                let reason = match &limitation {
                    Some(limitation) => format!(
                        "No sink is advertising on a band this adapter can hear.\n{}",
                        sentence(limitation)
                    ),
                    None => match &diagnostics.wedge {
                        Some(wedge) => format!(
                            "The radio is not answering, so nothing can be found.\n{}",
                            sentence(wedge)
                        ),
                        None => "Looking for Miracast sinks. Put the TV in screen sharing mode."
                            .to_string(),
                    },
                };
                self.empty.set_description(Some(&reason));
            }
        }

        self.update_banner();
    }

    /// A row for one sink.
    fn device_row(&self, device: &Device, state: &State) -> adw::ActionRow {
        let mut subtitle = device.id.clone();
        if let Some(wfd) = &device.wfd {
            let role = wfd
                .get("device_type")
                .and_then(|role| role.as_str())
                .unwrap_or("sink");
            let port = wfd.get("control_port").and_then(|port| port.as_u64());
            let rate = wfd
                .get("max_throughput_mbps")
                .and_then(|rate| rate.as_u64())
                .unwrap_or(0);
            subtitle = match port {
                Some(port) => format!("{subtitle} — {role}, port {port}, {rate} Mbit/s"),
                None => format!("{subtitle} — {role}"),
            };
        }
        let row = adw::ActionRow::builder()
            .title(device.display_name())
            .subtitle(subtitle)
            .build();

        match state.session_of_device.get(&device.id) {
            Some(session) => {
                let stop = gtk::Button::with_label("Stop");
                stop.add_css_class("suggested-action");
                let commander = self.commander.clone();
                let session = session.clone();
                stop.connect_clicked(move |_| commander.stop(&session));
                row.add_suffix(&stop);
            }
            None => {
                let cast = gtk::Button::with_label("Cast");
                cast.add_css_class("suggested-action");
                cast.set_sensitive(device.is_available());
                cast.set_tooltip_text(Some(if device.is_available() {
                    "Cast the screen to this sink"
                } else {
                    "This sink is not taking sessions right now"
                }));
                let commander = self.commander.clone();
                let device_id = device.id.clone();
                cast.connect_clicked(move |_| commander.cast(&device_id, Profile::default()));
                row.add_suffix(&cast);
            }
        }
        row
    }

    /// A row for one session.
    fn session_row(&self, session: &Session) -> adw::ActionRow {
        let title = if session.device.is_empty() {
            session.id.clone()
        } else {
            format!("{} → {}", session.id, session.device)
        };
        let subtitle = match &session.detail {
            Some(detail) if !detail.is_empty() => format!("{} — {detail}", session.state),
            _ => session.state.clone(),
        };
        adw::ActionRow::builder()
            .title(title)
            .subtitle(subtitle)
            .build()
    }

    /// Show or hide the banner.
    fn update_banner(&self) {
        let state = self.state.borrow();
        match &state.error {
            Some(error) => {
                self.banner.set_title(error);
                self.banner.set_revealed(true);
            }
            None => {
                self.banner.set_revealed(false);
            }
        }
    }
}

/// An empty row with a title and a subtitle.
fn row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build()
}

/// A sentence, with a capital letter.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The empty state has to be able to say *why* there is nothing to
    /// cast to, so the description has to change.
    #[test]
    fn the_status_page_description_changes() {
        gtk4::init().expect("gtk has to initialise");
        let page = libadwaita::StatusPage::new();
        page.set_description(Some("the first reason"));
        assert_eq!(page.description().as_deref(), Some("the first reason"));
        page.set_description(Some("the second reason"));
        assert_eq!(page.description().as_deref(), Some("the second reason"));
    }

    /// A sentence the daemon wrote starts in the middle of a thought.
    #[test]
    fn a_sentence_starts_with_a_capital() {
        assert_eq!(
            sentence("this adapter is 2.4 GHz only"),
            "This adapter is 2.4 GHz only"
        );
        assert_eq!(sentence(""), "");
    }
}
