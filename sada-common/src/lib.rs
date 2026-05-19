//! Shared types and utilities for the Sada workspace.

use serde::{Deserialize, Serialize};

/// Largest accepted control frame payload.
pub const MAX_CONTROL_FRAME_LEN: usize = 64 * 1024;

/// Reusable storage for one encoded or decoded control frame.
pub struct ControlFrameBuffer {
    /// Raw frame payload bytes.
    bytes: [u8; MAX_CONTROL_FRAME_LEN],
}

impl ControlFrameBuffer {
    /// Create an empty frame buffer.
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_CONTROL_FRAME_LEN],
        }
    }

    /// Encode `value` into this buffer and return the initialized payload slice.
    pub fn encode<T: Serialize>(&mut self, value: &T) -> Result<&mut [u8], postcard::Error> {
        postcard::to_slice(value, &mut self.bytes)
    }

    /// Decode a compact postcard control protocol payload.
    pub fn decode<'a: 'de, 'de, T: Deserialize<'de>>(&'a self, len: usize) -> Result<T, postcard::Error> {
        postcard::from_bytes(&self.bytes[..len])
    }

    /// Return the initialized payload bytes.
    pub fn payload(&self, len: usize) -> &[u8] { &self.bytes[..len] }

    /// Return mutable storage for reading a payload.
    pub fn payload_mut(&mut self, len: usize) -> &mut [u8] { &mut self.bytes[..len] }
}

impl Default for ControlFrameBuffer {
    fn default() -> Self { Self::new() }
}

/// A control request sent from the BYOND bridge to the Sada server.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

#[cfg(test)]
mod tests {
    use super::{ControlFrameBuffer, ControlRequest, ControlResponse};

    #[test]
    fn control_request_roundtrips() {
        let request = ControlRequest::SetPtt {
            ckey: "sefa".to_owned(),
            pressed: true,
        };
        let mut buffer = ControlFrameBuffer::new();
        let length = buffer.encode(&request).unwrap().len();
        let decoded: ControlRequest = buffer.decode(length).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn control_response_roundtrips() {
        let response = ControlResponse::Version {
            version: "0.1.0".to_owned(),
        };
        let mut buffer = ControlFrameBuffer::new();
        let length = buffer.encode(&response).unwrap().len();
        let decoded: ControlResponse = buffer.decode(length).unwrap();
        assert_eq!(decoded, response);
    }
}
