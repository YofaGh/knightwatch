use tokio::net::TcpListener;

use crate::prelude::*;

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
    Some(socket.local_addr().ok()?.ip().to_string())
}

pub fn print_local_ips() {
    let port = get_config().args.port;
    println!("API Server running at:");
    println!("  → http://localhost:{}", port);
    println!("  → http://127.0.0.1:{}", port);
    if let Some(ip) = get_local_ip() {
        println!("  → http://{}:{}", ip, port);
    }
}
