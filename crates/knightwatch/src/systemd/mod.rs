use crate::prelude::*;

mod client;
mod event;
mod commands;
mod systemd_snap;

#[cfg(target_os = "linux")]
mod monitor;
#[cfg(target_os = "linux")]
mod proxies;
#[cfg(target_os = "linux")]
mod types;
#[cfg(target_os = "linux")]
mod utils;

#[cfg(target_os = "linux")]
pub fn init_systemd_monitor() {
    let config = get_config();
    if !config.args.systemd {
        return;
    }
    tokio::spawn(async move {
        match monitor::SystemdMonitor::new().await {
            Ok(monitor) => {
                monitor::SYSTEMD_QUERY_SENDER
                    .set(monitor.channels.query_tx.clone())
                    .unwrap();
                monitor::SYSTEMD_EVENT_SENDER
                    .set(monitor.channels.event_tx.clone())
                    .unwrap();
                if config.args.allow_systemd_commands {
                    monitor::SYSTEMD_COMMAND_SENDER
                        .set(monitor.channels.command_tx.clone())
                        .unwrap();
                }
                info!("Systemd Monitor started");
                if let Err(e) = monitor.start_monitor_loop().await {
                    error!(?e, "systemd monitor loop exited with error");
                }
            }
            Err(e) => {
                error!(
                    ?e,
                    "failed to initialise systemd monitor — is D-Bus available?"
                );
            }
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub fn init_systemd_monitor() {
    if !get_config().args.systemd {
        return;
    }
    warn!("Systemd is only available on linux os");
}

pub use kw_types::systemd::{SystemdSnapshot, UnitActiveState, UnitSnapshot};

pub use client::*;
pub use event::SystemdEvent;
