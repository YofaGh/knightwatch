use std::fmt::Write;

use super::display::TelegramDisplay;
use crate::{
    docker_tracker::{ContainerHealth, ContainerStatus, DockerTrackerEvent},
    process_tracker::ProcessTrackerEvent,
    system_resources::{BatteryState, SystemHealth, SystemResourcesEvent},
    systemd::{SystemdEvent, UnitActiveState},
};

const SPECIAL: &[char] = &[
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!', '\\',
];

pub fn format_process_tracker_event(event: &ProcessTrackerEvent) -> String {
    match event {
        ProcessTrackerEvent::InitialSnapshot { root, children } => {
            let mut msg = root.as_ref().map_or_else(
                || "🟢 *Initial Snapshot*\n\n*Root:*\n_none_".to_string(),
                |root| {
                    let root_info = TelegramDisplay(root);
                    format!("🟢 *Initial Snapshot*\n\n*Root:*\n{root_info}")
                },
            );
            if children.is_empty() {
                msg.push_str("\n\n*Children:* _none_");
            } else {
                let _ = write!(msg, "\n\n*Children* \\({}\\):", children.len());
                for child in children {
                    let child_info = TelegramDisplay(child);
                    let _ = write!(msg, "\n{child_info}\n");
                }
            }
            msg
        }
        ProcessTrackerEvent::ChildrenAppeared { pid, children } => {
            let mut msg = format!(
                "🆕 *New Children Appeared* \\({}\\)\n*Root:* `{pid}`",
                children.len()
            );
            for child in children {
                let info = TelegramDisplay(child);
                let _ = write!(msg, "\n{info}\n");
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
        ProcessTrackerEvent::ProcessKilled { pid, success } => {
            if *success {
                format!("✅ *Successfully killed process*\nPID: `{pid}`")
            } else {
                format!("✅ *Failed to killed process*\nPID: `{pid}`")
            }
        }
    }
}

pub fn format_system_resources_event(event: &SystemResourcesEvent) -> Option<String> {
    match event {
        SystemResourcesEvent::InitialSnapshot { snapshot } => {
            let display = TelegramDisplay(snapshot);
            Some(format!("🟢 *System Resouces Started*\n\n{display}"))
        }
        SystemResourcesEvent::Tick { .. } => None,
        SystemResourcesEvent::CpuThresholdExceeded {
            usage_percent,
            threshold,
        } => Some(format!(
            "⚠️ *CPU Threshold Exceeded*\n\
                 ├ Usage: `{usage_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`"
        )),
        SystemResourcesEvent::MemoryThresholdExceeded {
            used_percent,
            threshold,
        } => Some(format!(
            "⚠️ *Memory Threshold Exceeded*\n\
                 ├ Usage: `{used_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`"
        )),
        SystemResourcesEvent::DiskThresholdExceeded {
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
        SystemResourcesEvent::BatteryLow {
            charge_percent,
            threshold,
        } => Some(format!(
            "🪫 *Battery Low*\n\
                 ├ Charge: `{charge_percent:.1}%`\n\
                 └ Threshold: `{threshold:.1}%`"
        )),
        SystemResourcesEvent::BatteryStateChanged { state } => {
            let (emoji, label) = match state {
                BatteryState::Charging => ("⚡", "Charging"),
                BatteryState::Discharging => ("🔋", "Discharging"),
                BatteryState::Full => ("✅", "Full"),
                BatteryState::Unknown => ("🔌", "Unknown"),
            };
            Some(format!(
                "{emoji} *Battery State Changed*\n└ State: `{label}`"
            ))
        }
    }
}

pub fn format_systemd_event(event: &SystemdEvent) -> Option<String> {
    match event {
        SystemdEvent::Tick { .. } => None,
        SystemdEvent::InitialSnapshot { snapshot } => {
            let display = TelegramDisplay(snapshot);
            Some(format!("🟢 *Systemd Started*\n\n{display}"))
        }
        SystemdEvent::UnitFailed {
            unit_name,
            previous_state,
        } => {
            let prev = previous_state.as_str();
            Some(format!(
                "🔴 *Unit Failed*\n\
                 ├ Unit: `{unit}`\n\
                 └ Was: `{prev}`",
                unit = escape_mdv2(unit_name),
            ))
        }
        SystemdEvent::UnitRecovered { unit_name } => Some(format!(
            "✅ *Unit Recovered*\n\
             └ Unit: `{unit}`",
            unit = escape_mdv2(unit_name),
        )),
        SystemdEvent::UnitAppeared { unit } => {
            Some(format!("🆕 *Unit Appeared*\n{}", TelegramDisplay(unit)))
        }
        SystemdEvent::UnitDisappeared { unit_name } => Some(format!(
            "👻 *Unit Disappeared*\n\
             └ Unit: `{unit}`",
            unit = escape_mdv2(unit_name),
        )),
    }
}

pub fn format_docker_tracker_event(event: &DockerTrackerEvent) -> String {
    match event {
        DockerTrackerEvent::InitialSnapshot { containers } => {
            if containers.is_empty() {
                return "🐳 *Docker Started* — no containers found\\.".to_string();
            }
            let body = containers
                .iter()
                .map(|c| TelegramDisplay(c).to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            format!("🐳 *Docker Started* \\({}\\)\n\n{body}", containers.len())
        }
        DockerTrackerEvent::ContainersAppeared { containers } => {
            let body = containers
                .iter()
                .map(|c| TelegramDisplay(c).to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            format!(
                "🆕 *Containers Appeared* \\({}\\)\n\n{body}",
                containers.len()
            )
        }
        DockerTrackerEvent::ContainersDisappeared { containers } => {
            let lines = containers
                .iter()
                .map(|c| {
                    format!(
                        "• `{}` `{}`",
                        escape_mdv2(&c.name),
                        escape_mdv2(&c.short_id)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "👻 *Containers Disappeared* \\({}\\)\n{lines}",
                containers.len()
            )
        }
        DockerTrackerEvent::ContainerStatusChanged {
            container,
            previous,
        } => {
            let prev = escape_mdv2(&previous.to_string());
            let now = escape_mdv2(&container.status.to_string());
            format!(
                "🔄 *Container Status Changed*\n\
                 ├ Name: `{name}`\n\
                 ├ Was: `{prev}`\n\
                 └ Now: `{now}`",
                name = escape_mdv2(&container.name),
            )
        }
        DockerTrackerEvent::ContainerHealthChanged {
            container,
            previous,
        } => {
            let prev = escape_mdv2(&previous.to_string());
            let now = escape_mdv2(&container.health.to_string());
            format!(
                "🏥 *Container Health Changed*\n\
                 ├ Name: `{name}`\n\
                 ├ Was: `{prev}`\n\
                 └ Now: `{now}`",
                name = escape_mdv2(&container.name),
            )
        }
        DockerTrackerEvent::ContainerOomKilled { id, name } => format!(
            "💥 *Container OOM Killed*\n\
             ├ Name: `{name}`\n\
             └ ID: `{id}`",
            name = escape_mdv2(name),
            id = escape_mdv2(id),
        ),
        DockerTrackerEvent::CommandExecuted {
            user,
            action,
            success,
            error,
        } => {
            let user_str = escape_mdv2(&format!("{user:?}"));
            let action_str = escape_mdv2(&action.to_string());

            if *success {
                format!(
                    "⚙️ *Command Executed*\n\
                     ├ User: `{user_str}`\n\
                     └ Action: `{action_str}`"
                )
            } else {
                let err_str = escape_mdv2(error.as_deref().unwrap_or("unknown error"));
                format!(
                    "⚠️ *Command Failed*\n\
                     ├ User: `{user_str}`\n\
                     ├ Action: `{action_str}`\n\
                     └ Error: `{err_str}`"
                )
            }
        }
    }
}

pub const fn unit_state_emoji(state: &UnitActiveState) -> &'static str {
    match state {
        UnitActiveState::Active => "🟢",
        UnitActiveState::Reloading => "🔄",
        UnitActiveState::Inactive => "⚫",
        UnitActiveState::Failed => "🔴",
        UnitActiveState::Activating => "🟡",
        UnitActiveState::Deactivating => "🟠",
    }
}

pub const fn container_status_emoji(status: &ContainerStatus) -> &'static str {
    match status {
        ContainerStatus::Running => "🟢",
        ContainerStatus::Paused => "🟡",
        ContainerStatus::Restarting => "🔄",
        ContainerStatus::Exited => "⚫",
        ContainerStatus::Dead => "🔴",
        ContainerStatus::Created => "🔵",
        ContainerStatus::Removing | ContainerStatus::Stopping => "🟠",
        ContainerStatus::Unknown(_) => "❓",
    }
}

pub const fn container_health_emoji(health: &ContainerHealth) -> &'static str {
    match health {
        ContainerHealth::Healthy => "💚",
        ContainerHealth::Unhealthy => "❤️",
        ContainerHealth::Starting => "🤍",
        ContainerHealth::None | ContainerHealth::Unknown => "🩶",
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

pub const fn health_emoji(health: &SystemHealth) -> &'static str {
    match health {
        SystemHealth::Healthy => "✅",
        SystemHealth::Warning => "⚠️",
        SystemHealth::Critical => "🔴",
    }
}
