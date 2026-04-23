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
