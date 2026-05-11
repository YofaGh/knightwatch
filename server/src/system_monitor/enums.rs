use serde::{Serialize, Deserialize};
#[cfg(feature = "ssr")]
use tokio::sync::oneshot;

use super::structs::*;

#[derive(Debug, Clone, serde::Serialize)]
pub enum SystemMonitorEvent {
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
    BatteryStateChanged { state: BatteryState },
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub enum SystemMonitorQuery {
    /// Returns the most recent full snapshot.
    Snapshot {
        response: oneshot::Sender<Option<SystemSnapshot>>,
    },

    /// Returns the most recent CPU reading only (cheaper to clone).
    Cpu {
        response: oneshot::Sender<Option<CpuSnapshot>>,
    },

    /// Returns the most recent memory reading.
    Memory {
        response: oneshot::Sender<Option<MemorySnapshot>>,
    },

    /// Returns the most recent per-disk readings.
    Disks {
        response: oneshot::Sender<Vec<DiskSnapshot>>,
    },

    /// Returns the most recent per-network-interface readings.
    Networks {
        response: oneshot::Sender<Vec<NetworkSnapshot>>,
    },

    /// Returns the most recent GPU readings (may be empty if unsupported).
    Gpus {
        response: oneshot::Sender<Vec<GpuSnapshot>>,
    },

    /// Returns the most recent battery snapshot (None if no battery present).
    Battery {
        response: oneshot::Sender<Option<BatterySnapshot>>,
    },

    /// Returns the host info (static — only changes on hostname/OS update).
    HostInfo {
        response: oneshot::Sender<Option<HostInfo>>,
    },

    /// Returns thermal readings (may be empty if unsupported).
    Temperatures {
        response: oneshot::Sender<Vec<ThermalSnapshot>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SystemHealth {
    /// Everything within normal thresholds.
    Healthy,
    /// One or more subsystems are elevated but not critical.
    Warning,
    /// One or more subsystems are at critical levels.
    Critical,
}

impl std::fmt::Display for SystemHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Warning => write!(f, "warning"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    /// Battery present but state cannot be determined.
    Unknown,
}

impl std::fmt::Display for BatteryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Charging => write!(f, "charging"),
            Self::Discharging => write!(f, "discharging"),
            Self::Full => write!(f, "full"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(feature = "ssr")]
impl From<battery::State> for BatteryState {
    fn from(state: battery::State) -> Self {
        match state {
            battery::State::Charging => Self::Charging,
            battery::State::Discharging => Self::Discharging,
            battery::State::Full => Self::Full,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DiskKind {
    Ssd,
    Hdd,
    /// e.g. RAM disk, network mount, fuse, etc.
    Unknown,
}

#[cfg(feature = "ssr")]
impl From<sysinfo::DiskKind> for DiskKind {
    fn from(kind: sysinfo::DiskKind) -> Self {
        match kind {
            sysinfo::DiskKind::HDD => Self::Hdd,
            sysinfo::DiskKind::SSD => Self::Ssd,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for DiskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ssd => write!(f, "SSD"),
            Self::Hdd => write!(f, "HDD"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}
