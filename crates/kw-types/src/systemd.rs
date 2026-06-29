use serde::{Deserialize, Serialize};
use std::fmt;

/// One unit row — equivalent to a line in `systemctl list-units`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitSnapshot {
    pub unit_name: String, // e.g. "nginx.service"
    pub unit_type: UnitType,
    pub load_state: UnitLoadState,
    pub active_state: UnitActiveState,
    pub sub_state: String, // e.g. "running", "dead", "waiting" — freeform from systemd
    pub description: String,

    // Only populated for .service units that are active
    pub main_pid: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub cpu_usage_ns: Option<u64>, // CPUUsageNSec from D-Bus
    pub restart_count: Option<u32>,
    pub since: Option<String>, // rfc3339 of last state change (ActiveEnterTimestamp)

    // Fragment path — useful for linking to unit file location
    pub fragment_path: Option<String>,
}

impl fmt::Display for UnitSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:<40}  {:>12}  {}",
            self.unit_name,
            self.active_state.as_str(),
            self.sub_state
        )?;
        if let Some(pid) = self.main_pid {
            write!(f, "  pid={pid}")?;
        }
        if let Some(mem) = self.memory_bytes {
            write!(f, "  mem={}", kw_utils::format_bytes(mem))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdSnapshot {
    pub timestamp: String,
    pub units: Vec<UnitSnapshot>,
    pub failed_count: u32,
    pub active_count: u32,
    pub inactive_count: u32,
}

impl fmt::Display for SystemdSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "Systemd  {}  active={}  inactive={}  failed={}",
            self.timestamp, self.active_count, self.inactive_count, self.failed_count
        )?;
        for u in &self.units {
            writeln!(f, "  {u}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitLoadState {
    Loaded,
    NotFound,
    BadSetting,
    Error,
    Masked,
}

impl From<&str> for UnitLoadState {
    fn from(s: &str) -> Self {
        match s {
            "loaded" => Self::Loaded,
            "not-found" => Self::NotFound,
            "bad-setting" => Self::BadSetting,
            "error" => Self::Error,
            "masked" => Self::Masked,
            _ => Self::NotFound,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitActiveState {
    Active,
    Reloading,
    Inactive,
    Failed,
    Activating,
    Deactivating,
}

impl UnitActiveState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Reloading => "reloading",
            Self::Inactive => "inactive",
            Self::Failed => "failed",
            Self::Activating => "activating",
            Self::Deactivating => "deactivating",
        }
    }
}

impl From<&str> for UnitActiveState {
    fn from(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "reloading" => Self::Reloading,
            "inactive" => Self::Inactive,
            "failed" => Self::Failed,
            "activating" => Self::Activating,
            "deactivating" => Self::Deactivating,
            _ => Self::Inactive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitType {
    Service,
    Socket,
    Target,
    Timer,
    Mount,
    Device,
    Other(String),
}

impl UnitType {
    pub fn from_name(name: &str) -> Self {
        match name.rsplit_once('.').map(|(_, ext)| ext) {
            Some("service") => UnitType::Service,
            Some("socket") => UnitType::Socket,
            Some("target") => UnitType::Target,
            Some("timer") => UnitType::Timer,
            Some("mount") | Some("automount") => UnitType::Mount,
            Some("device") => UnitType::Device,
            Some(other) => UnitType::Other(other.to_string()),
            None => UnitType::Other(String::new()),
        }
    }
}
