//! Main VC and signaling server binary.

#[macro_use]
extern crate tracing;

mod config;
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

    let config = Arc::new(Config::load("config.toml")?);
    let addr = config.server.listen;

    let state = Arc::new(AppState {
        config,
        room: Room::new(),
    });

    let app = Router::new().route("/ws", get(ws_handler)).with_state(state);

    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "http server listening");

    axum::serve(listener, app).await?;

    Ok(())
}
