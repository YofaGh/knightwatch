use tokio::task::JoinHandle;
use tokio_tungstenite::accept_async;

use super::{
    framing::{receive_message, send_message},
    message::SocketMessage,
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

pub fn init_tcp_server() -> Option<JoinHandle<()>> {
    let args = &get_config().args;
    if !args.tcp_socket {
        return None;
    }
    spawn_server(&args.tcp_host, args.tcp_port, ServerProtocol::Tcp).ok()
}

pub fn init_ws_server() -> Option<JoinHandle<()>> {
    let args = &get_config().args;
    if !args.ws_socket {
        return None;
    }
    spawn_server(&args.ws_host, args.ws_port, ServerProtocol::WebSocket).ok()
}

async fn handle_client(connection: &mut Transport) -> Result<()> {
    send_message(connection, &SocketMessage::Handshake).await?;
    match receive_message(connection).await? {
        SocketMessage::HandshakeResponse => {}
        invalid => {
            return Err(Error::Other(format!(
                "Expected HandshakeResponse, Received {invalid:?}"
            )));
        }
    }
    Ok(())
}

fn spawn_server(host: &str, port: u16, protocol: ServerProtocol) -> Result<JoinHandle<()>> {
    let listener = crate::utils::get_listener(&format!("{host}:{port}"))?;
    let server = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tokio::spawn(async move {
                        let mut stream = match protocol {
                            ServerProtocol::WebSocket => match accept_async(stream).await {
                                Ok(ws) => Transport::new_websocket(ws),
                                Err(err) => {
                                    error!("WebSocket handshake failed for {addr}: {err}");
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
    });
    info!(protocol = %protocol, %host, port, "Socket listener started");
    Ok(server)
}
