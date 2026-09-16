use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, Result},
    net::TcpStream,
};

use super::ws_adapter::WebSocketAdapter;

#[derive(Debug)]
pub enum Transport {
    Tcp(Box<TcpStream>),
    WebSocket(Box<WebSocketAdapter<TcpStream>>),
}

impl Transport {
    pub fn new_tcp(stream: TcpStream) -> Self {
        Self::Tcp(Box::new(stream))
    }

    pub fn new_websocket(ws: tokio_tungstenite::WebSocketStream<TcpStream>) -> Self {
        Self::WebSocket(Box::new(WebSocketAdapter::new(ws)))
    }
}

impl AsyncRead for Transport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut tokio::io::ReadBuf,
    ) -> Poll<Result<()>> {
        match &mut *self {
            Self::Tcp(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            Self::WebSocket(adapter) => Pin::new(adapter.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Transport {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
        match &mut *self {
            Self::Tcp(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            Self::WebSocket(adapter) => Pin::new(adapter.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        match &mut *self {
            Self::Tcp(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            Self::WebSocket(adapter) => Pin::new(adapter.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        match &mut *self {
            Self::Tcp(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            Self::WebSocket(adapter) => Pin::new(adapter.as_mut()).poll_shutdown(cx),
        }
    }
}
