//! WebSocket signaling endpoint for exchanging SDP messages.

use std::{future, ops::ControlFlow, sync::Arc};

use axum::{
    extract::{
        State,
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt as _, StreamExt, stream::SplitSink};
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc};

use crate::{
    config::Config,
    session::{Room, SessionBuilder},
};

/// Shared application state for signaling handlers.
pub struct AppState {
    /// Server configuration.
    pub config: Arc<Config>,
    /// Room used to relay media between sessions.
    pub room: Room,
}

/// Message exchanged over the signaling WebSocket.
///
/// The same message shape is used in both directions, but the meaning depends on
/// who sent it. A client `Offer` starts a new session, while a server `Offer`
/// asks the existing client session to renegotiate. An `Answer` always completes
/// the most recent offer sent by the other side. `Close` is a graceful shutdown
/// request from either side.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SignalMessage {
    /// SDP offer that starts a call or requests renegotiation.
    Offer {
        /// Session description payload.
        sdp: String,
    },
    /// SDP answer for the most recent offer sent by the other side.
    Answer {
        /// Session description payload.
        sdp: String,
    },
    /// Request to close the signaling session.
    Close,
}

/// Upgrade an HTTP request to a signaling WebSocket.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_signaling(socket, state))
}

/// Process signaling messages on an upgraded WebSocket.
async fn handle_signaling(ws: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected");

    let (mut ws_sink, mut ws_stream) = ws.split();

    let mut ws_rx = None;
    let mut session_tx = None;

    loop {
        select! {
            inbound = ws_stream.next() => if handle_incoming(&mut ws_sink, &inbound, &state, &mut ws_rx, &mut session_tx).await.is_break() {
                break;
            },
            outbound = recv_outbound(&mut ws_rx) => if handle_outgoing(&mut ws_sink, outbound).await.is_break() {
                break;
            },
        }
    }
}

/// Handle a single message from the client WebSocket.
async fn handle_incoming(
    ws: &mut SplitSink<WebSocket, Message>,
    msg: &Option<Result<Message, axum::Error>>,
    state: &Arc<AppState>,
    ws_rx: &mut Option<mpsc::Receiver<SignalMessage>>,
    session_tx: &mut Option<mpsc::Sender<SignalMessage>>,
) -> ControlFlow<()> {
    let Some(msg) = msg else {
        info!("WebSocket closed ungracefully by client");
        return ControlFlow::Break(());
    };

    let msg = match msg {
        Ok(Message::Text(t)) => t,
        Ok(Message::Close(_)) => {
            info!("WebSocket closed by client");
            return ControlFlow::Break(());
        },
        Err(err) => {
            warn!(?err, "WebSocket read error");
            return ControlFlow::Break(());
        },
        _ => return ControlFlow::Continue(()),
    };

    let signal = match serde_json::from_str(msg) {
        Ok(s) => s,
        Err(err) => {
            warn!(?err, "invalid signaling message");
            return ControlFlow::Continue(());
        },
    };

    match signal {
        SignalMessage::Offer { sdp } => {
            if ws_rx.is_some() {
                warn!("received multiple offers in one session (ignored)");
                return ControlFlow::Continue(());
            }

            info!(bytes = sdp.len(), "received offer");

            let (session, answer_sdp) = match SessionBuilder::from_offer(&sdp, state).await {
                Ok(res) => res,
                Err(err) => {
                    error!(?err, "failed to create session from offer");
                    return ControlFlow::Continue(());
                },
            };

            let answer_msg = SignalMessage::Answer { sdp: answer_sdp };
            let json = serde_json::to_string(&answer_msg).unwrap();

            if let Err(err) = ws.send(Message::Text(json.into())).await {
                error!(?err, "failed to send answer");
                return ControlFlow::Continue(());
            }

            // Channel for messages from the session thread to the WebSocket thread.
            let (ws_tx, ws_rx_) = mpsc::channel(8);
            // Channel for messages from the WebSocket thread to the session thread.
            let (ssn_tx, ssn_rx) = mpsc::channel(8);

            *ws_rx = Some(ws_rx_);
            *session_tx = Some(ssn_tx);

            tokio::spawn(session.build(ws_tx, ssn_rx, &state.room).run());
        },
        SignalMessage::Answer { .. } => {
            if let Some(tx) = session_tx {
                if let Err(err) = tx.send(signal).await {
                    warn!(?err, "failed to forward answer to session");
                    return ControlFlow::Break(());
                }
            } else {
                warn!("received Answer before Offer (ignored)");
            }
        },
        SignalMessage::Close => {
            info!("client sent close");
            if let Some(tx) = session_tx {
                let _ = tx.send(signal).await;
            }
            return ControlFlow::Break(());
        },
    };

    ControlFlow::Continue(())
}

/// Await a message from the session channel, or wait indefinitely if the channel is not set up.
async fn recv_outbound(ws_rx: &mut Option<mpsc::Receiver<SignalMessage>>) -> Option<SignalMessage> {
    match ws_rx {
        Some(rx) => rx.recv().await,
        None => future::pending().await,
    }
}

/// Handle a single outgoing message to the client WebSocket.
async fn handle_outgoing(ws: &mut SplitSink<WebSocket, Message>, outbound: Option<SignalMessage>) -> ControlFlow<()> {
    match outbound {
        Some(msg) => {
            let json = serde_json::to_string(&msg).unwrap();
            if let Err(err) = ws.send(Message::Text(json.into())).await {
                error!(?err, "failed to send message to client");
                return ControlFlow::Break(());
            }
        },
        None => {
            info!("session channel closed");
            return ControlFlow::Break(());
        },
    };

    ControlFlow::Continue(())
}
