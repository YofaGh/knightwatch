use tokio::{net::TcpListener, sync::broadcast};

use crate::prelude::*;

#[cfg(debug_assertions)]
pub fn start_dev_server() -> Option<std::process::Child> {
    if std::net::TcpStream::connect("localhost:5173").is_ok() {
        return None;
    }
    info!("Starting vite server...");
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let child = std::process::Command::new(npm)
        .args(["run", "dev"])
        .current_dir(concat!(env!("CARGO_WORKSPACE_DIR"), "/dashboard"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to start vite dev server");
    Some(child)
}

pub fn get_listener(address: &str) -> Result<TcpListener> {
    let std_listener =
        std::net::TcpListener::bind(address).map_err(|err| Error::bind_address(address, err))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|err| Error::bind_address(address, err))?;
    TcpListener::from_std(std_listener).map_err(|err| Error::bind_address(address, err))
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

pub fn print_local_ips(port: u16) {
    println!("API Server running at:");
    println!("  → http://localhost:{}", port);
    println!("  → http://127.0.0.1:{}", port);
    if let Some(ip) = get_local_ip() {
        println!("  → http://{}:{}", ip, port);
    } else {
        debug!("Could not determine local IP address");
    }
}

pub fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn recv_or_pending<T: Clone>(rx: &mut Option<broadcast::Receiver<T>>, name: &str) -> T {
    match rx {
        Some(rx) => loop {
            match rx.recv().await {
                Ok(val) => return val,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => {
                    error!("{name} channel closed");
                    std::future::pending().await
                }
            }
        },
        None => std::future::pending().await,
    }
}
