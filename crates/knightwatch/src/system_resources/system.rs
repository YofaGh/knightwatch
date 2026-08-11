use kw_types::resources::AlarmStatus;

#[derive(Debug, Clone, serde::Serialize)]
pub struct StaticHostInfo {
    pub hostname: Option<String>,
    /// OS long name, e.g. "Ubuntu 24.04.1 LTS".
    pub os_name: Option<String>,
    /// Kernel version string.
    pub kernel_version: Option<String>,
    /// CPU architecture, e.g. "`x86_64`", "aarch64".
    pub cpu_arch: Option<String>,
}

#[derive(Default, Clone)]
pub struct ThresholdAlarm {
    pub exceeded: bool,
    pub since: Option<std::time::SystemTime>, // set on rising edge only
    pub last_emitted: Option<std::time::SystemTime>, // cooldown bookkeeping, not exposed
}

impl From<&ThresholdAlarm> for AlarmStatus {
    fn from(a: &ThresholdAlarm) -> Self {
        Self {
            active: a.exceeded,
            since: a.exceeded.then_some(a.since).flatten(),
        }
    }
}

impl From<ThresholdAlarm> for AlarmStatus {
    fn from(a: ThresholdAlarm) -> Self {
        (&a).into()
    }
}
