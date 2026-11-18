use kw_types::resources::{
    BatterySnapshot, CpuSnapshot, DiskSnapshot, MemorySnapshot, SystemHealth, Thresholds,
};

/// Usage at or above this is critical, unless the warning level is set higher.
const CRITICAL_PERCENT: f32 = 95.0;
/// Battery charge at or below this is critical, unless the low level is set lower.
const CRITICAL_BATTERY: f32 = 5.0;

/// Warning at the configured thresholds (the same ones the alarms use), critical
/// at 95% usage or 5% battery, but never before the warning level. A NaN reading
/// makes it `Unknown` unless something else is already elevated.
pub fn derive_health(
    cpu: &CpuSnapshot,
    memory: &MemorySnapshot,
    disks: &[DiskSnapshot],
    battery: Option<&BatterySnapshot>,
    thresholds: &Thresholds,
) -> SystemHealth {
    // a charging or full battery's charge does not matter, not even a NaN one
    let charge = battery
        .filter(|b| b.state.may_run_down())
        .map(|b| b.charge_percent);
    let discharging_at_or_below = |limit: f32| charge.is_some_and(|c| c <= limit);
    let over = |cpu_at: f32, memory_at: f32, disk_at: f32| {
        cpu.usage_percent >= cpu_at
            || memory.used_percent >= memory_at
            || disks.iter().any(|d| d.used_percent >= disk_at)
    };
    if over(
        CRITICAL_PERCENT.max(thresholds.cpu_warn),
        CRITICAL_PERCENT.max(thresholds.memory_warn),
        CRITICAL_PERCENT.max(thresholds.disk_warn),
    ) || discharging_at_or_below(CRITICAL_BATTERY.min(thresholds.battery_low))
    {
        return SystemHealth::Critical;
    }
    if over(
        thresholds.cpu_warn,
        thresholds.memory_warn,
        thresholds.disk_warn,
    ) || discharging_at_or_below(thresholds.battery_low)
    {
        return SystemHealth::Warning;
    }
    let unread = cpu.usage_percent.is_nan()
        || memory.used_percent.is_nan()
        || disks.iter().any(|d| d.used_percent.is_nan())
        || charge.is_some_and(f32::is_nan);
    if unread {
        return SystemHealth::Unknown;
    }
    SystemHealth::Healthy
}
