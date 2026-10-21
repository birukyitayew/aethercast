//! Running a cast session.
//!
//! The same code serves the command line and the D-Bus API, so the two can
//! never disagree about what a session does.

use std::net::IpAddr;

use anyhow::Context;
use gstreamer::prelude::*;

use crate::core::Manager;
use crate::media;
use crate::radio::Radio;
use crate::rtsp;

/// How a session should be run.
#[derive(Debug, Clone)]
pub struct Options {
    /// The RTSP control port to listen on.
    pub port: u16,
    /// How long to wait for a sink to connect, in seconds.
    pub wait: u32,
    /// How long to stream for, in seconds. Zero means until the sink leaves.
    pub seconds: u32,
    /// Form a Wi-Fi Direct group and serve the session on it.
    pub group: bool,
    /// The frequency for the group, in MHz.
    pub freq: Option<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            port: rtsp::DEFAULT_CONTROL_PORT,
            wait: 60,
            seconds: 30,
            group: false,
            freq: None,
        }
    }
}

/// Run a cast session: wait for a sink, negotiate with it, and stream.
pub fn run(
    manager: &Manager,
    profile: &media::Profile,
    options: &Options,
    log: &mut dyn FnMut(&str),
) -> anyhow::Result<()> {
    // On a Wi-Fi Display session the sink reaches us on the group
    // interface, so the group has to exist before the server is bound.
    // The radio is optional: a sink that already found us does not need us
    // to be advertising, and a build server has no adapter at all.
    let mut radio = match Radio::open(manager.interface()) {
        Ok(radio) => Some(radio),
        Err(e) => {
            if options.group {
                return Err(e).context("forming a group needs the wireless adapter");
            }
            log(&format!(
                "note: no wireless adapter ({e}), serving the session anyway"
            ));
            None
        }
    };

    let mut group_address = None;
    if options.group {
        let radio = radio
            .as_mut()
            .expect("the radio was opened for a group request");
        // A group left over from an earlier session can make the next one
        // fail with "Name not unique on network", so clear it first.
        let _ = radio.p2p_group_remove();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let removed = radio.remove_stale_group_interfaces()?;
        if !removed.is_empty() {
            log(&format!(
                "removed stale group interface(s): {}",
                removed.join(", ")
            ));
        } else if !radio.p2p_group_interfaces().is_empty() {
            let stale = radio.p2p_group_interfaces().join(", ");
            log(&format!(
                "note: {stale} is left over and needs 'sudo iw dev {stale} del'"
            ));
        }

        let before = radio.p2p_group_interfaces();
        radio.p2p_group_add(options.freq)?;
        log(&format!("group requested (freq: {:?})", options.freq));
        let interface =
            radio.wait_for_group_interface(&before, std::time::Duration::from_secs(20))?;
        match interface {
            Some((interface, address)) => {
                log(&format!("the group is up on {interface} ({address})"));
                log(&format!(
                    "a sink must join the group and connect to {address}:{}",
                    options.port
                ));
                group_address = Some(address);
            }
            None => {
                log("the group interface has no address yet - it gets one");
                log("when a peer joins, so the session will be served on all");
                log("addresses until then");
            }
        }
    }

    let server = match group_address {
        Some(address) => rtsp::Server::bind_to(IpAddr::V4(address), options.port),
        None => rtsp::Server::bind(options.port),
    }
    .map_err(|e| anyhow::anyhow!("binding port {} failed: {e}", options.port))?;
    log(&format!(
        "waiting for a sink on port {} for up to {}s",
        server.port(),
        options.wait
    ));

    // The sink has to be discoverable before it can connect, so advertise.
    if let Some(radio) = radio.as_mut() {
        let ie = crate::wfd::source_ie(rtsp::DEFAULT_CONTROL_PORT, 200);
        let _ = radio.set_wfd_ie(&ie);
    }

    let session = server.handle_once(log)?;
    log(&format!("handshake reached {:?}", session.stage));
    if let Some(caps) = &session.capabilities {
        log(&format!("sink capabilities: {caps:?}"));
        if caps.is_primary_sink() {
            log("the sink is a primary sink: it wants a single MPEG-TS stream");
        } else {
            log("the sink is a secondary sink: it wants audio only");
        }
    }
    let Some(rtp_port) = session.rtp_port else {
        anyhow::bail!("the sink did not give us an RTP port, so there is nowhere to stream to");
    };
    log(&format!(
        "streaming to {}:{} for {}s",
        session.peer.ip(),
        rtp_port,
        options.seconds
    ));

    let sink = media::Sink::Udp {
        host: session.peer.ip().to_string(),
        port: rtp_port,
    };
    let built = media::build(profile, &sink)?;

    let bus = built.pipeline.bus().expect("the pipeline has a bus");
    if let Err(e) = built.pipeline.set_state(gstreamer::State::Playing) {
        let mut detail = String::new();
        while let Some(message) = bus.timed_pop(gstreamer::ClockTime::ZERO) {
            if let gstreamer::MessageView::Error(err) = message.view() {
                detail = format!("{err}\n{}", err.debug().unwrap_or_default());
                break;
            }
        }
        built.pipeline.set_state(gstreamer::State::Null).ok();
        anyhow::bail!("the pipeline would not start: {e}\n{detail}");
    }

    let deadline = if options.seconds == 0 {
        None
    } else {
        Some(std::time::Instant::now() + std::time::Duration::from_secs(options.seconds as u64))
    };
    loop {
        if let Some(message) = bus.timed_pop(gstreamer::ClockTime::from_mseconds(200)) {
            if let gstreamer::MessageView::Error(err) = message.view() {
                built.pipeline.set_state(gstreamer::State::Null).ok();
                anyhow::bail!("the stream failed: {err}");
            }
        }
        if let Some(deadline) = deadline {
            if std::time::Instant::now() >= deadline {
                break;
            }
        }
    }
    built.pipeline.set_state(gstreamer::State::Null).ok();
    log("the stream finished");
    Ok(())
}
