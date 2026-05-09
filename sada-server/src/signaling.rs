use std::sync::Arc;

use axum::extract::{State, ws::{Message, WebSocket}};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::session::Session;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SignalMessage {
    #[serde(rename = "offer")]
    Offer { sdp: String },
    #[serde(rename = "answer")]
    Answer { sdp: String },
    #[serde(rename = "close")]
    Close,
}

pub async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    State(config): State<Arc<Config>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_signaling(socket, config))
}

async fn handle_signaling(mut ws: WebSocket, config: Arc<Config>) {
    info!("WebSocket client connected");

    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => {
                info!("WebSocket closed by client");
                break;
            },
            Err(e) => {
                warn!("WebSocket read error: {e}");
                break;
            },
            _ => continue,
        };

        let signal: SignalMessage = match serde_json::from_str(&msg) {
            Ok(s) => s,
            Err(e) => {
                warn!("invalid signaling message: {e}");
                continue;
            },
        };

        match signal {
            SignalMessage::Offer { sdp } => {
                info!("received offer ({} bytes)", sdp.len());

                let (session, answer_sdp) = match Session::from_offer(&sdp, &config.webrtc).await {
                    Ok(result) => result,
                    Err(e) => {
                        error!("{e:#}");
                        continue;
                    },
                };

                let answer_msg = SignalMessage::Answer { sdp: answer_sdp };
                let json = serde_json::to_string(&answer_msg).unwrap();
                if let Err(e) = ws.send(Message::Text(json.into())).await {
                    error!("failed to send answer: {e}");
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
