//! Unix socket control client used by exported functions.

use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

use sada_common::{ControlRequest, ControlResponse};

thread_local! {
    /// Connected control socket state.
    static CONTROL: RefCell<Option<UnixStream>> = const { RefCell::new(None) };
}

/// Connect to the server control socket and return the server version.
pub fn init(path: &str) -> ControlResponse {
    match UnixStream::connect(path) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            CONTROL.set(Some(stream));
            request(ControlRequest::Version)
        },
        Err(err) => ControlResponse::Error {
            message: format!("failed to connect to {path}: {err}"),
        },
    }
}

/// Send one request and read one response over the persistent socket.
fn request(request: ControlRequest) -> ControlResponse {
    CONTROL.with_borrow_mut(|control| {
        let Some(stream) = control.as_mut() else {
            return ControlResponse::Error {
                message: "sada client is not initialized".to_owned(),
            };
        };

        match request_with_stream(stream, &request) {
            Ok(response) => response,
            Err(err) => {
                *control = None;
                ControlResponse::Error {
                    message: err.to_string(),
                }
            },
        }
    })
}

/// Perform the wire-format request/response exchange.
fn request_with_stream(
    stream: &mut UnixStream, request: &ControlRequest,
) -> Result<ControlResponse, Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *stream, request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line)?;

    Ok(serde_json::from_str(&line)?)
}

/// Send a push-to-talk state update.
pub fn set_ptt(ckey: String, pressed: bool) -> ControlResponse { request(ControlRequest::SetPtt { ckey, pressed }) }
