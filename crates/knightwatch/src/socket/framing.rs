use std::fmt::Debug;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::prelude::*;

pub async fn send_message<W, T>(writer: &mut W, message: &T) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: serde::Serialize + Debug,
{
    let data = serde_json::to_vec(message).map_err(|err| {
        Error::Other(format!(
            "Failed to Serialize message: {message:?} err: {err}"
        ))
    })?;
    let length = data.len() as u32;
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(Error::connection)?;
    writer.write_all(&data).await.map_err(Error::connection)?;
    writer.flush().await.map_err(Error::connection)
}

pub async fn receive_message<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncReadExt + Unpin,
    T: for<'de> serde::Deserialize<'de> + Debug,
{
    let mut length_buf = [0u8; 4];
    reader
        .read_exact(&mut length_buf)
        .await
        .map_err(Error::connection)?;
    let message_length = u32::from_be_bytes(length_buf) as usize;
    let mut message_buf = vec![0u8; message_length];
    reader
        .read_exact(&mut message_buf)
        .await
        .map_err(Error::connection)?;
    let message = serde_json::from_slice(&message_buf).map_err(|err| {
        Error::Other(format!(
            "Failed to Deserialize message_buf: {message_buf:?} err: {err}"
        ))
    })?;
    tracing::Span::current().record("message", tracing::field::debug(&message));
    debug!(?message, "close receive_message");
    Ok(message)
}

pub async fn close_connection(connection: &mut super::transport::Transport) -> Result<()> {
    connection.shutdown().await.map_err(Error::connection)
}
