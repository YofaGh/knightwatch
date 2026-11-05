use nvml_wrapper::Nvml;
use sysinfo::{Components, CpuRefreshKind, Disks, Networks, System};

use kw_types::resources::{
    BatterySnapshot, CpuSnapshot, DiskSnapshot, GpuSnapshot, HostInfo, MemorySnapshot,
    NetworkSnapshot, RefreshMask, ThermalSnapshot,
};
use kw_utils::conv::u64_ratio_percent_f32;

use super::system::StaticHostInfo;

/// What the tracker needs from the OS, so its alarm logic can run on a fake in tests.
pub trait ResourceSource: Send {
    /// Refresh the subsystems the mask enables. The rest keep their last readings.
    fn refresh(&mut self, mask: &RefreshMask);
    fn cpu(&self) -> CpuSnapshot;
    fn memory(&self) -> MemorySnapshot;
    fn disks(&self) -> Vec<DiskSnapshot>;
    fn networks(&self) -> Vec<NetworkSnapshot>;
    fn temperatures(&self) -> Vec<ThermalSnapshot>;
    fn gpus(&self) -> Vec<GpuSnapshot>;
    fn battery(&self) -> Option<BatterySnapshot>;
    fn host(&self) -> HostInfo;
}

pub struct SysinfoSource {
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    nvml: Option<Nvml>,
    static_host_info: StaticHostInfo,
    uptime_baseline: u64,
    uptime_started: std::time::Instant,
}

impl SysinfoSource {
    pub fn new() -> Self {
        Self {
            sys: System::new_with_specifics(
                sysinfo::RefreshKind::nothing()
                    .with_cpu(CpuRefreshKind::everything())
                    .with_memory(sysinfo::MemoryRefreshKind::everything())
                    .with_processes(sysinfo::ProcessRefreshKind::nothing()),
            ),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            nvml: Nvml::init().ok(),
            static_host_info: StaticHostInfo::new(),
            uptime_baseline: System::uptime(),
            uptime_started: std::time::Instant::now(),
        }
    }
}

impl ResourceSource for SysinfoSource {
    fn refresh(&mut self, mask: &RefreshMask) {
        // sysinfo requires two CPU ticks to produce non-zero usage numbers.
        if mask.cpu {
            self.sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        }
        if mask.memory {
            self.sys.refresh_memory();
        }
        if mask.disks {
            self.disks.refresh(false);
        }
        if mask.networks {
            self.networks.refresh(false);
        }
        if mask.temperatures {
            self.components.refresh(false);
        }
    }

    fn cpu(&self) -> CpuSnapshot {
        let cpus = self.sys.cpus();
        let usage_percent = self.sys.global_cpu_usage();
        let cores = cpus.iter().map(Into::into).collect();
        let frequency_mhz = cpus.first().map_or(0, sysinfo::Cpu::frequency);
        let brand = cpus
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default();
        CpuSnapshot {
            usage_percent,
            cores,
            frequency_mhz,
            brand,
            physical_core_count: System::physical_core_count(),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            load_avg: Some(System::load_average().into()),
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            load_avg: None,
        }
    }

    fn memory(&self) -> MemorySnapshot {
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let used_percent = if total > 0 {
            u64_ratio_percent_f32(used, total)
        } else {
            0.0
        };

        let swap_total = self.sys.total_swap();
        let swap_used = self.sys.used_swap();
        let swap_used_percent = if swap_total > 0 {
            Some(u64_ratio_percent_f32(swap_used, swap_total))
        } else {
            None
        };

        MemorySnapshot {
            total_bytes: total,
            used_bytes: used,
            available_bytes: self.sys.available_memory(),
            free_bytes: self.sys.free_memory(),
            used_percent,
            swap_total_bytes: swap_total,
            swap_used_bytes: swap_used,
            swap_free_bytes: self.sys.free_swap(),
            swap_used_percent,
        }
    }

    fn disks(&self) -> Vec<DiskSnapshot> {
        self.disks.iter().map(Into::into).collect()
    }

    fn networks(&self) -> Vec<NetworkSnapshot> {
        self.networks.iter().map(Into::into).collect()
    }

    fn temperatures(&self) -> Vec<ThermalSnapshot> {
        self.components.iter().map(Into::into).collect()
    }

    fn gpus(&self) -> Vec<GpuSnapshot> {
        let Some(ref nvml) = self.nvml else {
            return vec![];
        };
        let devices_count = nvml.device_count().unwrap_or(0);
        (0..devices_count)
            .filter_map(|i| nvml.device_by_index(i).ok().map(Into::into))
            .collect()
    }

    fn battery(&self) -> Option<BatterySnapshot> {
        starship_battery::Manager::new()
            .ok()?
            .batteries()
            .ok()?
            .next()?
            .ok()
            .map(Into::into)
    }

    fn host(&self) -> HostInfo {
        HostInfo {
            hostname: self.static_host_info.hostname.clone(),
            os_name: self.static_host_info.os_name.clone(),
            kernel_version: self.static_host_info.kernel_version.clone(),
            cpu_arch: self.static_host_info.cpu_arch.clone(),
            uptime_secs: self
                .uptime_baseline
                .saturating_add(self.uptime_started.elapsed().as_secs()),
            process_count: self.sys.processes().len(),
        }
    }
}
