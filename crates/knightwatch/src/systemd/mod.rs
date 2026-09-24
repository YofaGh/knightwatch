mod client;
mod commands;
mod event;

#[cfg(target_os = "linux")]
mod helper_client;
#[cfg(target_os = "linux")]
mod monitor;
#[cfg(target_os = "linux")]
mod proxies;
#[cfg(target_os = "linux")]
mod systemd_snap;
#[cfg(target_os = "linux")]
mod types;
#[cfg(target_os = "linux")]
mod utils;

#[cfg(target_os = "linux")]
pub use monitor::{init_systemd_monitor, start_systemd_monitor};

pub use kw_types::systemd::{ServiceAction, SystemdSnapshot, UnitActiveState, UnitSnapshot};

pub use client::*;
pub use event::SystemdEvent;
