//! Configuration loading and schema definitions.

use std::{
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use serde::Deserialize;
use thiserror::Error;

/// Result type used by configuration loading.
pub type Result<T> = std::result::Result<T, Error>;

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
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|s| Error::Read(path.to_owned(), s))?;
        let config = toml::from_str(&content).map_err(|s| Error::Parse(path.to_owned(), s))?;
        Ok(config)
    }
}

/// Errors that can happen while loading server configuration.
#[derive(Debug, Error)]
pub enum Error {
    /// The configuration file could not be read.
    #[error("failed to read config from {0}")]
    Read(String, #[source] std::io::Error),
    /// The configuration file was not valid TOML for the expected schema.
    #[error("failed to parse config from {0}")]
    Parse(String, #[source] toml::de::Error),
}
