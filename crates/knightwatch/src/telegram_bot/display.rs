use kw_utils::{format_bytes, format_time};

use super::utils::escape_mdv2;
use crate::{docker_tracker, process_tracker, systemd};

pub struct TelegramDisplay<'a, T>(pub &'a T);

impl std::fmt::Display for TelegramDisplay<'_, process_tracker::ProcessSnapshot> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.0;
        write!(
            f,
            "🔹 *{name}* `\\(PID {pid}\\)`\n   ├ State: `{state}`\n   ├ CPU: `{cpu:.1}%`\n   └ Mem: `{mem}`\n   └ Disk: `{disk}`",
            pid = s.pid,
            name = escape_mdv2(&s.name),
            state = escape_mdv2(&s.state.to_string()),
            cpu = s.cpu_usage,
            mem = escape_mdv2(&format_bytes(s.memory_bytes)),
            disk = escape_mdv2(&format_bytes(s.disk_usage)),
        )?;
        #[cfg(target_os = "linux")]
        {
            if let Some(cwd) = &s.cwd {
                write!(f, "\n   ├ CWD: `{}`", escape_mdv2(cwd))?;
            }
            write!(f, "\n   ├ FDs: `{}`", s.open_files.len())?;
            if let Some(io) = &s.io_stats {
                write!(
                    f,
                    "\n   ├ I/O Read: `{}` / Write: `{}`",
                    escape_mdv2(&io.read_bytes.to_string()),
                    escape_mdv2(&io.write_bytes.to_string()),
                )?;
            }
            if !s.cmdline.is_empty() {
                let cmd = s.cmdline.join(" ");
                write!(f, "\n   └ CMD: `{}`", escape_mdv2(&cmd))?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for TelegramDisplay<'_, process_tracker::ProcessTree> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = self.0; // Header
        let status_icon = if t.work_done { "✅" } else { "⏳" };
        writeln!(f, "{status_icon} *Process Tree*\n🕐 `{}`\n", t.timestamp)?;

        // Root process
        writeln!(f, "*Root Process*")?;
        match &t.root {
            Some(root) => writeln!(f, "{}", TelegramDisplay(root))?,
            None => writeln!(f, "_No root process_")?,
        }

        // Children
        if t.child_count == 0 {
            writeln!(f, "\n*Children:* _none_")?;
        } else {
            writeln!(f, "\n*Children* \\({}\\):", t.child_count)?;
            for child in &t.children {
                writeln!(f, "{}\n", TelegramDisplay(child))?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for TelegramDisplay<'_, crate::system_resources::SystemSnapshot> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.0;

        // ── Header ──────────────────────────────────────────────────────────
        writeln!(
            f,
            "🖥️ *{host}* — `{ts}`",
            host = escape_mdv2(s.host.hostname.as_deref().unwrap_or("unknown")),
            ts = escape_mdv2(&s.timestamp),
        )?;

        // ── Host meta ───────────────────────────────────────────────────────
        writeln!(
            f,
            "├ OS: `{os}`\n\
             ├ Kernel: `{kernel}`\n\
             ├ Arch: `{arch}`\n\
             ├ Uptime: `{uptime}`\n\
             └ Processes: `{procs}`",
            os = escape_mdv2(s.host.os_name.as_deref().unwrap_or("?")),
            kernel = escape_mdv2(s.host.kernel_version.as_deref().unwrap_or("?")),
            arch = escape_mdv2(s.host.cpu_arch.as_deref().unwrap_or("?")),
            uptime = format_time(s.host.uptime_secs),
            procs = s.host.process_count,
        )?;

        // ── CPU ─────────────────────────────────────────────────────────────
        writeln!(
            f,
            "\n🔲 *CPU* — `{brand}`\n\
             ├ Usage: `{usage:.1}%`\n\
             └ Freq: `{freq} MHz`",
            brand = escape_mdv2(&s.cpu.brand),
            usage = s.cpu.usage_percent,
            freq = s.cpu.frequency_mhz,
        )?;

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Some(load_avg) = &s.cpu.load_avg {
            writeln!(
                f,
                "   └ Load avg: `{:.2}` / `{:.2}` / `{:.2}`",
                load_avg.one, load_avg.five, load_avg.fifteen,
            )?;
        }

        // ── Memory ──────────────────────────────────────────────────────────
        write!(
            f,
            "\n🧠 *Memory*\n\
             ├ RAM: `{used}` / `{total}` \\(`{pct:.1}%`\\)\n\
             └ Swap: `{sused}` / `{stotal}`",
            used = escape_mdv2(&format_bytes(s.memory.used_bytes)),
            total = escape_mdv2(&format_bytes(s.memory.total_bytes)),
            pct = s.memory.used_percent,
            sused = escape_mdv2(&format_bytes(s.memory.swap_used_bytes)),
            stotal = escape_mdv2(&format_bytes(s.memory.swap_total_bytes)),
        )?;
        if let Some(swap_pct) = s.memory.swap_used_percent {
            write!(f, " \\(`{swap_pct:.1}%`\\)")?;
        }
        writeln!(f)?;

        // ── Disks ───────────────────────────────────────────────────────────
        if !s.disks.is_empty() {
            write!(f, "\n💾 *Disks*\n")?;
            let last = s.disks.len().saturating_sub(1);
            for (i, disk) in s.disks.iter().enumerate() {
                let connector = if i == last { "└" } else { "├" };
                writeln!(
                    f,
                    "{con} `{mount}` \\({fs}, {kind:?}\\): \
                     `{used_pct:.1}%` used \
                     \\(`{used}` / `{total}`\\)",
                    con = connector,
                    mount = escape_mdv2(&disk.mount_point),
                    fs = escape_mdv2(&disk.file_system),
                    kind = disk.kind,
                    used_pct = disk.used_percent,
                    used = escape_mdv2(&format_bytes(disk.used_bytes)),
                    total = escape_mdv2(&format_bytes(disk.total_bytes)),
                )?;
            }
        }

        // ── Networks ────────────────────────────────────────────────────────
        if !s.networks.is_empty() {
            writeln!(f, "\n🌐 *Network*")?;
            let last = s.networks.len().saturating_sub(1);
            for (i, net) in s.networks.iter().enumerate() {
                let connector = if i == last { "└" } else { "├" };
                writeln!(
                    f,
                    "{con} `{iface}`: ↓ `{rx}/s` ↑ `{tx}/s`",
                    con = connector,
                    iface = escape_mdv2(&net.interface),
                    rx = escape_mdv2(&format_bytes(net.rx_bytes_per_sec)),
                    tx = escape_mdv2(&format_bytes(net.tx_bytes_per_sec)),
                )?;
            }
        }

        // ── GPUs ────────────────────────────────────────────────────────────
        if !s.gpus.is_empty() {
            writeln!(f, "\n🎮 *GPU*")?;
            let last = s.gpus.len().saturating_sub(1);
            for (i, gpu) in s.gpus.iter().enumerate() {
                let connector = if i == last { "└" } else { "├" };
                writeln!(f, "{connector} *{}*", escape_mdv2(&gpu.name))?;

                if let Some(usage) = gpu.usage_percent {
                    writeln!(f, "   ├ Core: `{usage:.1}%`")?;
                }
                if let (Some(used), Some(total)) = (gpu.vram_used_bytes, gpu.vram_total_bytes) {
                    let pct_str = gpu
                        .vram_used_percent
                        .map(|p| format!(" \\(`{p:.1}%`\\)"))
                        .unwrap_or_default();
                    writeln!(
                        f,
                        "   ├ VRAM: `{used}` / `{total}`{pct_str}",
                        used = escape_mdv2(&format_bytes(used)),
                        total = escape_mdv2(&format_bytes(total)),
                    )?;
                }
                if let Some(temp) = gpu.temperature_celsius {
                    writeln!(f, "   ├ Temp: `{temp:.1}°C`")?;
                }
                if let (Some(draw), Some(limit)) = (gpu.power_draw_watts, gpu.power_limit_watts) {
                    writeln!(f, "   ├ Power: `{draw:.1}W` / `{limit:.1}W`")?;
                } else if let Some(draw) = gpu.power_draw_watts {
                    writeln!(f, "   ├ Power: `{draw:.1}W`")?;
                }
                if !gpu.fan_speed_percent.is_empty() {
                    let fans = gpu
                        .fan_speed_percent
                        .iter()
                        .map(|f| format!("`{f:.0}%`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(
                        f,
                        "   └ Fan{}: {fans}",
                        if gpu.fan_speed_percent.len() > 1 {
                            "s"
                        } else {
                            ""
                        }
                    )?;
                }
            }
        }

        // ── Battery ─────────────────────────────────────────────────────────
        if let Some(bat) = &s.battery {
            writeln!(
                f,
                "\n🔋 *Battery* — `{charge:.1}%` \\({state:?}\\)",
                charge = bat.charge_percent,
                state = bat.state,
            )?;
            if let Some(secs) = bat.time_to_empty_secs {
                writeln!(f, "   ├ Time to empty: `{}`", format_time(secs))?;
            }
            if let Some(secs) = bat.time_to_full_secs {
                writeln!(f, "   ├ Time to full: `{}`", format_time(secs))?;
            }
            if let Some(watts) = bat.power_draw_watts {
                writeln!(f, "   ├ Draw: `{watts:.1}W`")?;
            }
            if let Some(health) = bat.health_percent {
                writeln!(f, "   ├ Health: `{health:.1}%`")?;
            }
            if let Some(cycles) = bat.cycle_count {
                writeln!(f, "   └ Cycles: `{cycles}`")?;
            }
        }

        // ── Thermals ────────────────────────────────────────────────────────
        if !s.temperatures.is_empty() {
            writeln!(f, "\n🌡️ *Thermals*")?;
            let last = s.temperatures.len().saturating_sub(1);
            for (i, t) in s.temperatures.iter().enumerate() {
                let connector = if i == last { "└" } else { "├" };
                let temp_str = t
                    .temperature_celsius
                    .map_or_else(|| "`n/a`".into(), |v| format!("`{v:.1}°C`"));
                let crit_str = t
                    .temperature_critical_celsius
                    .map(|v| format!(" \\(crit `{v:.1}°C`\\)"))
                    .unwrap_or_default();
                writeln!(
                    f,
                    "{connector} `{label}`: {temp}{crit}",
                    label = escape_mdv2(&t.label),
                    temp = temp_str,
                    crit = crit_str,
                )?;
            }
        }

        // ── Health summary ──────────────────────────────────────────────────
        write!(
            f,
            "\n{emoji} *Health*: `{health:?}`",
            emoji = super::utils::health_emoji(&s.health),
            health = s.health,
        )?;

        Ok(())
    }
}

impl std::fmt::Display for TelegramDisplay<'_, kw_types::resources::AlarmSnapshot> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.0;

        let mut items: Vec<(&str, &kw_types::resources::AlarmStatus)> = vec![
            ("CPU", &s.cpu),
            ("Memory", &s.memory),
            ("Battery Low", &s.battery_low),
        ];
        for (mount, status) in &s.disks {
            items.push((mount.as_str(), status));
        }

        let active_count = items.iter().filter(|(_, st)| st.active).count();
        let emoji = if active_count == 0 { "✅" } else { "🚨" };

        writeln!(f, "{emoji} *Alarms* — `{active_count}` active")?;

        let last = items.len().saturating_sub(1);
        for (i, (label, status)) in items.iter().enumerate() {
            let connector = if i == last { "└" } else { "├" };
            let icon = if status.active { "🔴" } else { "⚫" };
            write!(f, "{connector} {icon} `{}`", escape_mdv2(label))?;
            if status.active
                && let Some(since) = status.since
                && let Ok(elapsed) = since.elapsed()
            {
                write!(
                    f,
                    " — active `{}`",
                    escape_mdv2(&format_time(elapsed.as_secs())),
                )?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

impl std::fmt::Display for TelegramDisplay<'_, systemd::UnitSnapshot> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let u = self.0;
        let emoji = super::utils::unit_state_emoji(&u.active_state);

        writeln!(
            f,
            "{emoji} *{name}*  `{sub}`",
            name = escape_mdv2(&u.unit_name),
            sub = escape_mdv2(&u.sub_state),
        )?;
        if !u.description.is_empty() && u.description != u.unit_name {
            writeln!(f, "   ├ `{}`", escape_mdv2(&u.description))?;
        }
        if let Some(pid) = u.main_pid {
            writeln!(f, "   ├ PID: `{pid}`")?;
        }
        if let Some(mem) = u.memory_bytes {
            writeln!(f, "   ├ Mem: `{}`", escape_mdv2(&format_bytes(mem)))?;
        }
        if let Some(cpu_ns) = u.cpu_usage_ns {
            let cpu_secs = std::time::Duration::from_nanos(cpu_ns).as_secs_f64();
            writeln!(f, "   ├ CPU time: `{cpu_secs:.2}s`")?;
        }
        if let Some(restarts) = u.restart_count {
            writeln!(f, "   ├ Restarts: `{restarts}`")?;
        }
        if let Some(since) = &u.since {
            writeln!(f, "   ├ Since: `{}`", escape_mdv2(since))?;
        }
        if let Some(path) = &u.fragment_path {
            write!(f, "   └ File: `{}`", escape_mdv2(path))?;
        }

        Ok(())
    }
}

impl std::fmt::Display for TelegramDisplay<'_, systemd::SystemdSnapshot> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.0;

        writeln!(f, "🔧 *Systemd* — `{ts}`", ts = escape_mdv2(&s.timestamp))?;
        writeln!(
            f,
            "├ 🟢 Active: `{active}`\n\
             ├ ⚫ Inactive: `{inactive}`\n\
             └ 🔴 Failed: `{failed}`",
            active = s.active_count,
            inactive = s.inactive_count,
            failed = s.failed_count,
        )?;
        if s.failed_count > 0 {
            let failed_units: Vec<_> = s
                .units
                .iter()
                .filter(|u| u.active_state == systemd::UnitActiveState::Failed)
                .collect();
            if !failed_units.is_empty() {
                writeln!(f, "\n🔴 *Failed Units:*")?;
                for unit in failed_units {
                    writeln!(
                        f,
                        "• `{name}` — {sub}",
                        name = escape_mdv2(&unit.unit_name),
                        sub = escape_mdv2(&unit.sub_state),
                    )?;
                }
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for TelegramDisplay<'_, crate::docker_tracker::ContainerSnapshot> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let c = self.0;
        let status_emoji = super::utils::container_status_emoji(&c.status);
        let health_emoji = super::utils::container_health_emoji(&c.health);
        // Header
        writeln!(
            f,
            "{status_emoji} *{name}* `{short_id}`",
            name = escape_mdv2(&c.name),
            short_id = escape_mdv2(&c.short_id),
        )?;
        writeln!(f, "   ├ Image: `{}`", escape_mdv2(&c.image))?;
        writeln!(
            f,
            "   ├ Status: `{}`",
            escape_mdv2(&format!("{:?}", c.status).to_lowercase()),
        )?;
        // Health — only show if a HEALTHCHECK is defined
        if c.health != docker_tracker::ContainerHealth::None {
            writeln!(
                f,
                "   ├ Health: {health_emoji} `{}`",
                escape_mdv2(&format!("{:?}", c.health).to_lowercase()),
            )?;
        }
        // Stats — optional block
        match &c.stats {
            None => {
                write!(f, "   └ Stats: `unavailable`")?;
            }
            Some(s) => {
                writeln!(f, "   ├ CPU: `{:.1}%`", s.cpu_percent)?;

                let mem_str = s.memory_percent.map_or_else(
                    || format!("`{}`", escape_mdv2(&format_bytes(s.memory_bytes))),
                    |pct| {
                        format!(
                            "`{}` / `{}` \\(`{:.1}%`\\)",
                            escape_mdv2(&format_bytes(s.memory_bytes)),
                            escape_mdv2(&format_bytes(s.memory_limit_bytes)),
                            pct * 100.0,
                        )
                    },
                );
                writeln!(f, "   ├ Mem: {mem_str}")?;

                writeln!(
                    f,
                    "   ├ Net: ↓ `{}` ↑ `{}`",
                    escape_mdv2(&format_bytes(s.net_rx_bytes)),
                    escape_mdv2(&format_bytes(s.net_tx_bytes)),
                )?;
                writeln!(
                    f,
                    "   ├ Block: R `{}` W `{}`",
                    escape_mdv2(&format_bytes(s.block_read_bytes)),
                    escape_mdv2(&format_bytes(s.block_write_bytes)),
                )?;
                write!(f, "   └ PIDs: `{}`", s.pid_count)?;
            }
        }
        Ok(())
    }
}
