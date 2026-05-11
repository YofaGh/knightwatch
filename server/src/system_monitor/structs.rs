use serde::{Serialize, Deserialize};

use super::enums::*;

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

    /// System load averages (1 min, 5 min, 15 min). Linux/macOS only.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub load_avg: LoadAverage,
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

#[cfg(feature = "ssr")]
impl From<&sysinfo::Cpu> for CpuCoreSnapshot {
    fn from(cpu: &sysinfo::Cpu) -> Self {
        Self {
            name: cpu.name().to_string(),
            usage_percent: cpu.cpu_usage(),
            frequency_mhz: cpu.frequency(),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
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
    /// swap_used / swap_total, 0–100. None when no swap is configured.
    pub swap_used_percent: Option<f32>,
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

#[cfg(feature = "ssr")]
impl From<&sysinfo::Disk> for DiskSnapshot {
    fn from(disk: &sysinfo::Disk) -> Self {
        let total = disk.total_space();
        let available = disk.available_space();
        let used = total.saturating_sub(available);
        let used_percent = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };
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

#[cfg(feature = "ssr")]
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
    /// Identifier, e.g. "NVIDIA GeForce RTX 4090" or "Apple M3 Pro (GPU)".
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

#[cfg(feature = "ssr")]
impl From<nvml_wrapper::Device<'_>> for GpuSnapshot {
    fn from(device: nvml_wrapper::Device) -> Self {
        let vram = device.memory_info().ok();
        let used = vram.as_ref().map(|v| v.used);
        let total = vram.as_ref().map(|v| v.total);
        let used_percent = if let (Some(used), Some(total)) = (used, total) {
            if total > 0 {
                Some((used as f32 / total as f32) * 100.0)
            } else {
                Some(0.0)
            }
        } else {
            None
        };
        let num_fans = device.num_fans().unwrap_or(0);
        let fan_speed_percent = (0..num_fans)
            .filter_map(|i| device.fan_speed(i).map(|f| f as f32).ok())
            .collect();
        Self {
            name: device.name().unwrap_or_default(),
            usage_percent: device.utilization_rates().map(|r| r.gpu as f32).ok(),
            vram_used_bytes: used,
            vram_total_bytes: total,
            vram_used_percent: used_percent,
            temperature_celsius: device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .map(|t| t as f32)
                .ok(),
            power_draw_watts: device.power_usage().map(|p| p as f32 / 1000.0).ok(),
            power_limit_watts: device
                .enforced_power_limit()
                .map(|p| p as f32 / 1000.0)
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

#[cfg(feature = "ssr")]
impl From<battery::Battery> for BatterySnapshot {
    fn from(battery: battery::Battery) -> Self {
        Self {
            charge_percent: battery.state_of_charge().value * 100.0,
            state: battery.state().into(),
            time_to_empty_secs: battery.time_to_empty().map(|t| t.value as u64),
            time_to_full_secs: battery.time_to_full().map(|t| t.value as u64),
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

#[cfg(feature = "ssr")]
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
    /// CPU architecture, e.g. "x86_64", "aarch64".
    pub cpu_arch: Option<String>,
    /// System uptime in seconds.
    pub uptime_secs: u64,
    /// Total number of running processes on the system.
    pub process_count: usize,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Serialize)]
pub struct StaticHostInfo {
    pub hostname: Option<String>,
    /// OS long name, e.g. "Ubuntu 24.04.1 LTS".
    pub os_name: Option<String>,
    /// Kernel version string.
    pub kernel_version: Option<String>,
    /// CPU architecture, e.g. "x86_64", "aarch64".
    pub cpu_arch: Option<String>,
}

#[cfg(feature = "ssr")]
pub struct Thresholds {
    pub cpu_warn: f32,
    pub memory_warn: f32,
    pub disk_warn: f32,
    pub battery_low: f32,
}

#[cfg(feature = "ssr")]
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
