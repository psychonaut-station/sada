//! Shared types and utilities for the Sada workspace.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;
#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// Result type used by the shared control protocol helpers.
pub type Result<T> = std::result::Result<T, Error>;

/// Reusable storage for one encoded or decoded control frame.
pub struct ControlFrameBuffer {
    /// Raw frame payload bytes.
    bytes: [u8; Self::MAX_CONTROL_FRAME_LEN],
}

impl ControlFrameBuffer {
    /// Largest accepted control frame payload.
    const MAX_CONTROL_FRAME_LEN: usize = 64 * 1024;

    /// Create an empty frame buffer.
    pub const fn new() -> Self {
        Self {
            bytes: [0; Self::MAX_CONTROL_FRAME_LEN],
        }
    }

    /// Encode `value` into this buffer and return the initialized payload slice.
    fn encode<T: Serialize>(&mut self, value: &T) -> Result<&mut [u8]> {
        Ok(postcard::to_slice(value, &mut self.bytes)?)
    }

    /// Decode a compact postcard control protocol payload.
    fn decode<'a: 'de, 'de, T: Deserialize<'de>>(&'a self, len: usize) -> Result<T> {
        Ok(postcard::from_bytes(&self.bytes[..len])?)
    }
}

impl Default for ControlFrameBuffer {
    fn default() -> Self { Self::new() }
}

/// Synchronous length-prefixed postcard control frame I/O.
pub trait ControlFrame {
    /// Read one frame from `reader`.
    ///
    /// Returns `Ok(None)` when the stream reaches EOF before a new frame begins.
    fn read<R, T>(&mut self, reader: &mut R) -> Result<Option<T>>
    where
        R: Read,
        T: for<'de> Deserialize<'de>;
    /// Encode and write one frame to `writer`.
    fn write<W, T>(&mut self, writer: &mut W, value: &T) -> Result<()>
    where
        W: Write,
        T: Serialize;
}

impl ControlFrame for ControlFrameBuffer {
    fn read<R, T>(&mut self, reader: &mut R) -> Result<Option<T>>
    where
        R: Read,
        T: for<'de> Deserialize<'de>,
    {
        let mut len = [0; size_of::<u32>()];
        match reader.read_exact(&mut len) {
            Ok(_) => {},
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(source) => return Err(Error::ReadFrameLength(source)),
        }

        let len = u32::from_le_bytes(len) as usize;
        if len > Self::MAX_CONTROL_FRAME_LEN {
            return Err(Error::ControlFrameTooLarge(len));
        }

        reader
            .read_exact(&mut self.bytes[..len])
            .map_err(Error::ReadFramePayload)?;

        Ok(Some(self.decode(len)?))
    }

    fn write<W, T>(&mut self, writer: &mut W, value: &T) -> Result<()>
    where
        W: Write,
        T: Serialize,
    {
        let payload = self.encode(value)?;
        let len = u32::try_from(payload.len()).map_err(Error::EncodeFrameTooLarge)?;

        writer.write_all(&len.to_le_bytes()).map_err(Error::WriteFrameLength)?;
        writer.write_all(payload).map_err(Error::WriteFramePayload)?;
        writer.flush().map_err(Error::Flush)?;

        Ok(())
    }
}

/// Async length-prefixed postcard control frame I/O.
#[cfg(feature = "async")]
#[allow(async_fn_in_trait)]
pub trait AsyncControlFrame {
    /// Read one frame from `reader`.
    ///
    /// Returns `Ok(None)` when the stream reaches EOF before a new frame begins.
    async fn read<R, T>(&mut self, reader: &mut R) -> Result<Option<T>>
    where
        R: AsyncRead + Unpin,
        T: for<'de> Deserialize<'de>;
    /// Encode and write one frame to `writer`.
    async fn write<W, T>(&mut self, writer: &mut W, value: &T) -> Result<()>
    where
        W: AsyncWrite + Unpin,
        T: Serialize;
}

#[cfg(feature = "async")]
impl AsyncControlFrame for ControlFrameBuffer {
    async fn read<R, T>(&mut self, reader: &mut R) -> Result<Option<T>>
    where
        R: AsyncRead + Unpin,
        T: for<'de> Deserialize<'de>,
    {
        let mut len = [0; size_of::<u32>()];
        match reader.read_exact(&mut len).await {
            Ok(_) => {},
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(source) => return Err(Error::ReadFrameLength(source)),
        }

        let len = u32::from_le_bytes(len) as usize;
        if len > Self::MAX_CONTROL_FRAME_LEN {
            return Err(Error::ControlFrameTooLarge(len));
        }

        reader
            .read_exact(&mut self.bytes[..len])
            .await
            .map_err(Error::ReadFramePayload)?;

        Ok(Some(self.decode(len)?))
    }

    async fn write<W, T>(&mut self, writer: &mut W, value: &T) -> Result<()>
    where
        W: AsyncWrite + Unpin,
        T: Serialize,
    {
        let payload = self.encode(value)?;
        let len = u32::try_from(payload.len()).map_err(Error::EncodeFrameTooLarge)?;

        writer
            .write_all(&len.to_le_bytes())
            .await
            .map_err(Error::WriteFrameLength)?;
        writer.write_all(payload).await.map_err(Error::WriteFramePayload)?;
        writer.flush().await.map_err(Error::Flush)?;

        Ok(())
    }
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

/// Errors that can happen while encoding or decoding control frames.
#[derive(Debug, Error)]
pub enum Error {
    /// The control frame length prefix could not be read.
    #[error("failed to read control frame length")]
    ReadFrameLength(#[source] io::Error),
    /// The incoming frame exceeded the maximum protocol size.
    #[error("control frame too large: {0} bytes")]
    ControlFrameTooLarge(usize),
    /// The control frame payload could not be read.
    #[error("failed to read control frame payload")]
    ReadFramePayload(#[source] io::Error),
    /// The response could not be encoded into the control protocol.
    #[error("failed to encode control frame")]
    EncodeFrame(#[source] postcard::Error),
    /// The encoded response was too large for the length prefix.
    #[error("control frame too large to encode")]
    EncodeFrameTooLarge(#[source] std::num::TryFromIntError),
    /// The control frame length prefix could not be written.
    #[error("failed to write control frame length")]
    WriteFrameLength(#[source] io::Error),
    /// The control frame payload could not be written.
    #[error("failed to write control frame payload")]
    WriteFramePayload(#[source] io::Error),
    /// The control frame could not be flushed to the socket.
    #[error("failed to flush control frame")]
    Flush(#[source] io::Error),
    /// The control frame payload could not be encoded/decoded.
    #[error(transparent)]
    Postcard(#[from] postcard::Error),
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
