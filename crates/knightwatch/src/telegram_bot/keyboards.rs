use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup};

use super::models::{DockerCallbackAction, ProcessCallbackAction, SystemResourcesCallbackAction};
use crate::system_resources::{RefreshMask, Thresholds};

pub fn main_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("📊 Process"),
            KeyboardButton::new("📊 Top Processes"),
        ],
        vec![
            KeyboardButton::new("🖼️ Screenshot"),
            KeyboardButton::new("🖥️ System Resources"),
        ],
        vec![
            KeyboardButton::new("🔧 Systemd"),
            KeyboardButton::new("🐳 Docker"),
        ],
        vec![
            KeyboardButton::new("⚙️ Settings"),
            KeyboardButton::new("📋 Help"),
        ],
        vec![
            KeyboardButton::new("🔑 Authenticate"),
            KeyboardButton::new("🔴 Stop"),
        ],
    ])
    .resize_keyboard()
}

pub fn top_processes_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("🔥 By CPU"),
            KeyboardButton::new("🧠 By Memory"),
        ],
        vec![
            KeyboardButton::new("💾 By Disk"),
            KeyboardButton::new("❌ Cancel"),
        ],
    ])
    .resize_keyboard()
}

pub fn systemd_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("📋 Systemd Overview"),
            KeyboardButton::new("🔴 Failed Units"),
        ],
        vec![KeyboardButton::new("🔍 Unit Status")],
        vec![KeyboardButton::new("❌ Cancel")],
    ])
    .resize_keyboard()
}

pub fn cancel_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([[KeyboardButton::new("❌ Cancel")]]).resize_keyboard()
}

pub fn settings_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![KeyboardButton::new("⏱️ Polling")],
        vec![KeyboardButton::new("❌ Cancel")],
    ])
    .resize_keyboard()
}

pub fn polling_subsystem_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("⏱️ Process Tracker Polling"),
            KeyboardButton::new("⏱️ Screen Capture Polling"),
        ],
        vec![
            KeyboardButton::new("⏱️ System Resources Polling"),
            KeyboardButton::new("⏱️ Systemd Polling"),
        ],
        vec![KeyboardButton::new("❌ Cancel")],
    ])
    .resize_keyboard()
}

pub fn subsystem_polling_keyboard(subsystem: &super::models::Subsystem) -> KeyboardMarkup {
    let label = subsystem.label();
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new(format!("⏸️ Pause {label}")),
            KeyboardButton::new(format!("▶️ Resume {label}")),
        ],
        vec![KeyboardButton::new(format!("🕐 Set {label} Interval"))],
        vec![KeyboardButton::new("❌ Cancel")],
    ])
    .resize_keyboard()
}

pub fn tracked_process_keyboard(root_pid: u32) -> InlineKeyboardMarkup {
    let mut rows = vec![vec![
        InlineKeyboardButton::callback(
            "➖ Untrack",
            ProcessCallbackAction::Untrack { pid: root_pid }.encode(),
        ),
        InlineKeyboardButton::callback(
            "🌲 Kill Tree",
            ProcessCallbackAction::KillTree { pid: root_pid }.encode(),
        ),
    ]];

    let signal_row = crate::process_tracker::ProcessSignal::get_supported_signals()
        .into_iter()
        .map(|signal| {
            let label = signal.to_string().to_uppercase();
            let data = ProcessCallbackAction::Signal {
                pid: root_pid,
                signal,
            }
            .encode();
            InlineKeyboardButton::callback(label, data)
        })
        .collect::<Vec<_>>();

    rows.push(signal_row);
    InlineKeyboardMarkup::new(rows)
}

/// Inline keyboard attached to a top-process message (shows Track).
pub fn top_process_keyboard(pid: u32) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[InlineKeyboardButton::callback(
        "➕ Track",
        ProcessCallbackAction::Track { pid }.encode(),
    )]])
}

pub fn system_resources_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([
        vec![
            InlineKeyboardButton::callback(
                "⚠️ Thresholds: Default",
                SystemResourcesCallbackAction::SetThresholds(Thresholds::default()).encode(),
            ),
            InlineKeyboardButton::callback(
                "⚠️ Thresholds: Strict",
                SystemResourcesCallbackAction::SetThresholds(Thresholds {
                    cpu_warn: 75.0,
                    memory_warn: 75.0,
                    disk_warn: 75.0,
                    battery_low: 25.0,
                })
                .encode(),
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                "🔄 Mask: All On",
                SystemResourcesCallbackAction::SetRefreshMask(RefreshMask::default()).encode(),
            ),
            InlineKeyboardButton::callback(
                "🔄 Mask: CPU+Mem Only",
                SystemResourcesCallbackAction::SetRefreshMask(RefreshMask {
                    cpu: true,
                    memory: true,
                    disks: false,
                    networks: false,
                    temperatures: false,
                    gpus: false,
                })
                .encode(),
            ),
        ],
    ])
}

pub fn docker_keyboard() -> KeyboardMarkup {
    KeyboardMarkup::new([
        vec![
            KeyboardButton::new("📋 Docker Containers"),
            KeyboardButton::new("🔥 By CPU (Docker)"),
        ],
        vec![
            KeyboardButton::new("🧠 By Memory (Docker)"),
            KeyboardButton::new("❌ Cancel"),
        ],
    ])
    .resize_keyboard()
}

/// Inline action buttons attached to a single container message.
pub fn docker_container_keyboard(id: &str) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([
        vec![
            InlineKeyboardButton::callback(
                "▶️ Start",
                DockerCallbackAction::Start { id: id.to_string() }.encode(),
            ),
            InlineKeyboardButton::callback(
                "⏹ Stop",
                DockerCallbackAction::Stop { id: id.to_string() }.encode(),
            ),
            InlineKeyboardButton::callback(
                "💀 Kill",
                DockerCallbackAction::Kill { id: id.to_string() }.encode(),
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                "🔄 Restart",
                DockerCallbackAction::Restart { id: id.to_string() }.encode(),
            ),
            InlineKeyboardButton::callback(
                "⏸️ Pause",
                DockerCallbackAction::Pause { id: id.to_string() }.encode(),
            ),
            InlineKeyboardButton::callback(
                "▶️ Unpause",
                DockerCallbackAction::Unpause { id: id.to_string() }.encode(),
            ),
        ],
    ])
}
