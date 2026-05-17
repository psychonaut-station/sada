//! WebRTC session setup and event loop handling.

use std::{
    net::IpAddr,
    sync::{
        Arc,
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use anyhow::{Context as _, Result, bail};
use str0m::{
    Candidate,
    Event,
    IceConnectionState,
    Input,
    Output,
    Rtc,
    RtcConfig,
    change::SdpOffer,
    format::PayloadParams,
    media::{MediaData, MediaTime, Mid},
    net::{Protocol, Receive},
};
use systemstat::{Platform, System};
use tokio::{
    net::UdpSocket,
    select,
    sync::broadcast::{self, Receiver, Sender, error::RecvError},
};

use crate::{config::WebRtcConfig, media::AudioSink};

/// Identifier assigned to a connected WebRTC session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionId(u64);

/// Audio frame forwarded between sessions.
#[derive(Clone, Debug)]
struct ForwardedAudio {
    /// Session that produced the frame.
    origin: SessionId,
    /// Media identifier to write the frame to.
    mid: Mid,
    /// Codec payload parameters from the incoming frame.
    params: PayloadParams,
    /// RTP media timestamp.
    time: MediaTime,
    /// Wallclock time associated with the received frame.
    network_time: Instant,
    /// Audio frame data.
    data: Vec<u8>,
    /// Whether this frame starts a talk spurt.
    start_of_talkspurt: bool,
}

impl ForwardedAudio {
    /// Build a forwarded audio frame from str0m media data.
    fn new(origin: SessionId, data: MediaData) -> Self {
        Self {
            origin,
            mid: data.mid,
            params: data.params,
            time: data.time,
            network_time: data.network_time,
            data: data.data,
            start_of_talkspurt: data.audio_start_of_talk_spurt,
        }
    }
}

/// Shared audio room used by all connected sessions.
#[derive(Clone)]
pub struct Room {
    /// Counter used to allocate session IDs.
    next_id: Arc<AtomicU64>,
    /// Broadcast channel carrying encoded audio frames.
    audio_tx: Sender<ForwardedAudio>,
}

impl Room {
    /// Create an empty room.
    pub fn new() -> Self {
        let (audio_tx, _) = broadcast::channel(256);

        Self {
            next_id: Arc::new(AtomicU64::new(1)),
            audio_tx,
        }
    }

    /// Register a new session and return its room handle.
    fn join(&self) -> SessionRoom {
        SessionRoom {
            id: SessionId(self.next_id.fetch_add(1, Ordering::Relaxed)),
            audio_tx: self.audio_tx.clone(),
            audio_rx: self.audio_tx.subscribe(),
        }
    }
}

/// Per-session view of the shared room.
struct SessionRoom {
    /// Current session ID.
    id: SessionId,
    /// Sender used to publish local audio.
    audio_tx: Sender<ForwardedAudio>,
    /// Receiver used to consume audio from other sessions.
    audio_rx: Receiver<ForwardedAudio>,
}

/// Select the first usable non-loopback IPv4 address from the host.
fn select_host_address() -> Result<IpAddr> {
    let system = System::new();
    let networks = system.networks().unwrap();

    for net in networks.values() {
        for n in &net.addrs {
            if let systemstat::IpAddr::V4(v) = n.addr
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_broadcast()
            {
                return Ok(IpAddr::V4(v));
            }
        }
    }

    bail!("found no usable network interface");
}

/// Select and cache the automatically detected host IP address.
fn auto_host_address() -> Result<IpAddr> {
    static ADDR: OnceLock<IpAddr> = OnceLock::new();

    match ADDR.get() {
        Some(addr) => Ok(*addr),
        None => {
            let addr = select_host_address()?;
            match ADDR.set(addr) {
                Ok(()) => Ok(addr),
                Err(addr) => Ok(addr),
            }
        },
    }
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
    /// Room state used to forward audio between peers.
    room: SessionRoom,
}

impl Session {
    /// Create a session from a remote SDP offer and return the SDP answer.
    pub async fn from_offer(offer_sdp: &str, webrtc_config: &WebRtcConfig, room: &Room) -> Result<(Self, String)> {
        let offer = SdpOffer::from_sdp_string(offer_sdp).context("failed to parse SDP offer")?;

        let mut rtc = RtcConfig::new().clear_codecs().enable_opus(true).build(Instant::now());

        let host_ip = match webrtc_config.host_ip {
            Some(ip) => ip,
            None => auto_host_address().context("failed to auto-detect host IP address")?,
        };
        let socket = UdpSocket::bind((host_ip, 0))
            .await
            .context("failed to bind UDP socket")?;
        let addr = socket.local_addr().expect("failed to get local socket address");
        let candidate = Candidate::host(addr, "udp").context("failed to create host candidate")?;
        rtc.add_local_candidate(candidate)
            .context("failed to add local candidate")?;

        let answer = rtc.sdp_api().accept_offer(offer).context("failed to accept offer")?;

        let answer_sdp = answer.to_sdp_string();

        Ok((
            Self {
                rtc,
                socket,
                room: room.join(),
            },
            answer_sdp,
        ))
    }

    /// Run the session event loop until the peer disconnects or an error occurs.
    pub async fn run(mut self) {
        info!("str0m run loop started");

        let mut audio = AudioSink::new();
        let mut buf = vec![0; 65535];

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

            select! {
                result = self.room.audio_rx.recv() => {
                    match result {
                        Ok(frame) => {
                            self.handle_forwarded_audio(frame);
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            warn!(skipped, "audio relay receiver lagged");
                        }
                        Err(RecvError::Closed) => {
                            return;
                        }
                    }
                }
                result = self.socket.recv_from(&mut buf) => {
                    match result {
                        Ok((n, source)) => {
                            let input = Input::Receive(
                                Instant::now(),
                                Receive {
                                    proto: Protocol::Udp,
                                    source,
                                    destination: self.socket.local_addr().unwrap(),
                                    contents: buf[..n].try_into().unwrap(),
                                },
                            );
                            if let Err(err) = self.rtc.handle_input(input) {
                                error!(?err, "str0m handle_input error");
                                return;
                            }
                        }
                        Err(err) => {
                            error!(?err, "UDP recv error");
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
    async fn poll(&mut self, audio: &mut AudioSink) -> Loop {
        match self.rtc.poll_output() {
            Ok(Output::Timeout(v)) => Loop::Timeout(v),
            Ok(Output::Transmit(v)) => {
                if let Err(err) = self.socket.send_to(&v.contents, v.destination).await {
                    warn!(?err, "UDP send error");
                }
                Loop::Continue
            },
            Ok(Output::Event(event)) => self.handle_event(event, audio),
            Err(err) => {
                error!(?err, "str0m poll_output error");
                Loop::Done
            },
        }
    }

    /// Handle a single str0m event.
    fn handle_event(&mut self, event: Event, audio: &mut AudioSink) -> Loop {
        match event {
            Event::Connected => {
                info!("WebRTC peer connected");
            },
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                info!("ICE disconnected, ending run loop");
                return Loop::Done;
            },
            Event::IceConnectionStateChange(state) => {
                info!(?state, "ICE connection state change");
            },
            Event::MediaAdded(ma) => {
                info!(mid = ?ma.mid, kind = ?ma.kind, "media added");
            },
            Event::MediaData(data) => {
                audio.handle_frame(&data);
                if let Err(err) = self.room.audio_tx.send(ForwardedAudio::new(self.room.id, data)) {
                    warn!(?err, "audio relay has no receivers");
                }
            },
            _ => debug!(?event, "unhandled event"),
        }
        Loop::Continue
    }

    /// Write audio received from another session to this WebRTC peer.
    fn handle_forwarded_audio(&mut self, frame: ForwardedAudio) {
        if frame.origin == self.room.id {
            return;
        }

        let Some(writer) = self.rtc.writer(frame.mid) else {
            warn!(mid = ?frame.mid, "no writer for forwarded audio");
            return;
        };

        let Some(pt) = writer.match_params(frame.params) else {
            warn!(mid = ?frame.mid, "no matching payload type for forwarded audio");
            return;
        };

        if let Err(err) =
            writer
                .start_of_talkspurt(frame.start_of_talkspurt)
                .write(pt, frame.network_time, frame.time, frame.data)
        {
            warn!(?err, "failed to write forwarded audio");
        }
    }
}
