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
//! The socket lives at `/run/wpa_supplicant/<iface>` and is group-writable for
//! members of `netdev`, so the whole daemon can run unprivileged.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("wpa_supplicant control socket not found at {0} — is the interface managed by NetworkManager?")]
    SocketMissing(PathBuf),
    #[error("i/o error talking to wpa_supplicant: {0}")]
    Io(#[from] std::io::Error),
    #[error("command {cmd:?} failed: {reason}")]
    CommandFailed { cmd: String, reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;

/// A connected `wpa_supplicant` control interface.
pub struct Supplicant {
    socket: UnixDatagram,
    /// The local address we bound, which the server replies to.
    local_path: PathBuf,
    pending_events: Vec<String>,
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
        let server = PathBuf::from(format!("/run/wpa_supplicant/{iface}"));
        let local_path = temp_control_path();

        // `wpa_supplicant` replies to the address the request came from, so
        // the client has to bind a local address first — exactly what
        // `wpa_ctrl_open()` does in `src/ctrl_iface_unix.c`. Without it the
        // request is delivered but the reply is lost and the caller blocks
        // forever.
        let socket = UnixDatagram::bind(&local_path).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                Error::SocketMissing(local_path.clone())
            } else {
                Error::Io(e)
            }
        })?;

        // `UnixDatagram::connect` binds the peer and returns `()`, unlike
        // `UnixStream::connect` which hands the socket back.
        socket.connect(&server).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                Error::SocketMissing(server.clone())
            } else {
                Error::Io(e)
            }
        })?;

        Ok(Self {
            socket,
            local_path,
            pending_events: Vec::new(),
        })
    }

    /// Send a command and return the reply.
    ///
    /// The reply is one of:
    ///
    /// * `OK` or `OK <text>` — the command succeeded,
    /// * `FAIL <reason>` — the command failed,
    /// * `UNKNOWN COMMAND` — the control interface does not know the command,
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
            if line.starts_with('<') {
                // Unsolicited event arriving between request and reply.
                self.pending_events.push(line);
                continue;
            }
            return Ok(line);
        }
    }

    /// Wait up to `timeout` for the next unsolicited event.
    ///
    /// Returns `Ok(None)` on timeout. Late replies to earlier commands are
    /// discarded.
    pub fn poll_event(&mut self, timeout: Duration) -> Result<Option<String>> {
        if let Some(event) = self.pending_events.pop() {
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
        match Supplicant::connect("definitely-not-an-interface") {
            Err(Error::SocketMissing(path)) => {
                assert!(path
                    .to_string_lossy()
                    .contains("definitely-not-an-interface"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("connecting to a missing interface should fail"),
        }
    }
}
