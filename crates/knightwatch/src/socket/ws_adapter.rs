use futures::Sink;
use std::{
    collections::VecDeque,
    io::{Error, ErrorKind, Result},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

#[derive(Debug)]
pub struct WebSocketAdapter<S> {
    inner: WebSocketStream<S>,
    read_buffer: VecDeque<u8>,
}

impl<S> WebSocketAdapter<S> {
    pub const fn new(ws: WebSocketStream<S>) -> Self {
        Self {
            inner: ws,
            read_buffer: VecDeque::new(),
        }
    }
}

impl<S> AsyncRead for WebSocketAdapter<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut tokio::io::ReadBuf,
    ) -> Poll<Result<()>> {
        if !self.read_buffer.is_empty() {
            let to_read = buf.remaining().min(self.read_buffer.len());
            for _ in 0..to_read {
                if let Some(byte) = self.read_buffer.pop_front() {
                    buf.put_slice(&[byte]);
                }
            }
            return Poll::Ready(Ok(()));
        }
        match futures::Stream::poll_next(Pin::new(&mut self.inner), cx) {
            Poll::Ready(Some(Ok(Message::Binary(data)))) => {
                let to_read = buf.remaining().min(data.len());
                let (head, tail) = data.split_at(to_read);
                buf.put_slice(head);
                self.read_buffer.extend(tail.iter().copied());
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(Ok(Message::Close(_)))) => Poll::Ready(Err(Error::new(
                ErrorKind::ConnectionAborted,
                "WebSocket closed",
            ))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(Error::other(e))),
            Poll::Ready(None) => Poll::Ready(Err(Error::new(
                ErrorKind::UnexpectedEof,
                "WebSocket stream ended",
            ))),
            Poll::Pending | Poll::Ready(_) => Poll::Pending,
        }
    }
}

impl<S> AsyncWrite for WebSocketAdapter<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
        let message = Message::Binary(buf.to_vec().into());
        match Pin::new(&mut self.inner).poll_ready(cx) {
            Poll::Ready(Ok(())) => match Pin::new(&mut self.inner).start_send(message) {
                Ok(()) => Poll::Ready(Ok(buf.len())),
                Err(e) => Poll::Ready(Err(Error::other(e))),
            },
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        match Pin::new(&mut self.inner).poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        match Pin::new(&mut self.inner).poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}