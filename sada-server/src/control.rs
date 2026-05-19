//! Unix socket control channel for the BYOND bridge.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result, bail};
use sada_common::{ControlFrameBuffer, ControlRequest, ControlResponse, MAX_CONTROL_FRAME_LEN};
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

/// Process length-prefixed postcard requests on one client connection.
async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let mut buffer = ControlFrameBuffer::new();

    while let Some(len) = read_frame(&mut reader, &mut buffer)
        .await
        .context("failed to read control request")?
    {
        let response = match buffer.decode(len) {
            Ok(request) => handle_request(request),
            Err(err) => ControlResponse::Error {
                message: format!("invalid request: {err}"),
            },
        };

        write_frame(&mut writer, &mut buffer, &response)
            .await
            .context("failed to write control response")?;
    }

    Ok(())
}

/// Read one length-prefixed postcard control frame.
async fn read_frame(reader: &mut OwnedReadHalf, buffer: &mut ControlFrameBuffer) -> Result<Option<usize>> {
    let mut len = [0; size_of::<u32>()];
    match reader.read_exact(&mut len).await {
        Ok(_) => {},
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err).context("failed to read control frame length"),
    }

    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_CONTROL_FRAME_LEN {
        bail!("control frame too large: {len} bytes");
    }

    reader
        .read_exact(buffer.payload_mut(len))
        .await
        .context("failed to read control frame payload")?;
    Ok(Some(len))
}

/// Write one length-prefixed postcard control frame.
async fn write_frame<T: serde::Serialize>(
    writer: &mut OwnedWriteHalf, buffer: &mut ControlFrameBuffer, value: &T,
) -> Result<()> {
    let payload = buffer.encode(value).context("failed to encode control frame")?;
    let len = u32::try_from(payload.len()).context("control frame too large to encode")?;

    writer
        .write_all(&len.to_le_bytes())
        .await
        .context("failed to write control frame length")?;
    writer
        .write_all(payload)
        .await
        .context("failed to write control frame payload")?;
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
