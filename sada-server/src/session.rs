//! WebRTC session setup and event loop handling.

use std::{net::IpAddr, time::Instant};

use anyhow::{Context as _, Result};
use str0m::{
    Candidate,
    Event,
    IceConnectionState,
    Input,
    Output,
    Rtc,
    RtcConfig,
    change::SdpOffer,
    net::{Protocol, Receive},
};
use systemstat::{Platform, System};
use tokio::{net::UdpSocket, select};
use tracing::{debug, error, info, warn};

use crate::media;

/// Select the first usable non-loopback IPv4 address from the host.
pub fn select_host_address() -> IpAddr {
    let system = System::new();
    let networks = system.networks().unwrap();

    for net in networks.values() {
        for n in &net.addrs {
            if let systemstat::IpAddr::V4(v) = n.addr
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_broadcast()
            {
                return IpAddr::V4(v);
            }
        }
    }

    panic!("Found no usable network interface");
}

/// Next action requested by the WebRTC polling loop.
enum Loop {
    /// Wait until the given instant before polling again.
    Timeout(Instant),
    /// Continue polling immediately.
    Continue,
    /// Stop the session loop.
    Done,
}

/// A single WebRTC session backed by one UDP socket.
pub struct Session {
    /// WebRTC instance.
    rtc: Rtc,
    /// UDP socket used for ICE and RTP traffic.
    socket: UdpSocket,
}

impl Session {
    /// Create a session from a remote SDP offer and return the SDP answer.
    pub async fn from_offer(offer_sdp: &str, webrtc_config: &crate::config::WebRtcConfig) -> Result<(Self, String)> {
        let offer = SdpOffer::from_sdp_string(offer_sdp).context("failed to parse SDP offer")?;

        let mut rtc = RtcConfig::new().clear_codecs().enable_opus(true).build(Instant::now());

        let host_ip = webrtc_config.host_ip.unwrap_or_else(select_host_address);
        let socket = UdpSocket::bind((host_ip, 0))
            .await
            .context("failed to bind UDP socket")?;
        let addr = socket.local_addr().expect("a local socket address");
        let candidate = Candidate::host(addr, "udp").context("failed to create host candidate")?;
        rtc.add_local_candidate(candidate).unwrap();

        let answer = rtc.sdp_api().accept_offer(offer).context("failed to accept offer")?;

        let answer_sdp = answer.to_sdp_string();

        Ok((Self { rtc, socket }, answer_sdp))
    }

    /// Run the session event loop until the peer disconnects or an error occurs.
    pub async fn run(mut self) {
        info!("str0m run loop started");

        let mut audio = media::AudioSink::new();
        let mut buf = vec![0u8; 65535];
        loop {
            let timeout = match self.poll(&mut audio).await {
                Loop::Timeout(t) => t,
                Loop::Continue => continue,
                Loop::Done => return,
            };

            let sleep = timeout - Instant::now();

            if sleep.is_zero() {
                self.rtc.handle_input(Input::Timeout(Instant::now())).unwrap();
                continue;
            }

            buf.resize(65535, 0);

            select! {
                result = self.socket.recv_from(&mut buf) => {
                    match result {
                        Ok((n, source)) => {
                            buf.truncate(n);
                            let input = Input::Receive(
                                Instant::now(),
                                Receive {
                                    proto: Protocol::Udp,
                                    source,
                                    destination: self.socket.local_addr().unwrap(),
                                    contents: buf.as_slice().try_into().unwrap(),
                                },
                            );
                            if let Err(e) = self.rtc.handle_input(input) {
                                error!("str0m handle_input error: {e}");
                                return;
                            }
                        }
                        Err(e) => {
                            error!("UDP recv error: {e}");
                            return;
                        }
                    }
                }
                _ = tokio::time::sleep(sleep) => {
                    self.rtc.handle_input(Input::Timeout(Instant::now())).unwrap();
                }
            }
        }
    }

    /// Poll str0m once and perform any requested I/O.
    async fn poll(&mut self, audio: &mut media::AudioSink) -> Loop {
        match self.rtc.poll_output() {
            Ok(Output::Timeout(v)) => Loop::Timeout(v),
            Ok(Output::Transmit(v)) => {
                if let Err(e) = self.socket.send_to(&v.contents, v.destination).await {
                    warn!("UDP send error: {e}");
                }
                Loop::Continue
            },
            Ok(Output::Event(event)) => self.handle_event(event, audio),
            Err(e) => {
                error!("str0m poll_output error: {e}");
                Loop::Done
            },
        }
    }

    /// Handle a single str0m event.
    fn handle_event(&mut self, event: Event, audio: &mut media::AudioSink) -> Loop {
        match &event {
            Event::Connected => {
                info!("WebRTC peer connected");
            },
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                info!("ICE disconnected, ending run loop");
                return Loop::Done;
            },
            Event::IceConnectionStateChange(state) => {
                info!("ICE connection state: {state:?}");
            },
            Event::MediaAdded(ma) => {
                info!("media added: mid={:?} kind={:?}", ma.mid, ma.kind);
            },
            Event::MediaData(data) => {
                audio.handle_frame(data);
            },
            _ => {
                debug!("event: {event:?}");
            },
        }
        Loop::Continue
    }
}
