//! Unix socket control channel for the BYOND bridge.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use sada_common::{ControlFrameBuffer, ControlRequest, ControlResponse, MAX_CONTROL_FRAME_LEN};
use thiserror::Error;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        UnixListener,
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::Semaphore,
};

/// Result type used by the control channel.
pub type Result<T> = std::result::Result<T, Error>;

/// Maximum number of simultaneous control socket clients.
const MAX_CONTROL_CLIENTS: usize = 4;

/// Run the Unix socket control listener.
pub async fn run(path: PathBuf) -> Result<()> {
    remove_stale_socket(&path).await?;

    let listener = UnixListener::bind(&path).map_err(|s| Error::BindSocket(path.clone(), s))?;
    info!(path = %path.display(), "control socket listening");

    let clients = Arc::new(Semaphore::new(MAX_CONTROL_CLIENTS));

    loop {
        let (stream, _) = listener.accept().await.map_err(Error::AcceptConnection)?;
        let permit = clients.clone().acquire_owned().await.map_err(Error::SemaphoreClosed)?;

        tokio::spawn(async move {
            let _permit = permit;
            if let Err(err) = handle_connection(stream).await {
                warn!(?err, "control socket connection ended with error");
            }
        });
    }
}

/// Remove an existing socket path left behind by a previous server process.
async fn remove_stale_socket(path: &Path) -> Result<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::RemoveStaleSocket(path.to_owned(), source)),
    }
}

/// Process length-prefixed postcard requests on one client connection.
async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let mut buffer = ControlFrameBuffer::new();

    while let Some(len) = read_frame(&mut reader, &mut buffer).await? {
        let response = match buffer.decode(len) {
            Ok(request) => handle_request(request),
            Err(err) => ControlResponse::Error {
                message: format!("invalid request: {err}"),
            },
        };

        write_frame(&mut writer, &mut buffer, &response).await?;
    }

    Ok(())
}

/// Read one length-prefixed postcard control frame.
async fn read_frame(reader: &mut OwnedReadHalf, buffer: &mut ControlFrameBuffer) -> Result<Option<usize>> {
    let mut len = [0; size_of::<u32>()];
    match reader.read_exact(&mut len).await {
        Ok(_) => {},
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(source) => return Err(Error::ReadFrameLength(source)),
    }

    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_CONTROL_FRAME_LEN {
        return Err(Error::ControlFrameTooLarge(len));
    }

    reader
        .read_exact(buffer.payload_mut(len))
        .await
        .map_err(Error::ReadFramePayload)?;
    Ok(Some(len))
}

/// Write one length-prefixed postcard control frame.
async fn write_frame<T: serde::Serialize>(
    writer: &mut OwnedWriteHalf, buffer: &mut ControlFrameBuffer, value: &T,
) -> Result<()> {
    let payload = buffer.encode(value).map_err(Error::EncodeFrame)?;
    let len = u32::try_from(payload.len()).map_err(Error::EncodeFrameTooLarge)?;

    writer
        .write_all(&len.to_le_bytes())
        .await
        .map_err(Error::WriteFrameLength)?;
    writer.write_all(payload).await.map_err(Error::WriteFramePayload)?;
    Ok(())
}

/// Apply one control request.
fn handle_request(request: ControlRequest) -> ControlResponse {
    match request {
        ControlRequest::Version => ControlResponse::Version {
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        ControlRequest::SetPtt { ckey, pressed } => {
            debug!(%ckey, pressed, "push-to-talk state updated");
            ControlResponse::Ok
        },
    }
}

/// Errors that can happen in the Unix socket control channel.
#[derive(Debug, Error)]
pub enum Error {
    /// A stale socket path from an earlier process could not be removed.
    #[error("failed to remove stale control socket at {0}")]
    RemoveStaleSocket(PathBuf, #[source] io::Error),
    /// The Unix listener could not bind to the configured socket path.
    #[error("failed to bind control socket at {0}")]
    BindSocket(PathBuf, #[source] io::Error),
    /// The listener failed to accept a client connection.
    #[error("failed to accept control socket connection")]
    AcceptConnection(#[source] io::Error),
    /// The client limiter semaphore was closed.
    #[error("control socket semaphore closed")]
    SemaphoreClosed(#[source] tokio::sync::AcquireError),
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
}
