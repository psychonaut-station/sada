//! WebRTC session setup and event loop handling.

use std::{
    io,
    net::IpAddr,
    sync::{
        Arc,
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use str0m::{
    Candidate,
    Event,
    IceConnectionState,
    Input,
    Output,
    Rtc,
    RtcConfig,
    change::SdpOffer,
    error::{IceError, RtcError, SdpError},
    format::PayloadParams,
    media::{MediaData, MediaTime, Mid},
    net::{Protocol, Receive},
};
use systemstat::{Platform, System};
use thiserror::Error;
use tokio::{
    net::UdpSocket,
    select,
    sync::{
        broadcast::{self, Receiver, Sender, error::RecvError},
        mpsc,
    },
};

use crate::{
    media::AudioSink,
    signaling::{AppState, SignalMessage},
};

/// Result type used by session setup.
pub type Result<T> = std::result::Result<T, Error>;

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
            id: SessionId(self.next_id.fetch_add(1, Ordering::AcqRel)),
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
    let networks = system.networks().map_err(Error::ListNetworkInterfaces)?;

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

    Err(Error::NoUsableNetworkInterface)
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
    /// Debug audio sink.
    sink: AudioSink,
    /// Sender for outgoing messages to the client WebSocket.
    _ws_tx: mpsc::Sender<SignalMessage>,
}

impl Session {
    /// Run the session event loop until the peer disconnects or an error occurs.
    pub async fn run(mut self) {
        info!("str0m run loop started");

        let mut buf = vec![0; 65535];

        loop {
            let timeout = match self.poll().await {
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
    async fn poll(&mut self) -> Loop {
        match self.rtc.poll_output() {
            Ok(Output::Timeout(v)) => Loop::Timeout(v),
            Ok(Output::Transmit(v)) => {
                if let Err(err) = self.socket.send_to(&v.contents, v.destination).await {
                    warn!(?err, "UDP send error");
                }
                Loop::Continue
            },
            Ok(Output::Event(event)) => self.handle_event(event),
            Err(err) => {
                error!(?err, "str0m poll_output error");
                Loop::Done
            },
        }
    }

    /// Handle a single str0m event.
    fn handle_event(&mut self, event: Event) -> Loop {
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
                self.sink.handle_frame(&data);
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

/// Partially constructed session created while accepting a WebRTC offer.
pub struct SessionBuilder {
    /// WebRTC instance.
    rtc: Rtc,
    /// UDP socket used for ICE and RTP traffic.
    socket: UdpSocket,
}

impl SessionBuilder {
    /// Create a session builder from a remote SDP offer and return the SDP answer.
    pub async fn from_offer(offer_sdp: &str, state: &Arc<AppState>) -> Result<(Self, String)> {
        let offer = SdpOffer::from_sdp_string(offer_sdp).map_err(Error::ParseOffer)?;

        let mut rtc = RtcConfig::new().clear_codecs().enable_opus(true).build(Instant::now());

        let ip = match state.config.webrtc.host_ip {
            Some(ip) => ip,
            None => auto_host_address()?,
        };

        let socket = UdpSocket::bind((ip, 0)).await.map_err(|s| Error::BindSocket(ip, s))?;
        let addr = socket.local_addr().map_err(Error::LocalSocketAddress)?;
        let candidate = Candidate::host(addr, "udp").map_err(Error::CreateHostCandidate)?;
        rtc.add_local_candidate(candidate).ok_or(Error::AddLocalCandidate)?;

        let answer = rtc.sdp_api().accept_offer(offer).map_err(Error::AcceptOffer)?;

        let answer_sdp = answer.to_sdp_string();

        Ok((Self { rtc, socket }, answer_sdp))
    }

    /// Join the room and finish the session with an outgoing signaling sender.
    pub fn build(self, ws_tx: mpsc::Sender<SignalMessage>, room: &Room) -> Session {
        let session_room = room.join();
        let id = session_room.id.0;

        Session {
            rtc: self.rtc,
            socket: self.socket,
            room: session_room,
            sink: AudioSink::new(id),
            _ws_tx: ws_tx,
        }
    }
}

/// Errors that can happen while creating a WebRTC session.
#[derive(Debug, Error)]
pub enum Error {
    /// The remote SDP offer could not be parsed.
    #[error("failed to parse SDP offer")]
    ParseOffer(#[source] SdpError),
    /// Local network interfaces could not be listed.
    #[error("failed to list network interfaces")]
    ListNetworkInterfaces(#[source] io::Error),
    /// No usable non-loopback IPv4 address was available.
    #[error("found no usable network interface")]
    NoUsableNetworkInterface,
    /// The UDP socket for WebRTC traffic could not be bound.
    #[error("failed to bind UDP socket to {0}")]
    BindSocket(IpAddr, #[source] io::Error),
    /// The bound UDP socket's local address could not be read.
    #[error("failed to get local socket address")]
    LocalSocketAddress(#[source] io::Error),
    /// A host ICE candidate could not be created.
    #[error("failed to create host candidate")]
    CreateHostCandidate(#[source] IceError),
    /// The host ICE candidate was rejected by str0m.
    #[error("failed to add local candidate")]
    AddLocalCandidate,
    /// str0m rejected the remote offer.
    #[error("failed to accept offer")]
    AcceptOffer(#[source] RtcError),
}
