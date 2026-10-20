//! The WFD RTSP handshake.
//!
//! After the P2P group is up, the **sink opens a TCP connection to the
//! source's control port** (7236 by default) and the source is the RTSP
//! *server* [1 §4.5.4]. WFD's method set is a profile of RFC 2326:
//!
//! ```text
//! sink -> source : TCP connect
//! source -> sink : M1  OPTIONS, Require: org.wfa.wfd1.0
//! sink -> source : 200 OK
//! sink -> source : M2  OPTIONS
//! source -> sink : 200 OK          (the source's public device information)
//! source -> sink : M3  GET_PARAMETER (ports, codecs, formats, UIBC, protection)
//! sink -> source : 200 OK with the sink's capabilities
//! source -> source: intersect codec, level, resolution, audio and ports
//! source -> sink : M4  SET_PARAMETER (formats, codecs, URL, ports)
//! sink -> source : 200 OK, or 415 / 457
//! source -> sink : M5  SET_PARAMETER (wfd_trigger_method: SETUP)
//! sink -> source : M6  SETUP with Transport: client-port=...
//! source -> sink : 200 OK with Session: <id>;timeout=60
//! sink -> source : M7  PLAY
//! source -> sink : 100 Continue
//! ```
//!
//! Timeouts [1 §6.5]: 6 s to send M1 after the connection, 5 s per exchange,
//! 6 s between messages before the session is up. The keep-alive timeout goes
//! in the `Session:` header, and M16 has to be sent more often than
//! `timeout - 5 s`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// The WFD protocol identifier used in the `Require` and `Public` headers.
pub const WFD_PUBLIC: &str = "org.wfa.wfd1.0";

/// The default WFD session management control port.
pub const DEFAULT_CONTROL_PORT: u16 = 7236;

/// A parsed RTSP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub uri: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    /// Parse a complete RTSP request.
    pub fn parse(raw: &str) -> Result<Self, ParseError> {
        let raw = raw.replace("\r\n", "\n");
        let (head, body) = match raw.split_once("\n\n") {
            Some((head, body)) => (head, body.to_string()),
            None => (raw.as_str(), String::new()),
        };
        let mut lines = head.lines();
        let request_line = lines.next().ok_or(ParseError::Empty)?;
        let mut parts = request_line.split_whitespace();
        let method = parts
            .next()
            .ok_or(ParseError::Malformed)?
            .to_string();
        let uri = parts.next().ok_or(ParseError::Malformed)?.to_string();
        let version = parts.next().unwrap_or("RTSP/1.0").to_string();

        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.push((key.trim().to_string(), value.trim().to_string()));
            }
        }
        Ok(Self {
            method,
            uri,
            version,
            headers,
            body,
        })
    }

    /// The value of a header, case insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The `CSeq` header, if present.
    pub fn cseq(&self) -> Option<&str> {
        self.header("CSeq")
    }

    /// The body, split into `key=value` lines.
    pub fn body_pairs(&self) -> Vec<(String, String)> {
        self.body
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                line.split_once('=')
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            })
            .collect()
    }
}

/// Why a request could not be parsed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("the request was empty")]
    Empty,
    #[error("the request line was malformed")]
    Malformed,
}

/// Build an RTSP response.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    /// A `200 OK` with the given headers.
    pub fn ok() -> Self {
        Self {
            status: 200,
            reason: "OK".into(),
            headers: Vec::new(),
            body: String::new(),
        }
    }

    /// A response with a status code and reason.
    pub fn with_status(status: u16, reason: &str) -> Self {
        Self {
            status,
            reason: reason.into(),
            headers: Vec::new(),
            body: String::new(),
        }
    }

    /// Add a header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Set the body.
    pub fn body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// Render the response.
    pub fn to_wire(&self) -> String {
        let mut out = format!("RTSP/1.0 {} {}\r\n", self.status, self.reason);
        for (key, value) in &self.headers {
            out.push_str(&format!("{key}: {value}\r\n"));
        }
        if !self.body.is_empty() {
            out.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        }
        out.push_str("\r\n");
        out.push_str(&self.body);
        out
    }
}

/// The capabilities a sink reports in its M3 response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SinkCapabilities {
    /// The sink's RTP ports, as `rtp-port0` and `rtp-port1`.
    pub rtp_port0: Option<u16>,
    /// The second RTP port. This is **not** RTCP: a value of zero means the
    /// sink wants a single MPEG-TS stream [1 Tables 89-90].
    pub rtp_port1: Option<u16>,
    /// The video formats the sink accepts, as the raw string.
    pub video_formats: Option<String>,
    /// The audio codecs the sink accepts.
    pub audio_codecs: Option<String>,
    /// Whether the sink wants input backchannel.
    pub uibc_capability: Option<String>,
    /// The content protection the sink requires.
    pub content_protection: Option<String>,
    /// The sink's display EDID.
    pub display_edid: Option<String>,
}

impl SinkCapabilities {
    /// Parse the body of an M3 response.
    pub fn parse(body: &str) -> Self {
        let mut caps = SinkCapabilities::default();
        for (key, value) in body
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                line.split_once('=')
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            })
        {
            match key.as_str() {
                "wfd_client_rtp_ports" => {
                    caps.rtp_port0 = parse_rtp_port(&value, "rtp-port0");
                    caps.rtp_port1 = parse_rtp_port(&value, "rtp-port1");
                }
                "wfd_video_formats" => caps.video_formats = Some(value),
                "wfd_audio_codecs" => caps.audio_codecs = Some(value),
                "wfd_uibc_capability" => caps.uibc_capability = Some(value),
                "wfd_content_protection" => caps.content_protection = Some(value),
                "wfd_display_edid" => caps.display_edid = Some(value),
                _ => {}
            }
        }
        caps
    }

    /// True when the sink asked for a single MPEG-TS stream.
    pub fn is_primary_sink(&self) -> bool {
        self.rtp_port0.is_some() && self.rtp_port1.unwrap_or(0) == 0
    }
}

/// Pull one `rtp-portN=` value out of a `wfd_client_rtp_ports` value.
fn parse_rtp_port(value: &str, name: &str) -> Option<u16> {
    value.split(';').find_map(|part| {
        let (key, val) = part.split_once('=')?;
        if key.trim().eq_ignore_ascii_case(name) {
            val.trim().parse().ok()
        } else {
            None
        }
    })
}

/// What the handshake reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the sink to connect.
    Connected,
    /// M1 sent, M2 answered.
    M1M2,
    /// M3 answered, the sink's capabilities are known.
    M3,
    /// M4 accepted: the sink accepted our format selection.
    M4,
    /// M6 answered: the transport is set up.
    M6,
    /// M7: the sink is playing.
    Playing,
}

/// The WFD RTSP server.
pub struct Server {
    listener: TcpListener,
}

impl Server {
    /// Put the listener in non-blocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.listener.set_nonblocking(nonblocking)
    }

    /// Bind the control port.
    pub fn bind(port: u16) -> std::io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(("0.0.0.0", port))?,
        })
    }

    /// The port the server is listening on.
    pub fn port(&self) -> u16 {
        self.listener.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// Accept one sink and run the WFD handshake against it.
    ///
    /// Returns the stage reached and the sink's capabilities.
    pub fn handle_once(&self, log: &mut dyn FnMut(&str)) -> std::io::Result<(Stage, Option<SinkCapabilities>)> {
        let (mut stream, peer) = self.listener.accept()?;
        log(&format!("sink connected from {}", peer));
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;

        // M1: the source asks the sink whether it speaks WFD.
        let m1 = format!(
            "OPTIONS rtsp://{}/wfd RTSP/1.0\r\nCSeq: 1\r\nRequire: {WFD_PUBLIC}\r\nUser-Agent: AetherCast/0.1\r\n\r\n",
            peer.ip()
        );
        write_line(&mut stream, &m1)?;
        log("-> M1 OPTIONS (Require: org.wfa.wfd1.0)");
        let reply = read_message(&mut stream)?;
        log(&format!("<- {}", reply.lines().next().unwrap_or_default()));

        // M2: the sink asks us the same question.
        let request = match read_message(&mut stream) {
            Ok(text) => match Request::parse(&text) {
                Ok(request) => request,
                Err(e) => {
                    log(&format!("unparsable request: {e}"));
                    return Ok((Stage::Connected, None));
                }
            },
            Err(e) => {
                log(&format!("no M2 from the sink: {e}"));
                return Ok((Stage::Connected, None));
            }
        };
        log(&format!("<- M2 {} {}", request.method, request.uri));
        let response = Response::ok()
            .header("CSeq", request.cseq().unwrap_or("2"))
            .header("Server", "AetherCast/0.1")
            .header("Public", WFD_PUBLIC)
            .header("WFD-Content-Protection", "none");
        write_line(&mut stream, &response.to_wire())?;
        log("-> M2 200 OK (Public: org.wfa.wfd1.0)");

        // M3: ask the sink what it can accept.
        let m3 = format!(
            "GET_PARAMETER rtsp://{}/wfd?wfd_client_rtp_ports;wfd_video_formats;wfd_audio_codecs;wfd_uibc_capability;wfd_display_edid;wfd_content_protection RTSP/1.0\r\nCSeq: 3\r\nRequire: {WFD_PUBLIC}\r\n\r\n",
            peer.ip()
        );
        write_line(&mut stream, &m3)?;
        log("-> M3 GET_PARAMETER (ports, formats, codecs, UIBC, protection)");
        let reply = read_message(&mut stream)?;
        log(&format!("<- {}", reply.lines().next().unwrap_or_default()));
        let capabilities = SinkCapabilities::parse(&reply);
        log(&format!(
            "   sink capabilities: ports={:?}/{:?} video={:?} audio={:?}",
            capabilities.rtp_port0,
            capabilities.rtp_port1,
            capabilities.video_formats,
            capabilities.audio_codecs
        ));

        // M4: choose a format and tell the sink.
        let m4 = format!(
            "SET_PARAMETER rtsp://{}/wfd RTSP/1.0\r\nCSeq: 4\r\nRequire: {WFD_PUBLIC}\r\nContent-Type: application/wfd\r\nContent-Length: 0\r\nwfd_video_formats=32,640x480p30-1\r\nwfd_audio_codecs=LPCM/44100/2\r\nwfd_presentation_URL=rtsp://{}/wfd\r\n\r\n",
            peer.ip(),
            peer.ip()
        );
        write_line(&mut stream, &m4)?;
        log("-> M4 SET_PARAMETER (video formats, audio codecs, presentation URL)");
        let reply = read_message(&mut stream)?;
        log(&format!("<- {}", reply.lines().next().unwrap_or_default()));
        let accepted = reply.starts_with("RTSP/1.0 200");
        if !accepted {
            log("the sink refused our format selection");
            return Ok((Stage::M3, Some(capabilities)));
        }

        // M5: trigger the sink's SETUP.
        let m5 = format!(
            "SET_PARAMETER rtsp://{}/wfd RTSP/1.0\r\nCSeq: 5\r\nRequire: {WFD_PUBLIC}\r\nContent-Type: application/wfd\r\nContent-Length: 0\r\nwfd_trigger_method=SETUP\r\n\r\n",
            peer.ip()
        );
        write_line(&mut stream, &m5)?;
        log("-> M5 SET_PARAMETER (wfd_trigger_method: SETUP)");
        let reply = read_message(&mut stream)?;
        log(&format!("<- {}", reply.lines().next().unwrap_or_default()));

        // M6: the sink sets the transport up.
        let request = match read_message(&mut stream) {
            Ok(text) => match Request::parse(&text) {
                Ok(request) => request,
                Err(e) => {
                    log(&format!("unparsable SETUP: {e}"));
                    return Ok((Stage::M4, Some(capabilities)));
                }
            },
            Err(e) => {
                log(&format!("no SETUP from the sink: {e}"));
                return Ok((Stage::M4, Some(capabilities)));
            }
        };
        log(&format!(
            "<- M6 {} (Transport: {})",
            request.method,
            request.header("Transport").unwrap_or("-")
        ));
        let session = format!("{};timeout=60", session_id(&peer));
        let response = Response::ok()
            .header("CSeq", request.cseq().unwrap_or("6"))
            .header("Server", "AetherCast/0.1")
            .header("Public", WFD_PUBLIC)
            .header("Session", &session);
        write_line(&mut stream, &response.to_wire())?;
        log(&format!("-> M6 200 OK (Session: {session})"));

        // M7: the sink starts playing.
        let request = match read_message(&mut stream) {
            Ok(text) => match Request::parse(&text) {
                Ok(request) => request,
                Err(e) => {
                    log(&format!("unparsable PLAY: {e}"));
                    return Ok((Stage::M6, Some(capabilities)));
                }
            },
            Err(e) => {
                log(&format!("no PLAY from the sink: {e}"));
                return Ok((Stage::M6, Some(capabilities)));
            }
        };
        log(&format!("<- M7 {}", request.method));
        let response = Response::with_status(100, "Continue")
            .header("CSeq", request.cseq().unwrap_or("7"))
            .header("Session", &session);
        write_line(&mut stream, &response.to_wire())?;
        log("-> M7 100 Continue");

        Ok((Stage::Playing, Some(capabilities)))
    }
}

/// A deterministic session identifier for a peer.
fn session_id(peer: &std::net::SocketAddr) -> String {
    let mut hash: u32 = 2166136261;
    for byte in peer.ip().to_string().as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    format!("{hash:08X}")
}

/// Read one complete RTSP message: headers, then the body if the headers
/// announce one.
fn read_message(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    // Read until the end of the headers.
    loop {
        if stream.read(&mut byte)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the sink closed the connection",
            ));
        }
        raw.push(byte[0]);
        if raw.ends_with(b"\r\n\r\n") || raw.ends_with(b"\n\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&raw).to_string();
    let length = head
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.trim().eq_ignore_ascii_case("Content-Length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    if length == 0 {
        return Ok(head);
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Ok(format!("{head}{}", String::from_utf8_lossy(&body)))
}

/// Write a message to the sink.
fn write_line(stream: &mut TcpStream, message: &str) -> std::io::Result<()> {
    stream.write_all(message.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_options_request() {
        let raw = "OPTIONS rtsp://192.168.1.2/wfd RTSP/1.0\r\nCSeq: 2\r\nRequire: org.wfa.wfd1.0\r\n\r\n";
        let request = Request::parse(raw).unwrap();
        assert_eq!(request.method, "OPTIONS");
        assert_eq!(request.uri, "rtsp://192.168.1.2/wfd");
        assert_eq!(request.cseq(), Some("2"));
        assert_eq!(request.header("require"), Some("org.wfa.wfd1.0"));
    }

    #[test]
    fn parses_a_request_with_a_body() {
        let raw = "GET_PARAMETER rtsp://x/wfd RTSP/1.0\r\nCSeq: 3\r\n\r\nwfd_client_rtp_ports=RTP/AVP/UDP;unicast;rtp-port0=50000;rtp-port1=0\n";
        let request = Request::parse(raw).unwrap();
        assert_eq!(request.method, "GET_PARAMETER");
        assert_eq!(request.body_pairs().len(), 1);
    }

    #[test]
    fn parses_the_sink_rtp_ports() {
        let caps = SinkCapabilities::parse(
            "wfd_client_rtp_ports=RTP/AVP/UDP;unicast;rtp-port0=50000;rtp-port1=0\n\
             wfd_video_formats=32,1280x720p30-1\n\
             wfd_audio_codecs=LPCM/44100/2\n",
        );
        assert_eq!(caps.rtp_port0, Some(50000));
        assert_eq!(caps.rtp_port1, Some(0));
        assert!(caps.is_primary_sink());
        assert_eq!(
            caps.video_formats.as_deref(),
            Some("32,1280x720p30-1")
        );
    }

    #[test]
    fn a_secondary_sink_reports_only_the_second_port() {
        let caps = SinkCapabilities::parse(
            "wfd_client_rtp_ports=RTP/AVP/UDP;unicast;rtp-port0=0;rtp-port1=50002\n",
        );
        assert_eq!(caps.rtp_port0, Some(0));
        assert_eq!(caps.rtp_port1, Some(50002));
        assert!(!caps.is_primary_sink());
    }

    #[test]
    fn renders_a_response() {
        let response = Response::ok()
            .header("CSeq", "2")
            .header("Public", WFD_PUBLIC);
        let wire = response.to_wire();
        assert!(wire.starts_with("RTSP/1.0 200 OK\r\n"));
        assert!(wire.contains("CSeq: 2\r\n"));
        assert!(wire.contains("Public: org.wfa.wfd1.0\r\n"));
    }
}
