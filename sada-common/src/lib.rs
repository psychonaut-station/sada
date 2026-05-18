//! Shared types and utilities for the Sada workspace.

use serde::{Deserialize, Serialize};

/// A control request sent from the BYOND bridge to the Sada server.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    /// Ask the server for its version.
    Version,
    /// Update push-to-talk state for a BYOND client.
    SetPtt {
        /// Client ckey.
        ckey: String,
        /// Whether push-to-talk is currently pressed.
        pressed: bool,
    },
}

/// A response returned by the Sada server control socket.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlResponse {
    /// Successful command completion.
    Ok,
    /// Server version response.
    Version {
        /// Server crate version.
        version: String,
    },
    /// Command failed.
    Error {
        /// Human-readable error message.
        message: String,
    },
}
