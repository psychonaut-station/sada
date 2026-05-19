//! Main VC and signaling server binary.

#[macro_use]
extern crate tracing;

mod config;
mod control;
mod media;
mod session;
mod signaling;

use std::sync::Arc;

use anyhow::Result;
use axum::{Router, routing::get};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::{
    config::Config,
    session::Room,
    signaling::{AppState, ws_handler},
};

/// Server entry point.
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("sada_server=debug".parse()?))
        .init();

    let path = std::env::var("SADA_CONFIG").unwrap_or_else(|_| "config.toml".into());
    let config = Arc::new(Config::load(&path)?);
    let addr = config.server.listen;
    let control_socket = config.server.control_socket.clone();

    let state = Arc::new(AppState {
        config,
        room: Room::new(),
    });

    let app = Router::new().route("/ws", get(ws_handler)).with_state(state);

    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "http server listening");

    if let Some(path) = control_socket {
        tokio::spawn(async move {
            if let Err(err) = control::run(path).await {
                error!(?err, "control socket listener stopped");
            }
        });
    }

    axum::serve(listener, app).await?;

    Ok(())
}
