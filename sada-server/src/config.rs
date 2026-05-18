//! Configuration loading and schema definitions.

use std::{
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use anyhow::Context;
use serde::Deserialize;

/// Complete server configuration loaded from TOML.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Server listener settings.
    pub server: ServerConfig,
    /// WebRTC transport settings.
    pub webrtc: WebRtcConfig,
}

/// HTTP server configuration.
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// Socket address the server listens on.
    pub listen: SocketAddr,
    /// Unix socket path used by the BYOND bridge control channel.
    pub control_socket: Option<PathBuf>,
}

/// WebRTC transport configuration.
#[derive(Default, Debug, Deserialize)]
#[serde(default)]
pub struct WebRtcConfig {
    /// IP address for WebRTC UDP sockets. If None, auto-detects.
    pub host_ip: Option<IpAddr>,
}

impl Config {
    /// Load configuration from a TOML file at `path`.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path).with_context(|| format!("failed to read config from {path}"))?;
        let config = toml::from_str(&content).with_context(|| format!("failed to parse config from {path}"))?;
        Ok(config)
    }
}
