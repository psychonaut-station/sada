//! WebSocket signaling endpoint for exchanging SDP messages.

use std::sync::Arc;

use axum::{
    extract::{
        State,
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::{config::Config, session::Session};

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
pub async fn ws_handler(ws: WebSocketUpgrade, State(config): State<Arc<Config>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_signaling(socket, config))
}

/// Process signaling messages on an upgraded WebSocket.
async fn handle_signaling(mut ws: WebSocket, config: Arc<Config>) {
    info!("WebSocket client connected");

    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => {
                info!("WebSocket closed by client");
                break;
            },
            Err(err) => {
                warn!(?err, "WebSocket read error");
                break;
            },
            _ => continue,
        };

        let signal = match serde_json::from_str(&msg) {
            Ok(s) => s,
            Err(err) => {
                warn!(?err, "invalid signaling message");
                continue;
            },
        };

        match signal {
            SignalMessage::Offer { sdp } => {
                info!(bytes = sdp.len(), "received offer");

                let (session, answer_sdp) = match Session::from_offer(&sdp, &config.webrtc).await {
                    Ok(result) => result,
                    Err(err) => {
                        error!(?err, "failed to create session from offer");
                        continue;
                    },
                };

                let answer_msg = SignalMessage::Answer { sdp: answer_sdp };
                let json = serde_json::to_string(&answer_msg).unwrap();
                if let Err(err) = ws.send(Message::Text(json.into())).await {
                    error!(?err, "failed to send answer");
                    return;
                }

                tokio::spawn(session.run());
            },
            SignalMessage::Answer { .. } => {
                warn!("unexpected Answer from client (ignored)");
            },
            SignalMessage::Close => {
                info!("client sent close");
                break;
            },
        }
    }
}
