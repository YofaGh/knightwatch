use serde::{Deserialize, Serialize};
use std::fmt;

use kw_utils::{conv, format_bytes, format_time};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub timestamp: String,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub disks: Vec<DiskSnapshot>,
    pub networks: Vec<NetworkSnapshot>,
    pub gpus: Vec<GpuSnapshot>,
    pub battery: Option<BatterySnapshot>,
    pub temperatures: Vec<ThermalSnapshot>,
    pub host: HostInfo,

    /// Derived aggregate health across all subsystems.
    pub health: SystemHealth,
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "=== System Snapshot  {}  health={} ===",
            self.timestamp, self.health
        )?;
        writeln!(f, "{}", self.host)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        if !self.disks.is_empty() {
            writeln!(f, "Disks:")?;
            for d in &self.disks {
                writeln!(f, "  {d}")?;
            }
        }
        if !self.networks.is_empty() {
            writeln!(f, "Networks:")?;
            for n in &self.networks {
                writeln!(f, "  {n}")?;
            }
        }
        if !self.gpus.is_empty() {
            writeln!(f, "GPUs:")?;
            for g in &self.gpus {
                writeln!(f, "  {g}")?;
            }
        }
        if let Some(b) = &self.battery {
            writeln!(f, "Battery: {b}")?;
        }
        if !self.temperatures.is_empty() {
            writeln!(f, "Temperatures:")?;
            for t in &self.temperatures {
                writeln!(f, "  {t}")?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSnapshot {
    /// Aggregate usage across all logical cores, 0–100.
    pub usage_percent: f32,

    /// Per-core breakdown.
    pub cores: Vec<CpuCoreSnapshot>,

    /// Current CPU frequency in MHz (aggregate / first physical core).
    pub frequency_mhz: u64,

    /// Brand string, e.g. "Intel(R) Core(TM) i9-13900K".
    pub brand: String,

    /// Number of physical cores (may differ from `cores.len()` with HT).
    pub physical_core_count: Option<usize>,

    /// System load averages (1/5/15 min). `None` on platforms where
    /// this isn't available (e.g. Windows), regardless of which
    /// platform is reading the data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_avg: Option<LoadAverage>,
}

impl fmt::Display for CpuSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "CPU: {}  {:.1}%  {} MHz  ({} physical cores)",
            self.brand,
            self.usage_percent,
            self.frequency_mhz,
            self.physical_core_count
                .map_or_else(|| "?".into(), |n| n.to_string())
        )?;
        for c in &self.cores {
            writeln!(
                f,
                "  {:<8} {:>5.1}%  {} MHz",
                c.name, c.usage_percent, c.frequency_mhz
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCoreSnapshot {
    /// Core label, e.g. "cpu0".
    pub name: String,
    /// Usage 0–100.
    pub usage_percent: f32,
    /// Frequency in MHz for this core.
    pub frequency_mhz: u64,
}

#[cfg(feature = "server")]
impl From<&sysinfo::Cpu> for CpuCoreSnapshot {
    fn from(cpu: &sysinfo::Cpu) -> Self {
        Self {
            name: cpu.name().to_string(),
            usage_percent: cpu.cpu_usage(),
            frequency_mhz: cpu.frequency(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[cfg(all(feature = "server", any(target_os = "linux", target_os = "macos")))]
impl From<sysinfo::LoadAvg> for LoadAverage {
    fn from(la: sysinfo::LoadAvg) -> Self {
        Self {
            one: la.one,
            five: la.five,
            fifteen: la.fifteen,
        }
    }
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    // --- RAM ---
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub free_bytes: u64,
    /// used / total, 0–100.
    pub used_percent: f32,

    // --- Swap ---
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_free_bytes: u64,
    /// `swap_used` / `swap_total`, 0–100. None when no swap is configured.
    pub swap_used_percent: Option<f32>,
}

impl fmt::Display for MemorySnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "RAM:  {:.1}% used  {} / {}  (free: {})",
            self.used_percent,
            format_bytes(self.used_bytes),
            format_bytes(self.total_bytes),
            format_bytes(self.free_bytes)
        )?;
        if self.swap_total_bytes > 0 {
            writeln!(
                f,
                "Swap: {}% used  {} / {}",
                self.swap_used_percent.map_or(0, conv::f32_percent_to_u32),
                format_bytes(self.swap_used_bytes),
                format_bytes(self.swap_total_bytes)
            )?;
        } else {
            writeln!(f, "Swap: none")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSnapshot {
    /// OS-level device name, e.g. "/dev/sda1" or "C:\\".
    pub name: String,
    /// Mount point or drive letter, e.g. "/" or "C:\\".
    pub mount_point: String,
    /// Filesystem type, e.g. "ext4", "apfs", "ntfs".
    pub file_system: String,
    pub kind: DiskKind,
    pub is_removable: bool,

    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    /// used / total, 0–100.
    pub used_percent: f32,
}

impl fmt::Display for DiskSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} ({})  {}  {:.1}% used  {} / {}  avail: {}{}",
            self.mount_point,
            self.file_system,
            self.kind,
            self.used_percent,
            format_bytes(self.used_bytes),
            format_bytes(self.total_bytes),
            format_bytes(self.available_bytes),
            if self.is_removable {
                "  [removable]"
            } else {
                ""
            },
        )
    }
}

#[cfg(feature = "server")]
impl From<&sysinfo::Disk> for DiskSnapshot {
    fn from(disk: &sysinfo::Disk) -> Self {
        let total = disk.total_space();
        let available = disk.available_space();
        let used = total.saturating_sub(available);
        let used_percent = conv::u64_ratio_percent_f32(used, total);
        Self {
            name: disk.name().to_string_lossy().into_owned(),
            mount_point: disk.mount_point().to_string_lossy().into_owned(),
            file_system: disk.file_system().to_string_lossy().into_owned(),
            kind: disk.kind().into(),
            is_removable: disk.is_removable(),
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
            used_percent,
        }
    }
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSnapshot {
    /// Interface name, e.g. "eth0", "en0", "Wi-Fi".
    pub interface: String,

    /// Received bytes since last tick (delta).
    pub rx_bytes_per_sec: u64,
    /// Transmitted bytes since last tick (delta).
    pub tx_bytes_per_sec: u64,

    /// Total received bytes since interface was brought up (cumulative).
    pub rx_total_bytes: u64,
    /// Total transmitted bytes since interface was brought up (cumulative).
    pub tx_total_bytes: u64,

    /// Received packets since last tick.
    pub rx_packets_per_sec: u64,
    /// Transmitted packets since last tick.
    pub tx_packets_per_sec: u64,

    /// Receive errors since last tick.
    pub rx_errors: u64,
    /// Transmit errors since last tick.
    pub tx_errors: u64,
}

impl fmt::Display for NetworkSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:<12}  ↓ {}/s  ↑ {}/s  total ↓ {}  ↑ {}",
            self.interface,
            format_bytes(self.rx_bytes_per_sec),
            format_bytes(self.tx_bytes_per_sec),
            format_bytes(self.rx_total_bytes),
            format_bytes(self.tx_total_bytes),
        )
    }
}

#[cfg(feature = "server")]
impl From<(&String, &sysinfo::NetworkData)> for NetworkSnapshot {
    fn from((name, data): (&String, &sysinfo::NetworkData)) -> Self {
        Self {
            interface: name.clone(),
            rx_bytes_per_sec: data.received(),
            tx_bytes_per_sec: data.transmitted(),
            rx_total_bytes: data.total_received(),
            tx_total_bytes: data.total_transmitted(),
            rx_packets_per_sec: data.packets_received(),
            tx_packets_per_sec: data.packets_transmitted(),
            rx_errors: data.errors_on_received(),
            tx_errors: data.errors_on_transmitted(),
        }
    }
}

// ---------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSnapshot {
    /// Identifier, e.g. "NVIDIA `GeForce` RTX 4090" or "Apple M3 Pro (GPU)".
    pub name: String,
    /// Core utilisation 0–100. None if unavailable.
    pub usage_percent: Option<f32>,
    /// VRAM used in bytes. None if unavailable.
    pub vram_used_bytes: Option<u64>,
    /// VRAM total in bytes. None if unavailable.
    pub vram_total_bytes: Option<u64>,
    pub vram_used_percent: Option<f32>,
    /// Core temperature °C. None if unavailable.
    pub temperature_celsius: Option<f32>,
    /// Power draw in watts. None if unavailable.
    pub power_draw_watts: Option<f32>,
    /// TDP limit in watts. None if unavailable.
    pub power_limit_watts: Option<f32>,
    /// Fans speed 0–100. empty if unavailable or fanless.
    pub fan_speed_percent: Vec<f32>,
}

impl fmt::Display for GpuSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(u) = self.usage_percent {
            write!(f, "  gpu={u:.1}%")?;
        }
        if let (Some(u), Some(t)) = (self.vram_used_bytes, self.vram_total_bytes) {
            write!(f, "  vram={}/{}", format_bytes(u), format_bytes(t))?;
        }
        if let Some(t) = self.temperature_celsius {
            write!(f, "  temp={t:.0}°C")?;
        }
        if let Some(p) = self.power_draw_watts {
            write!(f, "  power={p:.0}W")?;
        }
        Ok(())
    }
}

#[cfg(feature = "server")]
impl From<nvml_wrapper::Device<'_>> for GpuSnapshot {
    fn from(device: nvml_wrapper::Device) -> Self {
        let vram = device.memory_info().ok();
        let used = vram.as_ref().map(|v| v.used);
        let total = vram.as_ref().map(|v| v.total);
        let used_percent = match (used, total) {
            (Some(used), Some(total)) => Some(conv::u64_ratio_percent_f32(used, total)),
            _ => None,
        };
        let num_fans = device.num_fans().unwrap_or(0);
        let fan_speed_percent = (0..num_fans)
            .filter_map(|i| device.fan_speed(i).map(conv::u32_to_f32).ok())
            .collect();
        Self {
            name: device.name().unwrap_or_default(),
            usage_percent: device
                .utilization_rates()
                .map(|r| conv::u32_to_f32(r.gpu))
                .ok(),
            vram_used_bytes: used,
            vram_total_bytes: total,
            vram_used_percent: used_percent,
            temperature_celsius: device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .map(conv::u32_to_f32)
                .ok(),
            power_draw_watts: device
                .power_usage()
                .map(|p| conv::u32_to_f32(p) / 1000.0)
                .ok(),
            power_limit_watts: device
                .enforced_power_limit()
                .map(|p| conv::u32_to_f32(p) / 1000.0)
                .ok(),
            fan_speed_percent,
        }
    }
}

// ---------------------------------------------------------------------------
// Battery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatterySnapshot {
    /// Charge level 0–100.
    pub charge_percent: f32,
    pub state: BatteryState,
    /// Estimated time remaining in seconds. None if charging or unknown.
    pub time_to_empty_secs: Option<u64>,
    /// Estimated time to full charge in seconds. None if discharging or unknown.
    pub time_to_full_secs: Option<u64>,
    /// Current power draw from the battery in watts. None if unavailable.
    pub power_draw_watts: Option<f32>,
    /// Battery health / cycle count if the OS exposes it.
    pub cycle_count: Option<u32>,
    /// Battery health percentage (100 = new). None if unavailable.
    pub health_percent: Option<f32>,
}

impl fmt::Display for BatterySnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:.0}%  {}", self.charge_percent, self.state)?;
        match self.state {
            BatteryState::Discharging => write!(
                f,
                "  remaining: {}",
                self.time_to_empty_secs
                    .map_or_else(|| "N/A".into(), format_time)
            )?,
            BatteryState::Charging => write!(
                f,
                "  full in: {}",
                self.time_to_full_secs
                    .map_or_else(|| "N/A".into(), format_time)
            )?,
            _ => {}
        }
        if let Some(h) = self.health_percent {
            write!(f, "  health={h:.0}%")?;
        }
        Ok(())
    }
}

#[cfg(feature = "server")]
impl From<starship_battery::Battery> for BatterySnapshot {
    fn from(battery: starship_battery::Battery) -> Self {
        Self {
            charge_percent: battery.state_of_charge().value * 100.0,
            state: battery.state().into(),
            time_to_empty_secs: battery
                .time_to_empty()
                .map(|t| conv::f32_to_u64_saturating(t.value)),
            time_to_full_secs: battery
                .time_to_full()
                .map(|t| conv::f32_to_u64_saturating(t.value)),
            power_draw_watts: Some(battery.energy_rate().value),
            cycle_count: battery.cycle_count(),
            health_percent: Some(battery.state_of_health().value * 100.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Thermals
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalSnapshot {
    /// Sensor label, e.g. "coretemp Package id 0", "acpitz temp1".
    pub label: String,
    pub temperature_celsius: Option<f32>,
    /// Maximum recorded temperature for this sensor.
    pub temperature_max_celsius: Option<f32>,
    /// Critical threshold for this sensor. None if not reported by driver.
    pub temperature_critical_celsius: Option<f32>,
}

impl fmt::Display for ThermalSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let temp = self
            .temperature_celsius
            .map_or_else(|| "N/A".into(), |t| format!("{t:.0}°C"));
        let max = self
            .temperature_max_celsius
            .map_or_else(|| "?".into(), |t| format!("{t:.0}°C"));
        let crit = self
            .temperature_critical_celsius
            .map_or_else(|| "?".into(), |t| format!("{t:.0}°C"));
        write!(
            f,
            "{:<40}  {} (max: {} crit: {})",
            self.label, temp, max, crit
        )
    }
}

#[cfg(feature = "server")]
impl From<&sysinfo::Component> for ThermalSnapshot {
    fn from(c: &sysinfo::Component) -> Self {
        Self {
            label: c.label().to_string(),
            temperature_celsius: c.temperature(),
            temperature_max_celsius: c.max(),
            temperature_critical_celsius: c.critical(),
        }
    }
}

// ---------------------------------------------------------------------------
// Host info — static / very slowly changing
// ---------------------------------------------------------------------------

/// Static host information. Collected once at startup and re-broadcast inside
/// every `SystemSnapshot` for convenience.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub hostname: Option<String>,
    /// OS long name, e.g. "Ubuntu 24.04.1 LTS".
    pub os_name: Option<String>,
    /// Kernel version string.
    pub kernel_version: Option<String>,
    /// CPU architecture, e.g. "`x86_64`", "aarch64".
    pub cpu_arch: Option<String>,
    /// System uptime in seconds.
    pub uptime_secs: u64,
    /// Total number of running processes on the system.
    pub process_count: usize,
}

impl fmt::Display for HostInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let uptime = format_time(self.uptime_secs);
        writeln!(
            f,
            "hostname: {}  os: {}  kernel: {}  arch: {}  uptime: {}  processes: {}",
            self.hostname.as_deref().unwrap_or("?"),
            self.os_name.as_deref().unwrap_or("?"),
            self.kernel_version.as_deref().unwrap_or("?"),
            self.cpu_arch.as_deref().unwrap_or("?"),
            uptime,
            self.process_count
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Thresholds {
    pub cpu_warn: f32,
    pub memory_warn: f32,
    pub disk_warn: f32,
    pub battery_low: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_warn: 90.0,
            memory_warn: 90.0,
            disk_warn: 90.0,
            battery_low: 15.0,
        }
    }
}

/// Controls which subsystems are collected on each tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshMask {
    pub cpu: bool,
    pub memory: bool,
    pub disks: bool,
    pub networks: bool,
    pub temperatures: bool,
    pub gpus: bool,
}

impl Default for RefreshMask {
    fn default() -> Self {
        Self {
            cpu: true,
            memory: true,
            disks: true,
            networks: true,
            temperatures: true,
            gpus: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemHealth {
    /// Everything within normal thresholds.
    Healthy,
    /// One or more subsystems are elevated but not critical.
    Warning,
    /// One or more subsystems are at critical levels.
    Critical,
}

impl fmt::Display for SystemHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Warning => write!(f, "warning"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    /// Battery present but state cannot be determined.
    Unknown,
}

impl fmt::Display for BatteryState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Charging => write!(f, "charging"),
            Self::Discharging => write!(f, "discharging"),
            Self::Full => write!(f, "full"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(feature = "server")]
impl From<starship_battery::State> for BatteryState {
    fn from(state: starship_battery::State) -> Self {
        match state {
            starship_battery::State::Charging => Self::Charging,
            starship_battery::State::Discharging => Self::Discharging,
            starship_battery::State::Full => Self::Full,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskKind {
    Ssd,
    Hdd,
    /// e.g. RAM disk, network mount, fuse, etc.
    Unknown,
}

#[cfg(feature = "server")]
impl From<sysinfo::DiskKind> for DiskKind {
    fn from(kind: sysinfo::DiskKind) -> Self {
        match kind {
            sysinfo::DiskKind::HDD => Self::Hdd,
            sysinfo::DiskKind::SSD => Self::Ssd,
            sysinfo::DiskKind::Unknown(_) => Self::Unknown,
        }
    }
}

impl fmt::Display for DiskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ssd => write!(f, "SSD"),
            Self::Hdd => write!(f, "HDD"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Public, client-facing status for a single threshold alarm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlarmStatus {
    pub active: bool,
    /// When the alarm most recently transitioned into `active`.
    /// None if it has never fired.
    pub since: Option<std::time::SystemTime>,
}

/// Aggregate snapshot of all alarm state, queryable independently
/// of the next tick's events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlarmSnapshot {
    pub cpu: AlarmStatus,
    pub memory: AlarmStatus,
    pub disks: Vec<(String, AlarmStatus)>, // mount_point -> status, stable order
    pub battery_low: AlarmStatus,
}

impl fmt::Display for AlarmStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.active {
            return write!(f, "clear");
        }
        match self.since.and_then(|t| t.elapsed().ok()) {
            Some(elapsed) => write!(f, "active (for {})", format_time(elapsed.as_secs())),
            None => write!(f, "active"),
        }
    }
}

impl fmt::Display for AlarmSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut active: Vec<String> = Vec::new();

        if self.cpu.active {
            active.push(format!("cpu: {}", self.cpu));
        }
        if self.memory.active {
            active.push(format!("memory: {}", self.memory));
        }
        if self.battery_low.active {
            active.push(format!("battery: {}", self.battery_low));
        }
        for (mount, status) in &self.disks {
            if status.active {
                active.push(format!("disk[{mount}]: {status}"));
            }
        }

        if active.is_empty() {
            write!(f, "no active alarms")
        } else {
            write!(f, "{}", active.join(", "))
        }
    }
}
