//! JSON protocol spoken between the unprivileged `knightwatch` server and
//! the privileged `kw-systemd-helper` binary over a Unix domain socket.
//! One JSON object per line (newline-delimited).

use serde::{Deserialize, Serialize};

use super::systemd::ServiceAction;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HelperRequest {
    Control {
        unit_name: String,
        action: ServiceAction,
    },
    Ping,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HelperResponse {
    Ok,
    Pong,
    Err { message: String },
}
