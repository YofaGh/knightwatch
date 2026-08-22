use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};
use tokio::sync::{OnceCell, broadcast, mpsc};
use zbus::{Connection, zvariant::OwnedObjectPath};

use kw_types::{
    polling::Poll,
    systemd::{SystemdSnapshot, UnitActiveState, UnitLoadState, UnitSnapshot, UnitType},
};
use kw_utils::conv::{u64_to_u32_saturating, usize_to_u32_saturating};

use super::{
    commands::{SystemdCommand, SystemdMonitorChannels, SystemdQuery},
    event::SystemdEvent,
    helper_client::SystemdHelperClient,
    proxies::{SystemdManagerProxy, SystemdServiceProxy, SystemdUnitProxy},
    systemd_snap::UnitFilter,
    types::{ServiceDetails, ServiceProperties, UnitProperties},
    utils::block_on_local,
};
use crate::prelude::*;

struct SystemdMonitorState {
    snapshot: Option<SystemdSnapshot>,
    active_states: HashMap<String, UnitActiveState>,
    failed_units: HashSet<String>,
}

impl SystemdMonitorState {
    fn new() -> Self {
        Self {
            snapshot: None,
            active_states: HashMap::new(),
            failed_units: HashSet::new(),
        }
    }
}

pub struct SystemdMonitor {
    state: SystemdMonitorState,
    pub channels: SystemdMonitorChannels,
    conn: Connection,
    poll: Poll,
    filter: UnitFilter,
    first_tick: bool,
    helper: Arc<OnceCell<SystemdHelperClient>>,
}

impl SystemdMonitor {
    pub async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .map_err(|e| Error::Systemd(format!("D-Bus connection failed: {e}")))?;
        Ok(Self {
            state: SystemdMonitorState::new(),
            channels: SystemdMonitorChannels::new(),
            conn,
            poll: Poll::new(5),
            filter: UnitFilter::default(),
            first_tick: true,
            helper: Arc::new(OnceCell::new()),
        })
    }

    #[allow(dead_code)]
    pub fn with_filter(mut self, filter: UnitFilter) -> Self {
        self.filter = filter;
        self
    }

    fn emit_event(&self, event: SystemdEvent) {
        let _ = self.channels.event_tx.send(event);
    }

    pub async fn start_monitor_loop(mut self) -> Result<()> {
        let mut query_rx = self.channels.take_query_rx()?;
        let mut command_rx = self.channels.take_command_rx()?;
        self.poll.resume();
        loop {
            let tick = async {
                match self.poll.interval_timer.as_mut() {
                    Some(timer) => timer.tick().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                Some(query) = query_rx.recv() => {
                    self.handle_query(query);
                }
                Some(command) = command_rx.recv() => {
                    self.handle_command(command);
                }
                _ = tick => {
                    self.handle_tick().await;
                }
            }
        }
    }

    fn handle_query(&self, query: SystemdQuery) {
        match query {
            SystemdQuery::Snapshot { response } => {
                let _ = response.send(self.state.snapshot.clone());
            }
            SystemdQuery::Unit {
                unit_name,
                response,
            } => {
                let unit = self
                    .state
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.units.iter().find(|u| u.unit_name == unit_name).cloned());
                let _ = response.send(unit);
            }
            SystemdQuery::ByActiveState { state, response } => {
                let units = self
                    .state
                    .snapshot
                    .as_ref()
                    .map(|s| {
                        s.units
                            .iter()
                            .filter(|u| u.active_state == state)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = response.send(units);
            }
            SystemdQuery::PollStatus { response } => {
                let _ = response.send(Some((&self.poll).into()));
            }
        }
    }

    fn handle_command(&mut self, command: SystemdCommand) {
        match command {
            // ----------------------------------------------------------------
            // Polling control.
            // ----------------------------------------------------------------
            SystemdCommand::SetPollInterval { interval, response } => {
                self.poll.set_interval(interval);
                info!(ms = interval.as_millis(), "systemd poll interval updated");
                let _ = response.send(Ok(()));
            }
            SystemdCommand::PausePoll { response } => {
                self.poll.pause();
                info!("systemd polling paused");
                let _ = response.send(Ok(()));
            }
            SystemdCommand::ResumePoll { response } => {
                self.poll.resume();
                info!("systemd polling resumed");
                let _ = response.send(Ok(()));
            }
            SystemdCommand::Control {
                unit_name,
                action,
                response,
            } => {
                let helper = self.helper.clone();
                tokio::spawn(async move {
                    let result = match helper.get() {
                        Some(h) => h.control(&unit_name, action).await,
                        None => Err(Error::Systemd(
                            "systemd command helper is not ready (auth pending, cancelled, or unavailable)".into(),
                        )),
                    };
                    let _ = response.send(result);
                });
            }
        }
    }

    async fn handle_tick(&mut self) {
        match self.build_snapshot().await {
            Ok(snapshot) => {
                self.diff_and_emit_events(&snapshot);
                if self.first_tick {
                    info!(
                        "Systemd Monitor: initial snapshot ready ({} units)",
                        snapshot.units.len()
                    );
                    self.emit_event(SystemdEvent::InitialSnapshot {
                        snapshot: snapshot.clone(),
                    });
                    self.first_tick = false;
                } else {
                    self.emit_event(SystemdEvent::Tick {
                        snapshot: snapshot.clone(),
                    });
                }
                self.state.snapshot = Some(snapshot);
            }
            Err(e) => {
                error!(?e, "systemd monitor: failed to build snapshot");
            }
        }
    }

    fn diff_and_emit_events(&mut self, snapshot: &SystemdSnapshot) {
        let current_names: HashSet<String> =
            snapshot.units.iter().map(|u| u.unit_name.clone()).collect();
        let previous_names: HashSet<String> = self.state.active_states.keys().cloned().collect();

        // Units that appeared since last tick
        for name in current_names.difference(&previous_names) {
            if let Some(unit) = snapshot.units.iter().find(|u| &u.unit_name == name) {
                // if !self.first_tick {
                //     info!(unit_name = %name, "systemd unit appeared");
                // }
                info!(unit_name = %name, "systemd unit appeared");
                self.emit_event(SystemdEvent::UnitAppeared { unit: unit.clone() });
            }
        }

        // Units that disappeared
        for name in previous_names.difference(&current_names) {
            info!(unit_name = %name, "systemd unit disappeared");
            self.emit_event(SystemdEvent::UnitDisappeared {
                unit_name: name.clone(),
            });
        }

        // State transitions for existing units
        for unit in &snapshot.units {
            let prev = self.state.active_states.get(&unit.unit_name);

            let was_failed = self.state.failed_units.contains(&unit.unit_name);
            let now_failed = unit.active_state == UnitActiveState::Failed;

            if now_failed && !was_failed {
                let previous_state = prev.cloned().unwrap_or(UnitActiveState::Inactive);
                warn!(unit_name = %unit.unit_name, ?previous_state, "systemd unit entered failed state");
                self.emit_event(SystemdEvent::UnitFailed {
                    unit_name: unit.unit_name.clone(),
                    previous_state,
                });
            } else if !now_failed && was_failed {
                info!(unit_name = %unit.unit_name, "systemd unit recovered from failed state");
                self.emit_event(SystemdEvent::UnitRecovered {
                    unit_name: unit.unit_name.clone(),
                });
            }
        }

        // Update state maps for next tick
        self.state.active_states = snapshot
            .units
            .iter()
            .map(|u| (u.unit_name.clone(), u.active_state.clone()))
            .collect();
        self.state.failed_units = snapshot
            .units
            .iter()
            .filter(|u| u.active_state == UnitActiveState::Failed)
            .map(|u| u.unit_name.clone())
            .collect();
    }

    async fn build_snapshot(&self) -> Result<SystemdSnapshot> {
        let manager = SystemdManagerProxy::new(&self.conn)
            .await
            .map_err(|e| Error::Systemd(format!("Failed to create manager proxy: {e}")))?;

        let raw_units = manager
            .list_units()
            .await
            .map_err(|e| Error::Systemd(format!("ListUnits() failed: {e}")))?;

        let mut units: Vec<UnitSnapshot> = Vec::new();

        for raw in raw_units {
            let unit_name = raw.0.clone();
            let unit_type = UnitType::from_name(&unit_name);

            if !self.filter.matches(&unit_type, &raw.3, &unit_name) {
                continue;
            }

            let active_state = UnitActiveState::from(raw.3.as_str());
            let load_state = UnitLoadState::from(raw.2.as_str());

            // Fetch per-unit extended properties for active services only
            let (main_pid, memory_bytes, cpu_usage_ns, restart_count, since, fragment_path) =
                if matches!(unit_type, UnitType::Service)
                    && matches!(
                        active_state,
                        UnitActiveState::Active | UnitActiveState::Reloading
                    )
                {
                    self.fetch_service_details(&unit_name, &raw.6)
                } else {
                    (None, None, None, None, None, None)
                };

            units.push(UnitSnapshot {
                unit_name,
                unit_type,
                load_state,
                active_state,
                sub_state: raw.4.clone(),
                description: raw.1.clone(),
                main_pid,
                memory_bytes,
                cpu_usage_ns,
                restart_count,
                since,
                fragment_path,
            });
        }
        units.sort_by(|a, b| {
            Self::rank(a)
                .cmp(&Self::rank(b))
                .then(a.unit_name.cmp(&b.unit_name))
        });

        let failed_count = usize_to_u32_saturating(
            units
                .iter()
                .filter(|u| u.active_state == UnitActiveState::Failed)
                .count(),
        );
        let active_count = usize_to_u32_saturating(
            units
                .iter()
                .filter(|u| u.active_state == UnitActiveState::Active)
                .count(),
        );
        let inactive_count = usize_to_u32_saturating(
            units
                .iter()
                .filter(|u| u.active_state == UnitActiveState::Inactive)
                .count(),
        );

        Ok(SystemdSnapshot {
            timestamp: crate::utils::now_rfc3339(),
            units,
            failed_count,
            active_count,
            inactive_count,
        })
    }

    // Sort: failed first, then active, then by name
    const fn rank(u: &UnitSnapshot) -> u8 {
        match u.active_state {
            UnitActiveState::Failed => 0,
            UnitActiveState::Active => 1,
            UnitActiveState::Reloading => 2,
            UnitActiveState::Activating => 3,
            UnitActiveState::Deactivating => 4,
            UnitActiveState::Inactive => 5,
        }
    }

    /// Fetch extended properties for a single active .service unit.
    /// Delegates to two focused helpers — one per D-Bus interface — so a
    /// missing interface on one doesn't affect the other.
    fn fetch_service_details(
        &self,
        unit_name: &str,
        object_path: &OwnedObjectPath,
    ) -> ServiceDetails {
        let (fragment_path, since) = self.fetch_unit_properties(unit_name, object_path);
        let (main_pid, memory_bytes, cpu_usage_ns, restart_count) =
            self.fetch_service_properties(object_path);

        (
            main_pid,
            memory_bytes,
            cpu_usage_ns,
            restart_count,
            since,
            fragment_path,
        )
    }

    /// Reads `org.freedesktop.systemd1.Unit` properties via the object path
    /// returned by `ListUnits()`. Both calls are independent so a missing
    /// property on one doesn't suppress the other.
    fn fetch_unit_properties(
        &self,
        unit_name: &str,
        object_path: &OwnedObjectPath,
    ) -> UnitProperties {
        // Build proxy once
        let Some(proxy) = SystemdUnitProxy::builder(&self.conn)
            .path(object_path.clone())
            .map_err(|e| debug!(?e, unit_name, "could not build unit proxy"))
            .ok()
            .and_then(|b| block_on_local(async { b.build().await.ok() }))
        else {
            return (None, None);
        };
        block_on_local(async {
            let fragment_path = proxy.fragment_path().await.ok();

            let since = proxy
                .active_enter_timestamp()
                .await
                .ok()
                .and_then(|ts_usec| {
                    if ts_usec == 0 {
                        return None;
                    }
                    let secs = ts_usec / 1_000_000;
                    let nsecs = u64_to_u32_saturating((ts_usec % 1_000_000).saturating_mul(1000));
                    chrono::DateTime::from_timestamp(secs.cast_signed(), nsecs)
                        .map(|dt| dt.to_rfc3339())
                });

            (fragment_path, since)
        })
    }

    /// Reads `org.freedesktop.systemd1.Service` properties.
    /// All four properties are fetched in a single proxy session.
    fn fetch_service_properties(&self, object_path: &OwnedObjectPath) -> ServiceProperties {
        let svc_proxy = SystemdServiceProxy::builder(&self.conn)
            .path(object_path.clone())
            .ok()
            .and_then(|b| block_on_local(async { b.build().await.ok() }));

        svc_proxy.map_or((None, None, None, None), |svc| {
            block_on_local(async {
                let pid = svc.main_pid().await.ok().filter(|&p| p != 0);
                let mem = svc.memory_current().await.ok().filter(|&m| m != u64::MAX);
                let cpu = svc
                    .c_p_u_usage_n_sec()
                    .await
                    .ok()
                    .filter(|&c| c != u64::MAX);
                let restarts = svc.n_restarts().await.ok();
                (pid, mem, cpu, restarts)
            })
        })
    }

    /// Handle used by `init_systemd_monitor` to inject the helper once
    /// `pkexec` resolves, without blocking the monitor loop while waiting.
    pub fn helper_handle(&self) -> Arc<OnceCell<SystemdHelperClient>> {
        self.helper.clone()
    }
}

pub static SYSTEMD_QUERY_SENDER: OnceLock<mpsc::Sender<SystemdQuery>> = OnceLock::new();
pub static SYSTEMD_EVENT_SENDER: OnceLock<broadcast::Sender<SystemdEvent>> = OnceLock::new();
pub static SYSTEMD_COMMAND_SENDER: OnceLock<mpsc::Sender<SystemdCommand>> = OnceLock::new();
