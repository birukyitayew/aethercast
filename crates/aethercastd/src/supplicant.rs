//! Client for the `wpa_supplicant` control interface (`ctrl_iface`).
//!
//! The protocol is newline-delimited text over a **datagram** Unix domain
//! socket (`ctrl_iface_unix.c` uses `SOCK_DGRAM`, not `SOCK_STREAM`):
//!
//! * a request is a single datagram,
//! * the reply is one datagram containing `OK`, `OK <text>`, or
//!   `FAIL <reason>`,
//! * unsolicited events arrive as their own datagrams, shaped
//!   `<level>EVENT ...`, and may interleave with replies.
//!
//! Datagram sockets preserve message boundaries, so no line buffering is
//! needed: one `recv` is one message.
//!
//! Events are delivered **only** to connections that have sent `ATTACH`
//! (what `wpa_ctrl_attach()` does). A connection that skips it receives
//! replies and nothing else, which looks exactly like a search that finds
//! no peers.
//!
//! The socket lives at `/run/wpa_supplicant/<iface>` and is group-writable for
//! members of `netdev`, so the whole daemon can run unprivileged. On a
//! hardened system the directory is `0750 root:netdev`, and a user who is not
//! in that group gets `EACCES` even for an interface that does not exist — so
//! the two cases are reported separately, with what to do about each.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("wpa_supplicant control socket not found at {0} — is the interface managed by NetworkManager?")]
    SocketMissing(PathBuf),
    #[error(
        "not allowed to open the wpa_supplicant control socket at {0} — your user is not in the group that owns it (normally `netdev`). Add it with `sudo usermod -aG netdev $USER`, then log out and back in"
    )]
    SocketDenied(PathBuf),
    #[error("creating the local control socket at {path} failed: {source}")]
    LocalSocket {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("i/o error talking to wpa_supplicant: {0}")]
    Io(#[from] std::io::Error),
    #[error("command {cmd:?} failed: {reason}")]
    CommandFailed { cmd: String, reason: String },
    #[error("the control connection is not attached for events (send ATTACH first)")]
    NotAttached,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A connected `wpa_supplicant` control interface.
pub struct Supplicant {
    socket: UnixDatagram,
    /// The local address we bound, which the server replies to.
    local_path: PathBuf,
    pending_events: VecDeque<String>,
    /// Whether `ATTACH` has been accepted on this connection.
    attached: bool,
}

static LOCAL_SEQ: AtomicU64 = AtomicU64::new(0);

impl Drop for Supplicant {
    fn drop(&mut self) {
        // The bound socket file has to outlive the connection: the server
        // addresses us by this path when it sends events.
        let _ = std::fs::remove_file(&self.local_path);
    }
}

impl Supplicant {
    /// Connect to the control interface of `iface`.
    pub fn connect(iface: &str) -> Result<Self> {
        Self::connect_to(PathBuf::from(format!("/run/wpa_supplicant/{iface}")))
    }

    /// Connect to a control socket at an explicit path.
    ///
    /// `connect` uses this with the system path; the tests use it against a
    /// fake control interface, so the protocol can be exercised without a
    /// wireless adapter or root.
    fn connect_to(server: PathBuf) -> Result<Self> {
        let local_path = temp_control_path();

        // `wpa_supplicant` replies to the address the request came from, so
        // the client has to bind a local address first — exactly what
        // `wpa_ctrl_open()` does in `src/ctrl_iface_unix.c`. Without it the
        // request is delivered but the reply is lost and the caller blocks
        // forever.
        let socket = UnixDatagram::bind(&local_path).map_err(|source| Error::LocalSocket {
            path: local_path.clone(),
            source,
        })?;

        // `UnixDatagram::connect` binds the peer and returns `()`, unlike
        // `UnixStream::connect` which hands the socket back.
        socket
            .connect(&server)
            .map_err(|e| connect_error(server.clone(), e))?;

        Ok(Self {
            socket,
            local_path,
            pending_events: VecDeque::new(),
            attached: false,
        })
    }

    /// Register this connection as a monitor for unsolicited events.
    ///
    /// `ATTACH` is what `wpa_ctrl_attach()` sends, and it is not optional:
    /// the control interface delivers events only to attached connections.
    /// Without it, `P2P_FIND` returns `OK` and the supplicant does find
    /// peers, but no `P2P-DEVICE-FOUND` event ever reaches us — discovery
    /// waits out its whole window and reports "no peers" while the peer
    /// table behind it is populated.
    pub fn attach(&mut self) -> Result<()> {
        if !self.attached {
            self.request("ATTACH")?;
            self.attached = true;
        }
        Ok(())
    }

    /// Whether this connection is registered for events.
    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// Send a command and return the reply.
    ///
    /// The reply is one of:
    ///
    /// * `OK` or `OK <text>` — the command succeeded,
    /// * `FAIL <reason>` — the command failed,
    /// * `UNKNOWN COMMAND` — the control interface does not know the command;
    ///   reported as a command failure rather than as an empty reply,
    /// * raw text — for the commands that return a document instead of a
    ///   status (`STATUS`, `WFD_SUBELEM_GET`, `RADIO_WORK show`, `P2P_PEER`).
    ///
    /// The command is sent **without** a trailing newline: the control
    /// interface parses the remainder of the line as command arguments, and
    /// a stray newline makes `WFD_SUBELEM_SET` fail on the hex payload.
    pub fn request(&mut self, cmd: &str) -> Result<String> {
        self.socket.send(cmd.as_bytes())?;
        loop {
            let line = self.recv()?;
            if let Some(rest) = line.strip_prefix("OK") {
                return Ok(rest.trim().to_string());
            }
            if let Some(rest) = line.strip_prefix("FAIL") {
                return Err(Error::CommandFailed {
                    cmd: cmd.to_string(),
                    reason: rest.trim().to_string(),
                });
            }
            // `UNKNOWN COMMAND` is an error the control interface reports as
            // text rather than as `FAIL`; treating it as a successful empty
            // reply hides a command the installed supplicant does not have.
            if line.starts_with("UNKNOWN COMMAND") {
                return Err(Error::CommandFailed {
                    cmd: cmd.to_string(),
                    reason: "the control interface does not know this command".to_string(),
                });
            }
            if line.starts_with('<') {
                // Unsolicited event arriving between request and reply.
                self.pending_events.push_back(line);
                continue;
            }
            return Ok(line);
        }
    }

    /// Wait up to `timeout` for the next unsolicited event.
    ///
    /// Returns `Ok(None)` on timeout. Late replies to earlier commands are
    /// discarded.
    ///
    /// This is an error on an unattached connection rather than a silent
    /// wait: without `ATTACH` the supplicant will never send an event, so a
    /// silent wait is indistinguishable from a search that finds nothing.
    pub fn poll_event(&mut self, timeout: Duration) -> Result<Option<String>> {
        if !self.is_attached() {
            return Err(Error::NotAttached);
        }
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }
        self.socket.set_read_timeout(Some(timeout))?;
        let result = self.recv();
        self.socket.set_read_timeout(None)?;
        match result {
            Ok(line) if line.starts_with('<') => Ok(Some(line)),
            Ok(_) => Ok(None),
            Err(Error::Io(e))
                if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Receive one datagram as a trimmed line.
    ///
    /// A zero-length datagram is a legitimate empty reply (for example
    /// `RADIO_WORK show` when the queue is empty), not a closed connection.
    fn recv(&mut self) -> Result<String> {
        let mut buf = [0u8; 8192];
        let n = self.socket.recv(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n])
            .trim_end_matches(['\n', '\r'])
            .to_string())
    }
}

/// Unused import guard: `Read` is used by `recv` on datagram sockets.
#[allow(dead_code)]
fn _assert_read_in_scope<T: Read>() {}
#[allow(dead_code)]
fn _assert_write_in_scope<T: Write>() {}

/// Classify a failed connection to the control socket.
///
/// `EACCES` is not a missing socket: on Debian and Ubuntu
/// `/run/wpa_supplicant` is `0750 root:netdev`, so a user outside `netdev`
/// is refused before the per-interface socket is even looked at. Reporting
/// that as a generic I/O error sends people looking for the wrong problem.
fn connect_error(path: PathBuf, source: std::io::Error) -> Error {
    match source.kind() {
        ErrorKind::NotFound => Error::SocketMissing(path),
        ErrorKind::PermissionDenied => Error::SocketDenied(path),
        _ => Error::Io(source),
    }
}

/// A unique local address for the control socket.
///
/// `wpa_supplicant` sends its replies to the path the request came from, so
/// this file must exist for as long as the connection is open.
fn temp_control_path() -> PathBuf {
    let seq = LOCAL_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("wpa_ctrl_{}_{}_{}", std::process::id(), nanos, seq))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_missing_socket() {
        // On a host where `/run/wpa_supplicant` is `0750 root:netdev` and the
        // test user is not in `netdev`, the kernel answers `EACCES` instead
        // of `ENOENT`. Both name the interface, and both are honest.
        match Supplicant::connect("definitely-not-an-interface") {
            Err(Error::SocketMissing(path)) | Err(Error::SocketDenied(path)) => {
                assert!(path
                    .to_string_lossy()
                    .contains("definitely-not-an-interface"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("connecting to a missing interface should fail"),
        }
    }

    #[test]
    fn a_permission_error_says_what_to_do() {
        let error = connect_error(
            PathBuf::from("/run/wpa_supplicant/wlan0"),
            std::io::Error::from(ErrorKind::PermissionDenied),
        );
        assert!(matches!(error, Error::SocketDenied(_)));
        let message = error.to_string();
        assert!(message.contains("netdev"), "{message}");
        assert!(message.contains("usermod"), "{message}");
        assert!(!message.contains("os error 13"), "{message}");
    }

    #[test]
    fn a_missing_socket_stays_a_missing_socket() {
        let error = connect_error(
            PathBuf::from("/run/wpa_supplicant/wlan0"),
            std::io::Error::from(ErrorKind::NotFound),
        );
        assert!(matches!(error, Error::SocketMissing(_)));
    }

    #[test]
    fn other_io_errors_are_not_misreported() {
        let error = connect_error(
            PathBuf::from("/run/wpa_supplicant/wlan0"),
            std::io::Error::from(ErrorKind::ConnectionRefused),
        );
        assert!(matches!(error, Error::Io(_)));
    }

    /// A fake `wpa_supplicant` control interface.
    ///
    /// The real one is a datagram socket at `/run/wpa_supplicant/<iface>`
    /// that replies to the address the request came from. A local socket
    /// pair behaves the same way, so the protocol can be tested without a
    /// radio, root, or the real daemon.
    struct Fake {
        socket: UnixDatagram,
        path: PathBuf,
    }

    impl Fake {
        fn start() -> Self {
            static FAKE_SEQ: AtomicU64 = AtomicU64::new(0);
            let seq = FAKE_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aethercast-fake-supplicant-{}-{seq}",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            let socket = UnixDatagram::bind(&path).expect("the fake control socket has to bind");
            Self { socket, path }
        }

        fn connect(&self) -> Supplicant {
            Supplicant::connect_to(self.path.clone()).expect("the client has to connect")
        }

        fn receive(&self, expected: &str) -> PathBuf {
            self.socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0u8; 4096];
            let (n, from) = self
                .socket
                .recv_from(&mut buf)
                .expect("the request has to arrive");
            assert_eq!(std::str::from_utf8(&buf[..n]).unwrap(), expected);
            from.as_pathname()
                .expect("the client has a named socket")
                .to_path_buf()
        }

        fn reply(&self, to: &std::path::Path, text: &str) {
            self.socket
                .send_to(text.as_bytes(), to)
                .expect("the reply has to be sent");
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn a_command_is_sent_verbatim_without_a_newline() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("WFD_SUBELEM_SET all 00010c4400c8");
            fake.reply(&to, "OK\n");
        });
        assert_eq!(
            client.request("WFD_SUBELEM_SET all 00010c4400c8").unwrap(),
            ""
        );
        server.join().unwrap();
    }

    #[test]
    fn the_text_after_ok_is_the_reply() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("PING");
            fake.reply(&to, "OK pong\r\n");
        });
        assert_eq!(client.request("PING").unwrap(), "pong");
        server.join().unwrap();
    }

    #[test]
    fn a_document_reply_is_returned_verbatim() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("STATUS");
            fake.reply(&to, "wpa_state=COMPLETED\nfreq=2412\nssid=test\n");
        });
        assert_eq!(
            client.request("STATUS").unwrap(),
            "wpa_state=COMPLETED\nfreq=2412\nssid=test"
        );
        server.join().unwrap();
    }

    #[test]
    fn a_fail_reply_becomes_a_command_error() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("P2P_FIND 30");
            fake.reply(&to, "FAIL busy\n");
        });
        match client.request("P2P_FIND 30") {
            Err(Error::CommandFailed { cmd, reason }) => {
                assert_eq!(cmd, "P2P_FIND 30");
                assert_eq!(reason, "busy");
            }
            other => panic!("expected a command failure, got {other:?}"),
        }
        server.join().unwrap();
    }

    #[test]
    fn an_unknown_command_is_an_error_not_an_empty_reply() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("WFD_SUBELEM_GET all");
            fake.reply(&to, "UNKNOWN COMMAND\n");
        });
        match client.request("WFD_SUBELEM_GET all") {
            Err(Error::CommandFailed { reason, .. }) => {
                assert!(reason.contains("does not know"), "{reason}");
            }
            other => panic!("expected a command failure, got {other:?}"),
        }
        server.join().unwrap();
    }

    #[test]
    fn an_empty_reply_is_a_successful_empty_document() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("RADIO_WORK show");
            fake.reply(&to, "");
        });
        assert_eq!(client.request("RADIO_WORK show").unwrap(), "");
        server.join().unwrap();
    }

    #[test]
    fn an_event_arriving_before_the_reply_is_kept_for_the_caller() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("ATTACH");
            fake.reply(&to, "OK\n");
            let to = fake.receive("RADIO_WORK show");
            // An unsolicited event can arrive between the request and the
            // reply; it must not be mistaken for the reply.
            fake.reply(&to, "<2>CTRL-EVENT-SCAN-STARTED\n");
            fake.reply(&to, "OK\n");
        });
        client.attach().unwrap();
        assert_eq!(client.request("RADIO_WORK show").unwrap(), "");
        assert_eq!(
            client.poll_event(Duration::from_millis(100)).unwrap(),
            Some("<2>CTRL-EVENT-SCAN-STARTED".to_string())
        );
        server.join().unwrap();
    }

    #[test]
    fn polling_for_an_event_times_out_without_one() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("ATTACH");
            fake.reply(&to, "OK\n");
        });
        client.attach().unwrap();
        assert_eq!(client.poll_event(Duration::from_millis(50)).unwrap(), None);
        server.join().unwrap();
    }

    #[test]
    fn attach_registers_the_connection_for_events() {
        let fake = Fake::start();
        let mut client = fake.connect();
        let server = std::thread::spawn(move || {
            let to = fake.receive("ATTACH");
            fake.reply(&to, "OK\n");
            fake.reply(
                &to,
                "<3>P2P-DEVICE-FOUND aa:bb:cc:dd:ee:ff p2p_dev_addr=11:22:33:44:55:66\n",
            );
        });
        client.attach().unwrap();
        assert!(client.is_attached());
        let event = client
            .poll_event(Duration::from_secs(2))
            .unwrap()
            .expect("the event has to arrive");
        assert!(event.contains("P2P-DEVICE-FOUND"), "{event}");
        server.join().unwrap();
    }

    #[test]
    fn polling_without_attaching_is_an_error() {
        // Silence here is indistinguishable from "no peers", which is the
        // exact bug this guards against.
        let fake = Fake::start();
        let mut client = fake.connect();
        assert!(matches!(
            client.poll_event(Duration::from_millis(10)),
            Err(Error::NotAttached)
        ));
    }
}
