use std::net::{IpAddr, SocketAddr};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub webrtc: WebRtcConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub listen: SocketAddr,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct WebRtcConfig {
    /// IP address for WebRTC UDP sockets. If None, auto-detects.
    pub host_ip: Option<IpAddr>,
}

impl Default for WebRtcConfig {
    fn default() -> Self {
        Self { host_ip: None }
    }
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config from {path}"))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse config from {path}"))?;
        Ok(config)
    }
}
