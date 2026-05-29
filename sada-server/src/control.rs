//! Unix socket control channel for the BYOND bridge.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use sada_common::{AsyncControlFrame as _, ControlFrameBuffer, ControlRequest, ControlResponse};
use thiserror::Error;
use tokio::{
    fs,
    net::{UnixListener, UnixStream},
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

    loop {
        let response = match buffer.read(&mut reader).await {
            Ok(Some(req)) => handle_request(req),
            Ok(None) => break, // eof
            Err(err) => ControlResponse::Error {
                message: format!("invalid request: {err}"),
            },
        };
        buffer.write(&mut writer, &response).await?;
    }

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
    /// Shared control frame protocol error.
    #[error(transparent)]
    Common(#[from] sada_common::Error),
}
