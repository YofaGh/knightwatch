use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{
    framing::{receive_message, send_message},
    transport::Transport,
};
use crate::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum ServerProtocol {
    Tcp,
    WebSocket,
}

impl std::fmt::Display for ServerProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Tcp => write!(f, "tcp"),
            Self::WebSocket => write!(f, "websocket"),
        }
    }
}

pub fn init_tcp_server(cancel_token: CancellationToken) -> Option<JoinHandle<()>> {
    let args = &get_config().args;
    if !args.tcp_socket {
        return None;
    }
    spawn_server(
        &args.tcp_host,
        args.tcp_port,
        ServerProtocol::Tcp,
        cancel_token,
    )
    .ok()
}

pub fn init_ws_server(cancel_token: CancellationToken) -> Option<JoinHandle<()>> {
    let args = &get_config().args;
    if !args.ws_socket {
        return None;
    }
    spawn_server(
        &args.ws_host,
        args.ws_port,
        ServerProtocol::WebSocket,
        cancel_token,
    )
    .ok()
}

/// How long a new client gets for the websocket upgrade and for the handshake,
/// so silent connections don't pile up.
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

async fn handle_client(connection: &mut Transport) -> Result<()> {
    use kw_types::socket::{Frame, SocketMessage};

    let exchange = async {
        send_message(connection, &Frame::push(SocketMessage::Handshake)).await?;
        receive_message::<_, Frame>(connection).await
    };
    let frame = tokio::time::timeout(HANDSHAKE_TIMEOUT, exchange)
        .await
        .map_err(|_| Error::Other("client did not answer the handshake in time".into()))??;
    if !matches!(frame.message, SocketMessage::HandshakeResponse) {
        return Err(Error::Other("Expected HandshakeResponse".into()));
    }
    Ok(())
}

fn spawn_server(
    host: &str,
    port: u16,
    protocol: ServerProtocol,
    cancel_token: CancellationToken,
) -> Result<JoinHandle<()>> {
    let listener = crate::utils::get_listener(&format!("{host}:{port}"))?;
    let server = tokio::spawn(async move {
        loop {
            tokio::select! {
                () = cancel_token.cancelled() => {
                    info!(%protocol, "Socket listener shutting down");
                    break;
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            tokio::spawn(async move {
                                let mut stream = match protocol {
                                    ServerProtocol::WebSocket => match tokio::time::timeout(HANDSHAKE_TIMEOUT, tokio_tungstenite::accept_async(stream)).await {
                                        Ok(Ok(ws)) => Transport::new_websocket(ws),
                                        Ok(Err(err)) => {
                                            error!("WebSocket handshake failed for {addr}: {err}");
                                            return;
                                        }
                                        Err(_) => {
                                            error!("WebSocket upgrade from {addr} timed out");
                                            return;
                                        }
                                    },
                                    ServerProtocol::Tcp => Transport::new_tcp(stream),
                                };
                                match handle_client(&mut stream).await {
                                    Ok(()) => match super::client_manager::add_client(stream).await {
                                        Ok(()) => info!("A new socket client was added"),
                                        Err(err) => error!("Failed to add client err: {err}"),
                                    },
                                    Err(err) => error!("Failed to handle client err: {err}"),
                                }
                            });
                        }
                        Err(err) => error!("Failed to accept {protocol:?} connection err: {err}"),
                    }
                }
            }
        }
    });
    info!(%protocol, %host, port, "Socket listener started");
    Ok(server)
}

#[cfg(test)]
#[path = "listener_tests.rs"]
mod tests;
