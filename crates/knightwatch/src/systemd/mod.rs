use crate::prelude::*;

mod client;
mod commands;
mod event;
mod systemd_snap;

#[cfg(target_os = "linux")]
mod helper_client;
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
                let _ = monitor::SYSTEMD_QUERY_SENDER.set(monitor.channels.query_tx.clone());
                let _ = monitor::SYSTEMD_EVENT_SENDER.set(monitor.channels.event_tx.clone());

                if config.args.allow_systemd_commands {
                    let helper_slot = monitor.helper_handle();
                    let command_tx = monitor.channels.command_tx.clone();
                    tokio::spawn(async move {
                        match helper_client::SystemdHelperClient::spawn().await {
                            Ok(Some(helper)) => {
                                if helper_slot.set(helper).is_ok() {
                                    let _ = monitor::SYSTEMD_COMMAND_SENDER.set(command_tx);
                                    info!("Systemd command helper ready");
                                }
                            }
                            Ok(None) => {
                                warn!(
                                    "Systemd command helper was not authorized — commands stay disabled"
                                );
                            }
                            Err(e) => {
                                error!(?e, "failed to start systemd command helper");
                            }
                        }
                    });
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

pub use kw_types::systemd::{ServiceAction, SystemdSnapshot, UnitActiveState, UnitSnapshot};

pub use client::*;
pub use event::SystemdEvent;
