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
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SignalMessage {
    /// SDP offer sent by a WebRTC client.
    #[serde(rename = "offer")]
    Offer {
        /// Session description payload.
        sdp: String,
    },
    /// SDP answer sent after accepting an offer.
    #[serde(rename = "answer")]
    Answer {
        /// Session description payload.
        sdp: String,
    },
    /// Request to close the signaling session.
    #[serde(rename = "close")]
    Close,
}

/// Upgrade an HTTP request to a signaling WebSocket.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_signaling(socket, state))
}

/// Process signaling messages on an upgraded WebSocket.
async fn handle_signaling(ws: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected");

    let (mut ws_tx, mut ws_rx) = ws.split();

    let mut ws_out_rx = None;
    let mut session_tx = None;

    loop {
        select! {
            inbound = ws_rx.next() => if handle_incoming(&mut ws_tx, &inbound, &state, &mut ws_out_rx, &mut session_tx).await.is_break() {
                break;
            },
            outbound = recv_outbound(&mut ws_out_rx) => if handle_outgoing(&mut ws_tx, outbound).await.is_break() {
                break;
            },
        }
    }
}

/// Handle a single message from the client WebSocket.
async fn handle_incoming(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    msg: &Option<Result<Message, axum::Error>>,
    state: &Arc<AppState>,
    ws_out_rx: &mut Option<mpsc::Receiver<SignalMessage>>,
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
            if ws_out_rx.is_some() {
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

            if let Err(err) = ws_tx.send(Message::Text(json.into())).await {
                error!(?err, "failed to send answer");
                return ControlFlow::Continue(());
            }

            // Channel for messages from the session thread to the WebSocket thread.
            let (ws_tx, ws_rx) = mpsc::channel(8);
            // Channel for messages from the WebSocket thread to the session thread.
            let (ssn_tx, ssn_rx) = mpsc::channel(8);

            *ws_out_rx = Some(ws_rx);
            *session_tx = Some(ssn_tx);

            tokio::spawn(session.build(ws_tx, ssn_rx, &state.room).run());
        },
        SignalMessage::Answer { .. } => {
            warn!("unexpected Answer from client (ignored)");
        },
        SignalMessage::Close => {
            info!("client sent close");
            return ControlFlow::Break(());
        },
    };

    ControlFlow::Continue(())
}

/// Await a message from the session channel, or wait indefinitely if the channel is not set up.
async fn recv_outbound(outbound_rx: &mut Option<mpsc::Receiver<SignalMessage>>) -> Option<SignalMessage> {
    match outbound_rx {
        Some(rx) => rx.recv().await,
        None => future::pending().await,
    }
}

/// Handle a single outgoing message to the client WebSocket.
async fn handle_outgoing(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    outbound: Option<SignalMessage>,
) -> ControlFlow<()> {
    match outbound {
        Some(msg) => {
            let json = serde_json::to_string(&msg).unwrap();
            if let Err(err) = ws_tx.send(Message::Text(json.into())).await {
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
