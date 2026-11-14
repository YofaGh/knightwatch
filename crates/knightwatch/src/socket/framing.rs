use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use kw_utils::conv;

use crate::prelude::*;

/// Largest accepted frame body. Checked before the body is allocated.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

pub async fn send_message<W, T>(writer: &mut W, message: &T) -> Result<()>
where
    W: AsyncWriteExt + Unpin + Send,
    T: Serialize + Sync,
{
    let data = serde_json::to_vec(message)
        .map_err(|err| Error::Other(format!("Failed to serialize message: {err}")))?;
    let length = conv::usize_to_u32_saturating(data.len());
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|err| Error::connection(&err))?;
    writer
        .write_all(&data)
        .await
        .map_err(|err| Error::connection(&err))?;
    writer.flush().await.map_err(|err| Error::connection(&err))
}

pub async fn receive_message<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncReadExt + Unpin + Send,
    T: DeserializeOwned,
{
    let mut length_buf = [0u8; 4];
    reader
        .read_exact(&mut length_buf)
        .await
        .map_err(|err| Error::connection(&err))?;
    let message_length = conv::u32_to_usize(u32::from_be_bytes(length_buf));
    if message_length > MAX_FRAME_LEN {
        return Err(Error::Socket(format!(
            "frame too large: {message_length} bytes"
        )));
    }
    let mut message_buf = vec![0u8; message_length];
    reader
        .read_exact(&mut message_buf)
        .await
        .map_err(|err| Error::connection(&err))?;
    serde_json::from_slice(&message_buf).map_err(|err| {
        Error::Other(format!(
            "Failed to Deserialize message: {:?} at line {} column {}",
            err.classify(),
            err.line(),
            err.column()
        ))
    })
}

pub async fn close_connection(connection: &mut super::transport::Transport) -> Result<()> {
    connection
        .shutdown()
        .await
        .map_err(|err| Error::connection(&err))
}
