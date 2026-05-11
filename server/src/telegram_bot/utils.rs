use super::models::TelegramDisplay;
use crate::{
    process_tracker::ProcessTrackerEvent,
    system_monitor::{BatteryState, SystemHealth, SystemMonitorEvent},
};

const SPECIAL: &[char] = &[
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!', '\\',
];

pub fn format_process_tracker_event(event: &ProcessTrackerEvent) -> String {
    match event {
        ProcessTrackerEvent::InitialSnapshot { root, children } => {
            let root_info = TelegramDisplay(root);
            let mut msg = format!("🟢 *Initial Snapshot*\n\n*Root:*\n{root_info}");
            if children.is_empty() {
                msg.push_str("\n\n*Children:* _none_");
            } else {
                msg.push_str(&format!("\n\n*Children* ({}):", children.len()));
                for child in children {
                    let child_info = TelegramDisplay(child);
                    msg.push_str(&format!("\n{child_info}\n"));
                }
            }
            msg
        }
        ProcessTrackerEvent::ChildrenAppeared { pid, children } => {
            let mut msg = format!(
                "🆕 *New Children Appeared* ({})\n*Root:* `{pid}`",
                children.len()
            );
            for child in children {
                let info = TelegramDisplay(child);
                msg.push_str(&format!("\n{info}\n"));
            }
            msg
        }
        ProcessTrackerEvent::ChildrenExited { pid, children } => {
            let pid_list = children
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("🔴 *Children Exited*\n*Root:* `{pid}`\nchildren PIDs: {pid_list}")
        }
        ProcessTrackerEvent::AllChildrenGone { pid } => {
            format!(
                "✅ *All children have exited*\n*Root:* `{pid}`\nThe root process may still be running\\."
            )
        }
        ProcessTrackerEvent::RootExited { pid } => {
            format!("💀 *Root Process Exited*\nPID: `{pid}`")
        }
        ProcessTrackerEvent::WorkComplete { pid } => {
            format!("✅ *Work Complete*\nPID: `{pid}`")
        }
    }
}

pub fn format_system_monitor_event(event: &SystemMonitorEvent) -> Option<String> {
    match event {
        SystemMonitorEvent::InitialSnapshot { snapshot } => {
            let display = TelegramDisplay(snapshot);
            Some(format!("🟢 *System Monitor Started*\n\n{display}"))
        }
        SystemMonitorEvent::Tick { .. } => None,
        SystemMonitorEvent::CpuThresholdExceeded {
            usage_percent,
            threshold,
        } => Some(format!(
            "⚠️ *CPU Threshold Exceeded*\n\
                 ├ Usage: `{usage_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`"
        )),
        SystemMonitorEvent::MemoryThresholdExceeded {
            used_percent,
            threshold,
        } => Some(format!(
            "⚠️ *Memory Threshold Exceeded*\n\
                 ├ Usage: `{used_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`"
        )),
        SystemMonitorEvent::DiskThresholdExceeded {
            mount_point,
            used_percent,
            threshold,
        } => Some(format!(
            "⚠️ *Disk Threshold Exceeded*\n\
                 ├ Mount: `{mount}`\n\
                 ├ Usage: `{used_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`",
            mount = escape_mdv2(mount_point),
        )),
        SystemMonitorEvent::BatteryLow {
            charge_percent,
            threshold,
        } => Some(format!(
            "🪫 *Battery Low*\n\
                 ├ Charge: `{charge_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`"
        )),
        SystemMonitorEvent::BatteryStateChanged { state } => {
            let (emoji, label) = match state {
                BatteryState::Charging => ("⚡", "Charging"),
                BatteryState::Discharging => ("🔋", "Discharging"),
                BatteryState::Full => ("✅", "Full"),
                _ => ("🔌", "Unknown"),
            };
            Some(format!(
                "{emoji} *Battery State Changed*\n└ State: `{label}`"
            ))
        }
    }
}

pub fn escape_mdv2(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn health_emoji(health: &SystemHealth) -> &'static str {
    match health {
        SystemHealth::Healthy => "✅",
        SystemHealth::Warning => "⚠️",
        SystemHealth::Critical => "🔴",
    }
}
