use tokio::io::{AsyncReadExt, AsyncWriteExt};

use kw_utils::conv;
use kw_types::socket::SocketMessage;

use crate::prelude::*;

pub async fn send_message<W>(writer: &mut W, message: &SocketMessage) -> Result<()>
where
    W: AsyncWriteExt + Unpin + Send,
{
    let data = serde_json::to_vec(message).map_err(|err| {
        Error::Other(format!(
            "Failed to Serialize message: {message:?} err: {err}"
        ))
    })?;
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

pub async fn receive_message<R>(reader: &mut R) -> Result<SocketMessage>
where
    R: AsyncReadExt + Unpin + Send,
{
    let mut length_buf = [0u8; 4];
    reader
        .read_exact(&mut length_buf)
        .await
        .map_err(|err| Error::connection(&err))?;
    let message_length = conv::u32_to_usize(u32::from_be_bytes(length_buf));
    let mut message_buf = vec![0u8; message_length];
    reader
        .read_exact(&mut message_buf)
        .await
        .map_err(|err| Error::connection(&err))?;
    let message = serde_json::from_slice(&message_buf).map_err(|err| {
        Error::Other(format!(
            "Failed to Deserialize message_buf: {message_buf:?} err: {err}"
        ))
    })?;
    Ok(message)
}

pub async fn close_connection(connection: &mut super::transport::Transport) -> Result<()> {
    connection
        .shutdown()
        .await
        .map_err(|err| Error::connection(&err))
}
