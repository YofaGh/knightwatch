use sysinfo::System;

use super::{enums::*, structs::*};

pub fn derive_health(
    cpu: &CpuSnapshot,
    memory: &MemorySnapshot,
    disks: &[DiskSnapshot],
    battery: &Option<BatterySnapshot>,
) -> SystemHealth {
    let cpu_crit = cpu.usage_percent >= 95.0;
    let mem_crit = memory.used_percent >= 95.0;
    let disk_crit = disks.iter().any(|d| d.used_percent >= 95.0);
    let bat_crit = battery
        .as_ref()
        .map(|b| b.state == BatteryState::Discharging && b.charge_percent <= 5.0)
        .unwrap_or(false);
    if cpu_crit || mem_crit || disk_crit || bat_crit {
        return SystemHealth::Critical;
    }
    let cpu_warn = cpu.usage_percent >= 80.0;
    let mem_warn = memory.used_percent >= 80.0;
    let disk_warn = disks.iter().any(|d| d.used_percent >= 90.0);
    let bat_warn = battery
        .as_ref()
        .map(|b| b.state == BatteryState::Discharging && b.charge_percent <= 20.0)
        .unwrap_or(false);
    if cpu_warn || mem_warn || disk_warn || bat_warn {
        return SystemHealth::Warning;
    }
    SystemHealth::Healthy
}

pub fn get_static_host_info() -> StaticHostInfo {
    StaticHostInfo {
        hostname: System::host_name(),
        os_name: System::long_os_version(),
        kernel_version: System::kernel_version(),
        cpu_arch: Some(System::cpu_arch()),
    }
}
