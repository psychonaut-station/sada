use std::sync::Arc;

use anyhow::Result;
use axum::{Router, routing::get};
use tracing::info;

use crate::signaling::ws_handler;

mod config;
mod media;
mod session;
mod signaling;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("sada_server=debug".parse()?))
        .init();

    let config = Arc::new(config::Config::load("config.toml")?);
    let addr = config.server.listen;

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(config);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on {addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
