pub fn compute_cpu_percent(stats: &bollard::config::ContainerStatsResponse) -> f64 {
    let cpu = &stats.cpu_stats;
    let precpu = &stats.precpu_stats;

    let cpu_total_usage = cpu
        .as_ref()
        .and_then(|cpu| cpu.cpu_usage.as_ref())
        .and_then(|cu| cu.total_usage);

    let precpu_total_usage = precpu
        .as_ref()
        .and_then(|cpu| cpu.cpu_usage.as_ref())
        .and_then(|cu| cu.total_usage);

    let cpu_delta = if let (Some(cpu_total_usage), Some(precpu_total_usage)) =
        (cpu_total_usage, precpu_total_usage)
    {
        cpu_total_usage.saturating_sub(precpu_total_usage)
    } else {
        0
    };

    let system_cpu_usage = cpu.as_ref().and_then(|cpu| cpu.system_cpu_usage);
    let presystem_total_usage = precpu.as_ref().and_then(|cpu| cpu.system_cpu_usage);

    let system_delta = if let (Some(system_cpu_usage), Some(presystem_total_usage)) =
        (system_cpu_usage, presystem_total_usage)
    {
        system_cpu_usage.saturating_sub(presystem_total_usage)
    } else {
        0
    };

    if system_delta == 0 {
        return 0.0;
    }

    let num_cpus = cpu
        .as_ref()
        .and_then(|cpu| cpu.online_cpus)
        .unwrap_or_else(|| {
            cpu.as_ref()
                .and_then(|cpu| cpu.cpu_usage.as_ref())
                .and_then(|cpu_usage| cpu_usage.percpu_usage.as_ref())
                .map(|v| v.len() as u32)
                .unwrap_or(1)
        });

    (cpu_delta as f64 / system_delta as f64) * num_cpus as f64 * 100.0
}
