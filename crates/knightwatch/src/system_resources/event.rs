use serde_json::json;

use kw_types::resources::SystemSnapshot;

#[derive(Debug, Clone, serde::Serialize)]
pub enum SystemResourcesEvent {
    /// Emitted on the very first tick; establishes a baseline for all metrics.
    InitialSnapshot { snapshot: SystemSnapshot },

    /// Emitted every tick with fresh readings for all subsystems.
    Tick { snapshot: SystemSnapshot },

    /// CPU usage crossed a threshold (aggregate across all cores).
    CpuThresholdExceeded { usage_percent: f32, threshold: f32 },

    /// Memory usage crossed a threshold.
    MemoryThresholdExceeded { used_percent: f32, threshold: f32 },

    /// A disk's used percentage crossed a threshold.
    DiskThresholdExceeded {
        mount_point: String,
        used_percent: f32,
        threshold: f32,
    },

    /// Battery is discharging and has fallen below a threshold.
    BatteryLow { charge_percent: f32, threshold: f32 },

    /// Battery state changed (e.g. plugged in / unplugged).
    BatteryStateChanged { state: kw_types::resources::BatteryState },
}

impl From<&SystemResourcesEvent> for crate::events::EventPayload {
    fn from(event: &SystemResourcesEvent) -> Self {
        let (event_name, data) = match event {
            SystemResourcesEvent::InitialSnapshot { snapshot } => (
                "resources.initial_snapshot",
                json!({ "snapshot": snapshot }),
            ),
            SystemResourcesEvent::Tick { snapshot } => {
                ("resources.tick", json!({ "snapshot": snapshot }))
            }
            SystemResourcesEvent::CpuThresholdExceeded {
                usage_percent,
                threshold,
            } => (
                "resources.cpu_threshold_exceeded",
                json!({ "usage_percent": usage_percent, "threshold": threshold }),
            ),
            SystemResourcesEvent::MemoryThresholdExceeded {
                used_percent,
                threshold,
            } => (
                "resources.memory_threshold_exceeded",
                json!({ "usage_percent": used_percent, "threshold": threshold }),
            ),
            SystemResourcesEvent::DiskThresholdExceeded {
                mount_point,
                used_percent,
                threshold,
            } => (
                "resources.disk_threshold_exceeded",
                json!({ "mount_point": mount_point, "usage_percent": used_percent, "threshold": threshold }),
            ),
            SystemResourcesEvent::BatteryLow {
                charge_percent,
                threshold,
            } => (
                "resources.battery_low",
                json!({ "charge_percent": charge_percent, "threshold": threshold }),
            ),
            SystemResourcesEvent::BatteryStateChanged { state } => {
                ("resources.battery_state_changed", json!({ "state": state }))
            }
        };
        Self::new(crate::events::EventSource::SystemResources, event_name, data)
    }
}
