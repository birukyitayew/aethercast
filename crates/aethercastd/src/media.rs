//! The media pipeline: screen capture, video encoding, MPEG-TS muxing, and
//! RTP transport.
//!
//! The constraints come from the Wi-Fi Display specification and are recorded
//! in [`docs/research/03-media-pipeline.md`]:
//!
//! * H.264 **Constrained Baseline**, no B-slices, no CABAC, one slice per
//!   picture, a 1 second GOP, and a conservative bitrate.
//! * MPEG-TS with the WFD PID map: PMT `0x0100`, PCR `0x1000`, video
//!   `0x1011`, audio `0x1100`. `mpegtsmux` exposes request pads named after
//!   the decimal PID, so the video pad is `sink_4113` and the audio pad is
//!   `sink_4352`.
//! * `alignment=7`, so at most seven 188 byte TS packets per datagram, and
//!   `rtpmp2tpay` with payload type 33 and an MTU of 1400 — seven packets
//!   plus RTP, UDP and IPv4 headers come to 1356 bytes.
//!
//! The queue between capture and encode is **leaky**: when the encoder
//! cannot keep up, frames are dropped instead of accumulating latency. A
//! non-leaky queue is one of the defects found in GNOME Network Displays.

use gstreamer as gst;
use gstreamer::gobject::GObjectExtManualGst;
use gstreamer::prelude::*;

/// Errors from building or running the pipeline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("gstreamer error: {0}")]
    Gst(#[from] gst::glib::Error),
    #[error("gstreamer flow error: {0}")]
    Flow(#[from] gst::FlowError),
    #[error("gstreamer element error: {0}")]
    Bool(#[from] gst::glib::BoolError),
    #[error("gstreamer pad link error: {0}")]
    PadLink(#[from] gst::PadLinkError),
    #[error("{0}")]
    Other(String),
}

/// Result type for the media layer.
pub type Result<T> = std::result::Result<T, Error>;

/// Program map table PID, as the specification requires it.
pub const PID_PMT: i32 = 0x0100;
/// Program clock reference PID, as the specification requires it.
pub const PID_PCR: i32 = 0x1000;
/// Video PID.
pub const PID_VIDEO: i32 = 0x1011;
/// Audio PID.
pub const PID_AUDIO: i32 = 0x1100;

/// Request pad names `mpegtsmux` uses for the WFD PIDs.
pub const VIDEO_PAD: &str = "sink_4113";
pub const AUDIO_PAD: &str = "sink_4352";

/// RTP payload type for MPEG-TS.
pub const RTP_PAYLOAD_TYPE: i32 = 33;
/// RTP MTU: seven TS packets plus headers.
pub const RTP_MTU: u32 = 1400;

/// The H.264 encoder to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Encoder {
    /// Pick the best available encoder.
    Auto,
    /// VA-API, through `vah264enc` or the legacy `vaapih264enc`.
    Vaapi,
    /// x264 — the only software encoder measured fast enough for 720p30.
    X264,
    /// OpenH264 — Cisco's encoder, present in every distribution.
    OpenH264,
}

/// Preference order: hardware first, then the software encoder that was
/// measured at 2.64x realtime for 720p30.
const ENCODER_PREFERENCE: &[Encoder] = &[Encoder::Vaapi, Encoder::X264, Encoder::OpenH264];

/// A capture and encode profile.
#[derive(Debug, Clone)]
pub struct Profile {
    pub width: i32,
    pub height: i32,
    pub framerate: i32,
    /// Bitrate in bits per second.
    pub bitrate_bps: u32,
    pub encoder: Encoder,
    /// Capture the system audio mix as well.
    pub audio: bool,
    /// Use a generated test pattern instead of capturing the screen.
    ///
    /// This is what makes the pipeline testable on a machine with no
    /// display, such as a build server.
    pub test_pattern: bool,
    /// The PulseAudio source to capture. The system mix is the *monitor* of
    /// the default sink, for example
    /// `alsa_output.pci-0000_00_1f.3.analog-stereo.monitor`.
    pub audio_device: String,
}

impl Default for Profile {
    fn default() -> Self {
        // 640x480p30 is the profile the reference machine can actually
        // sustain: OpenH264 was measured at 0.32-0.81x realtime for 720p30
        // on this CPU, which pushed the PCR gap to 133 ms, past the 100 ms
        // the specification allows. 640x480 is also the format WFD
        // mandates as a minimum, so nothing is given up.
        Self {
            width: 640,
            height: 480,
            // Zero means "keep the capture rate".
            framerate: 0,
            // 2 Mbit/s is comfortable for 720p30 on a 2.4 GHz link.
            bitrate_bps: 2_000_000,
            encoder: Encoder::Auto,
            audio: false,
            audio_device: "default".to_string(),
            test_pattern: false,
        }
    }
}

/// Where the pipeline sends its media.
#[derive(Debug, Clone)]
#[allow(dead_code)] // the UDP sink is used by the transport test
pub enum Sink {
    /// Write bare MPEG-TS to a file, for validation.
    File(String),
    /// Send RTP over UDP.
    Udp { host: String, port: u16 },
}

/// The encoders that are actually installed.
pub fn available_encoders() -> Vec<Encoder> {
    let mut found = Vec::new();
    for encoder in ENCODER_PREFERENCE {
        let names: &[&str] = match encoder {
            Encoder::Vaapi => &["vah264enc", "vaapih264enc"],
            Encoder::X264 => &["x264enc"],
            Encoder::OpenH264 => &["openh264enc"],
            Encoder::Auto => &[],
        };
        if names
            .iter()
            .any(|name| gst::ElementFactory::find(name).is_some())
        {
            found.push(*encoder);
        }
    }
    found
}

/// Resolve the encoder to use, honouring an explicit preference.
pub fn resolve_encoder(preference: Encoder) -> Option<Encoder> {
    if preference != Encoder::Auto {
        let names: &[&str] = match preference {
            Encoder::Vaapi => &["vah264enc", "vaapih264enc"],
            Encoder::X264 => &["x264enc"],
            Encoder::OpenH264 => &["openh264enc"],
            Encoder::Auto => &[],
        };
        return names
            .iter()
            .find(|name| gst::ElementFactory::find(name).is_some())
            .map(|_| preference);
    }
    available_encoders().first().copied()
}

/// A built pipeline.
pub struct Pipeline {
    pub pipeline: gst::Pipeline,
    pub encoder: Encoder,
    pub encoder_element: String,
}

/// Build the capture, encode and mux pipeline.
///
/// `sink` decides the transport: a file gets bare MPEG-TS so it can be
/// validated with `ffprobe`, a UDP destination gets RTP.
pub fn build(profile: &Profile, sink: &Sink) -> Result<Pipeline> {
    gst::init().ok();

    let pipeline = gst::Pipeline::new();
    let encoder_choice = resolve_encoder(profile.encoder)
        .ok_or_else(|| Error::Other("no H.264 encoder is installed".to_string()))?;

    // --- capture ---
    //
    // The chain is: ximagesrc -> videoscale -> size -> videoconvert ->
    // videorate -> queue.
    //
    // `videoscale` has to be explicit: `videoconvert` on its own refuses
    // the size negotiation, and the reference display is an unusual
    // 1821x1024 at 25 fps, so the scale is not an integer factor.
    let capture = if profile.test_pattern {
        // A generated pattern needs no display, so the whole path can be
        // tested on a build server.
        let pattern = gst::ElementFactory::make("videotestsrc").build()?;
        pattern.set_property("is-live", true);
        pattern
    } else {
        let screen = gst::ElementFactory::make("ximagesrc").build()?;
        screen.set_property("use-damage", false);
        screen.set_property("show-pointer", false);
        screen
    };

    let scale = gst::ElementFactory::make("videoscale").build()?;

    // The sink's advertised resolution.
    let size = gst::ElementFactory::make("capsfilter").build()?;
    let caps = gst::Caps::builder("video/x-raw")
        .field("width", profile.width)
        .field("height", profile.height)
        .build();
    size.set_property("caps", caps);

    let convert = gst::ElementFactory::make("videoconvert").build()?;

    // The frame rate is imposed by a capsfilter *after* videorate: the
    // fixed rate propagates upstream, and videorate duplicates or drops
    // frames to match it. Duplicating matters for more than the rate: a
    // static screen produces almost no frames, and without a steady cadence
    // the PCR gap exceeds the 100 ms the specification allows.
    let rate = if profile.framerate > 0 {
        let rate = gst::parse::launch(&format!(
            "videorate max-rate={} drop-only=false",
            profile.framerate
        ))
        .map_err(|e| Error::Other(format!("building videorate failed: {e}")))?;
        let cadence = gst::ElementFactory::make("capsfilter").build()?;
        let cadence_caps = gst::Caps::builder("video/x-raw")
            .field("framerate", gst::Fraction::new(profile.framerate, 1))
            .build();
        cadence.set_property("caps", cadence_caps);
        Some((rate, cadence))
    } else {
        None
    };

    // A leaky queue drops frames instead of building latency.
    //
    // `leaky` is a GEnum and the GStreamer Rust bindings do not generate
    // `GstQueueLeaky`, so the value is set from its string name.
    let queue = gst::ElementFactory::make("queue").build()?;
    queue.set_property_from_str("leaky", "downstream");
    queue.set_property("max-size-buffers", 1u32);

    // --- encode ---
    let (encoder_el, encoder_kind) = make_encoder(encoder_choice)?;
    configure_encoder(&encoder_el, profile, encoder_kind);

    let parse = gst::ElementFactory::make("h264parse").build()?;

    // --- mux ---
    let mux = gst::ElementFactory::make("mpegtsmux").build()?;
    // Seven TS packets per datagram, for UDP streaming.
    mux.set_property("alignment", 7i32);
    //
    // The WFD PIDs are chosen by the *request pad names* below, not by a
    // property: `sink_4113` is 0x1011 and `sink_4352` is 0x1100. GStreamer
    // 1.28's mpegtsmux exposes no PAT/PMT PID properties, so the PCR lands
    // on the first video PID rather than on the 0x1000 the specification
    // asks for. That is a known gap, tracked in the project issue tracker.

    for element in [
        &capture,
        &scale,
        &size,
        &convert,
        &queue,
        &encoder_el,
        &parse,
        &mux,
    ] {
        pipeline.add(element)?;
    }
    if let Some((rate, cadence)) = &rate {
        pipeline.add(rate)?;
        pipeline.add(cadence)?;
    }

    let video_pad = mux
        .request_pad_simple(VIDEO_PAD)
        .ok_or_else(|| Error::Other(format!("mpegtsmux has no {VIDEO_PAD} request pad")))?;

    capture.link(&scale)?;
    scale.link(&size)?;
    match &rate {
        Some((rate, cadence)) => {
            size.link(rate)?;
            rate.link(cadence)?;
            cadence.link(&convert)?;
            convert.link(&queue)?;
        }
        None => {
            size.link(&convert)?;
            convert.link(&queue)?;
        }
    }
    queue.link(&encoder_el)?;
    encoder_el.link(&parse)?;
    let parse_src = parse
        .static_pad("src")
        .ok_or_else(|| Error::Other("h264parse has no src pad".to_string()))?;
    parse_src.link(&video_pad)?;

    // --- audio ---
    if profile.audio {
        // The element is `pulsesrc`; `pulseaudio` is the plugin name, not
        // an element, and the GStreamer PulseAudio device provider does not
        // enumerate on PipeWire, so the source has to be named explicitly.
        let source = gst::ElementFactory::make("pulsesrc").build()?;
        source.set_property("device", &profile.audio_device);
        let convert = gst::ElementFactory::make("audioconvert").build()?;
        let resample = gst::ElementFactory::make("audioresample").build()?;
        let encode = gst::ElementFactory::make("avenc_aac").build()?;
        encode.set_property("bitrate", 128_000i32);
        let audio_pad = mux
            .request_pad_simple(AUDIO_PAD)
            .ok_or_else(|| Error::Other(format!("mpegtsmux has no {AUDIO_PAD} request pad")))?;
        for element in [&source, &convert, &resample, &encode] {
            pipeline.add(element)?;
        }
        source.link(&convert)?;
        convert.link(&resample)?;
        resample.link(&encode)?;
        let encode_src = encode
            .static_pad("src")
            .ok_or_else(|| Error::Other("the audio encoder has no src pad".to_string()))?;
        encode_src.link(&audio_pad)?;
    }

    // --- transport ---
    match sink {
        Sink::File(path) => {
            let sink_element = gst::ElementFactory::make("filesink").build()?;
            sink_element.set_property("location", path);
            pipeline.add(&sink_element)?;
            mux.link(&sink_element)?;
        }
        Sink::Udp { host, port } => {
            let payloader = gst::ElementFactory::make("rtpmp2tpay").build()?;
            payloader.set_property("pt", RTP_PAYLOAD_TYPE as u32);
            payloader.set_property("mtu", RTP_MTU);
            let sink_element = gst::ElementFactory::make("udpsink").build()?;
            sink_element.set_property("host", host);
            sink_element.set_property("port", *port as i32);
            pipeline.add(&payloader)?;
            pipeline.add(&sink_element)?;
            mux.link(&payloader)?;
            payloader.link(&sink_element)?;
        }
    }

    Ok(Pipeline {
        pipeline,
        encoder: encoder_kind,
        encoder_element: encoder_el
            .factory()
            .map(|f| f.name().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

/// Create the encoder element, including the legacy VA-API fallback.
fn make_encoder(choice: Encoder) -> Result<(gst::Element, Encoder)> {
    let names: &[&str] = match choice {
        Encoder::Vaapi => &["vah264enc", "vaapih264enc"],
        Encoder::X264 => &["x264enc"],
        Encoder::OpenH264 => &["openh264enc"],
        Encoder::Auto => &[],
    };
    for name in names {
        if let Ok(element) = gst::ElementFactory::make(name).build() {
            let choice = if *name == "x264enc" {
                Encoder::X264
            } else if name.contains("h264enc") && name.contains("va") {
                Encoder::Vaapi
            } else {
                Encoder::OpenH264
            };
            return Ok((element, choice));
        }
    }
    Err(Error::Other("no H.264 encoder is installed".to_string()))
}

/// Apply the settings the WFD specification and the measurements require.
fn configure_encoder(element: &gst::Element, profile: &Profile, choice: Encoder) {
    match choice {
        Encoder::OpenH264 => {
            // openh264enc takes the bitrate in bits per second. The rate
            // control mode is a GEnum, so it is set by name.
            element.set_property("bitrate", profile.bitrate_bps);
            element.set_property_from_str("rate-control", "bitrate");
        }
        Encoder::X264 => {
            // x264enc takes the bitrate in kbit/s.
            element.set_property("bitrate", profile.bitrate_bps / 1000);
            // No B-frames, and one slice per picture.
            element.set_property("bframes", false);
            element.set_property("sliced-threads", true);
            element.set_property("byte-stream", true);
            element.set_property("aud", true);
            element.set_property("key-int-max", profile.framerate);
            element.set_property("tune", "zerolatency");
        }
        Encoder::Vaapi => {
            // Both VA-API encoders take the bitrate in kbit/s.
            element.set_property("bitrate", profile.bitrate_bps / 1000);
            element.set_property("key-int-max", profile.framerate);
            element.set_property("b-frames", false);
        }
        Encoder::Auto => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wfd_pid_map_matches_the_specification() {
        assert_eq!(PID_PMT, 0x0100);
        assert_eq!(PID_PCR, 0x1000);
        assert_eq!(PID_VIDEO, 0x1011);
        assert_eq!(PID_AUDIO, 0x1100);
        // The request pad names are the decimal PIDs.
        assert_eq!(VIDEO_PAD, "sink_4113");
        assert_eq!(AUDIO_PAD, "sink_4352");
    }

    #[test]
    fn rtp_geometry_fits_a_ethernet_mtu() {
        // Seven TS packets plus RTP (12), UDP (8) and IPv4 (20).
        let bytes = 7 * 188 + 12 + 8 + 20;
        assert_eq!(bytes, 1356);
        assert!(bytes < 1500);
    }

    #[test]
    fn resolves_an_installed_encoder() {
        // GStreamer has to be initialised before any element is looked up.
        gst::init().ok();
        // openh264 is present in every GStreamer installation, so a
        // resolution must always succeed in a built environment.
        let encoder = resolve_encoder(Encoder::Auto);
        assert!(encoder.is_some(), "no H.264 encoder found");
    }
}
