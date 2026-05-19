//! Unix socket control client used by exported functions.

use std::{
    cell::RefCell,
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

use sada_common::{ControlFrameBuffer, ControlRequest, ControlResponse, MAX_CONTROL_FRAME_LEN};

thread_local! {
    /// Connected control socket state.
    static CONTROL: RefCell<Option<ControlState>> = const { RefCell::new(None) };
}

/// Persistent control connection state.
struct ControlState {
    /// Connected Unix socket.
    stream: UnixStream,
    /// Reused frame payload buffer.
    buffer: ControlFrameBuffer,
}

impl ControlState {
    /// Create control state for `stream`.
    fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            buffer: ControlFrameBuffer::new(),
        }
    }

    /// Write one length-prefixed postcard control frame.
    fn write_frame<T: serde::Serialize>(&mut self, value: &T) -> Result<(), Box<dyn std::error::Error>> {
        let payload = self.buffer.encode(value)?;

        self.stream.write_all(&u32::try_from(payload.len())?.to_le_bytes())?;
        self.stream.write_all(payload)?;

        self.stream.flush()?;

        Ok(())
    }

    /// Read one length-prefixed postcard control frame.
    fn read_frame(&mut self) -> Result<ControlResponse, Box<dyn std::error::Error>> {
        let mut len = [0; size_of::<u32>()];
        self.stream.read_exact(&mut len)?;

        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_CONTROL_FRAME_LEN {
            return Err(format!("control frame too large: {len} bytes").into());
        }

        self.stream.read_exact(self.buffer.payload_mut(len))?;

        Ok(self.buffer.decode(len)?)
    }

    /// Perform the wire-format request/response exchange.
    fn request(&mut self, request: &ControlRequest) -> Result<ControlResponse, Box<dyn std::error::Error>> {
        self.write_frame(request)?;
        self.read_frame()
    }
}

/// Send one request and read one response over the persistent socket.
fn request(request: ControlRequest) -> ControlResponse {
    CONTROL.with_borrow_mut(|slot| {
        let Some(control) = slot.as_mut() else {
            return ControlResponse::Error {
                message: "sada client is not initialized".to_owned(),
            };
        };

        match control.request(&request) {
            Ok(response) => response,
            Err(err) => {
                *slot = None;
                ControlResponse::Error {
                    message: err.to_string(),
                }
            },
        }
    })
}

/// Connect to the server control socket and return the server version.
pub fn init(path: &str) -> ControlResponse {
    match UnixStream::connect(path) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            CONTROL.set(Some(ControlState::new(stream)));
            request(ControlRequest::Version)
        },
        Err(err) => ControlResponse::Error {
            message: format!("failed to connect to {path}: {err}"),
        },
    }
}

/// Send a push-to-talk state update.
pub fn set_ptt(ckey: String, pressed: bool) -> ControlResponse { request(ControlRequest::SetPtt { ckey, pressed }) }
