//! Unix socket control channel for the BYOND bridge.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result};
use sada_common::{ControlRequest, ControlResponse};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::Semaphore,
};

/// Maximum number of simultaneous control socket clients.
const MAX_CONTROL_CLIENTS: usize = 4;

/// Run the Unix socket control listener.
pub async fn run(path: PathBuf) -> Result<()> {
    remove_stale_socket(&path).await?;

    let listener =
        UnixListener::bind(&path).with_context(|| format!("failed to bind control socket at {}", path.display()))?;
    info!(path = %path.display(), "control socket listening");

    let clients = Arc::new(Semaphore::new(MAX_CONTROL_CLIENTS));

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("failed to accept control socket connection")?;
        let permit = clients
            .clone()
            .acquire_owned()
            .await
            .context("control socket semaphore closed")?;

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
        Err(err) => Err(err).with_context(|| format!("failed to remove stale control socket at {}", path.display())),
    }
}

/// Process newline-delimited JSON requests on one client connection.
async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await.context("failed to read control request")? {
        let response = match serde_json::from_str::<ControlRequest>(&line) {
            Ok(request) => handle_request(request),
            Err(err) => ControlResponse::Error {
                message: format!("invalid request: {err}"),
            },
        };

        let encoded = serde_json::to_vec(&response).context("failed to encode control response")?;
        writer
            .write_all(&encoded)
            .await
            .context("failed to write control response")?;
        writer
            .write_all(b"\n")
            .await
            .context("failed to write control response newline")?;
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
