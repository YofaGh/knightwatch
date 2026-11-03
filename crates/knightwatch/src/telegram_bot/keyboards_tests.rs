#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup, KeyboardMarkup,
};

use super::{
    keyboards::{
        cancel_keyboard, docker_container_keyboard, docker_keyboard, main_keyboard,
        polling_subsystem_keyboard, settings_keyboard, subsystem_polling_keyboard,
        system_resources_keyboard, systemd_keyboard, top_process_keyboard, top_processes_keyboard,
        tracked_process_keyboard, unit_control_keyboard,
    },
    models::{
        DockerCallbackAction, ProcessCallbackAction, Subsystem, SystemResourcesCallbackAction,
        SystemdCallbackAction,
    },
};

use crate::{
    process_tracker::ProcessSignal,
    system_resources::{RefreshMask, Thresholds},
    systemd::ServiceAction,
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

const MAX_CALLBACK_DATA_BYTES: usize = 64;
const MAX_BUTTONS_PER_ROW: usize = 8;

fn rows(k: &KeyboardMarkup) -> Vec<Vec<String>> {
    k.keyboard
        .iter()
        .map(|row| row.iter().map(|b| b.text.clone()).collect())
        .collect()
}

fn flat(k: &KeyboardMarkup) -> Vec<String> {
    rows(k).into_iter().flatten().collect()
}

fn v(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|s| (*s).to_string()).collect())
        .collect()
}

fn data_of(b: &InlineKeyboardButton) -> String {
    match &b.kind {
        InlineKeyboardButtonKind::CallbackData(d) => d.clone(),
        other => panic!("not a callback button: {other:?}"),
    }
}

/// Every row as `(text, callback data)` pairs.
fn inline_rows(k: &InlineKeyboardMarkup) -> Vec<Vec<(String, String)>> {
    k.inline_keyboard
        .iter()
        .map(|row| row.iter().map(|b| (b.text.clone(), data_of(b))).collect())
        .collect()
}

fn inline_data(k: &InlineKeyboardMarkup) -> Vec<String> {
    inline_rows(k)
        .into_iter()
        .flatten()
        .map(|(_, d)| d)
        .collect()
}

fn assert_inline_limits(k: &InlineKeyboardMarkup) {
    for row in &k.inline_keyboard {
        assert!(
            row.len() <= MAX_BUTTONS_PER_ROW,
            "too many buttons in a row"
        );
        for b in row {
            let data = data_of(b);
            assert!(!data.is_empty());
            assert!(
                data.len() <= MAX_CALLBACK_DATA_BYTES,
                "callback data {data:?} is {} bytes",
                data.len()
            );
            assert!(!b.text.is_empty());
        }
    }
}

const SUBSYSTEMS: [Subsystem; 5] = [
    Subsystem::ProcessTracker,
    Subsystem::ScreenCapture,
    Subsystem::SystemResources,
    Subsystem::Systemd,
    Subsystem::DockerTracker,
];

/// Every text that `handle_plain_message` routes (copied from
/// `handlers/mod.rs`). A button whose text is not here would do nothing
/// but print "You said: ...".
fn handled_texts() -> Vec<String> {
    let mut out: Vec<String> = [
        // core menu
        "📋 Help",
        "🖼️ Screenshot",
        "🖥️ System Resources",
        "🚨 System Alarms",
        "📊 Process",
        "📊 Top Processes",
        "🔥 By CPU",
        "🧠 By Memory",
        "💾 By Disk",
        "⚙️ Settings",
        // systemd menu
        "🔧 Systemd",
        "📋 Systemd Overview",
        "🔴 Failed Units",
        "🔍 Unit Status",
        // docker menu
        "🐳 Docker",
        "📋 Docker Containers",
        "🔥 By CPU (Docker)",
        "🧠 By Memory (Docker)",
        // polling menu
        "⏱️ Polling",
        "⏱️ Process Tracker Polling",
        "⏱️ Screen Capture Polling",
        "⏱️ System Resources Polling",
        "⏱️ Systemd Polling",
        "⏱️ Docker Tracker Polling",
        // last match arms
        "🔑 Authenticate",
        "❌ Cancel",
        "🔴 Stop",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    for label in [
        "Process Tracker",
        "Screen Capture",
        "System Resources",
        "Systemd",
        "Docker Tracker",
    ] {
        out.push(format!("⏸️ Pause {label}"));
        out.push(format!("▶️ Resume {label}"));
        out.push(format!("🕐 Set {label} Interval"));
    }
    out
}

fn assert_all_handled(name: &str, k: &KeyboardMarkup) {
    let handled = handled_texts();
    for text in flat(k) {
        assert!(
            handled.contains(&text),
            "{name}: button {text:?} is not routed by handle_plain_message \
             (if it looks identical, check invisible characters such as U+FE0F)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Reply keyboards: layout
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn main_keyboard_layout() {
    assert_eq!(
        rows(&main_keyboard()),
        v(&[
            &["📊 Process", "📊 Top Processes"],
            &["🚨 System Alarms", "🖥️ System Resources"],
            &["🔧 Systemd", "🐳 Docker", "🖼️ Screenshot"],
            &["⚙️ Settings", "📋 Help"],
            &["🔑 Authenticate", "🔴 Stop"],
        ])
    );
}

#[test]
fn top_processes_keyboard_layout() {
    assert_eq!(
        rows(&top_processes_keyboard()),
        v(&[&["🔥 By CPU", "🧠 By Memory"], &["💾 By Disk", "❌ Cancel"]])
    );
}

#[test]
fn systemd_keyboard_layout() {
    assert_eq!(
        rows(&systemd_keyboard()),
        v(&[
            &["📋 Systemd Overview", "🔴 Failed Units"],
            &["🔍 Unit Status"],
            &["❌ Cancel"],
        ])
    );
}

#[test]
fn cancel_keyboard_layout() {
    assert_eq!(rows(&cancel_keyboard()), v(&[&["❌ Cancel"]]));
}

#[test]
fn settings_keyboard_layout() {
    assert_eq!(
        rows(&settings_keyboard()),
        v(&[&["⏱️ Polling"], &["❌ Cancel"]])
    );
}

#[test]
fn docker_keyboard_layout() {
    assert_eq!(
        rows(&docker_keyboard()),
        v(&[
            &["📋 Docker Containers", "🔥 By CPU (Docker)"],
            &["🧠 By Memory (Docker)", "❌ Cancel"],
        ])
    );
}

#[test]
fn polling_subsystem_keyboard_layout() {
    assert_eq!(
        rows(&polling_subsystem_keyboard()),
        v(&[
            &["⏱️ Process Tracker Polling", "⏱️ Screen Capture Polling"],
            &["⏱️ System Resources Polling", "⏱️ Systemd Polling"],
            &["❌ Cancel"],
        ])
    );
}

#[test]
fn subsystem_polling_keyboard_layout_uses_the_subsystem_label() {
    assert_eq!(
        rows(&subsystem_polling_keyboard(&Subsystem::Systemd)),
        v(&[
            &["⏸️ Pause Systemd", "▶️ Resume Systemd"],
            &["🕐 Set Systemd Interval"],
            &["❌ Cancel"],
        ])
    );
    assert_eq!(
        rows(&subsystem_polling_keyboard(&Subsystem::ProcessTracker)),
        v(&[
            &["⏸️ Pause Process Tracker", "▶️ Resume Process Tracker"],
            &["🕐 Set Process Tracker Interval"],
            &["❌ Cancel"],
        ])
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Reply keyboards: every button is routed
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn every_main_menu_button_is_routed() {
    assert_all_handled("main", &main_keyboard());
}

#[test]
fn every_submenu_button_is_routed() {
    assert_all_handled("top_processes", &top_processes_keyboard());
    assert_all_handled("systemd", &systemd_keyboard());
    assert_all_handled("cancel", &cancel_keyboard());
    assert_all_handled("settings", &settings_keyboard());
    assert_all_handled("docker", &docker_keyboard());
    assert_all_handled("polling_subsystem", &polling_subsystem_keyboard());
}

#[test]
fn every_per_subsystem_polling_button_is_routed() {
    for s in &SUBSYSTEMS {
        assert_all_handled(s.label(), &subsystem_polling_keyboard(s));
    }
}

#[test]
fn per_subsystem_buttons_are_distinct_between_subsystems() {
    let mut all: Vec<String> = Vec::new();
    for s in &SUBSYSTEMS {
        all.extend(flat(&subsystem_polling_keyboard(s)));
    }
    // "❌ Cancel" appears once per subsystem; every other text must be unique.
    let non_cancel: Vec<&String> = all.iter().filter(|t| *t != "❌ Cancel").collect();
    let mut unique = non_cancel.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), non_cancel.len());
}

#[test]
fn reply_keyboards_have_no_empty_buttons_and_no_row_over_eight() {
    for k in [
        main_keyboard(),
        top_processes_keyboard(),
        systemd_keyboard(),
        cancel_keyboard(),
        settings_keyboard(),
        docker_keyboard(),
        polling_subsystem_keyboard(),
    ] {
        for row in rows(&k) {
            assert!(!row.is_empty());
            assert!(row.len() <= MAX_BUTTONS_PER_ROW);
            for text in row {
                assert!(!text.trim().is_empty());
            }
        }
    }
}

#[test]
fn polling_menu_has_no_docker_button_today() {
    // Pinned (finding 46): `handle_polling_menu` lists Docker Tracker and
    // `dispatch_polling_menu` routes "⏱️ Docker Tracker Polling", but the
    // keyboard never offers that button, so Docker polling cannot be reached.
    let texts = flat(&polling_subsystem_keyboard());
    assert!(!texts.iter().any(|t| t.contains("Docker")));
}

#[test]
#[ignore = "finding 46: the polling submenu has no Docker Tracker button"]
fn polling_menu_offers_every_subsystem() {
    let texts = flat(&polling_subsystem_keyboard());
    for s in &SUBSYSTEMS {
        let wanted = format!("⏱️ {} Polling", s.label());
        assert!(texts.contains(&wanted), "missing {wanted:?}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline keyboards: tracked process
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tracked_process_keyboard_first_row_is_untrack_and_kill_tree() {
    let rows = inline_rows(&tracked_process_keyboard(1234));
    assert_eq!(
        rows.first().cloned().unwrap(),
        vec![
            ("➖ Untrack".to_string(), "untrack:1234".to_string()),
            ("🌲 Kill Tree".to_string(), "killtree:1234".to_string()),
        ]
    );
}

#[test]
fn tracked_process_keyboard_second_row_has_one_button_per_supported_signal() {
    let signals = ProcessSignal::get_supported_signals();
    let kb = tracked_process_keyboard(1234);
    let rows = inline_rows(&kb);
    assert_eq!(rows.len(), 2);
    let signal_row = rows.get(1).cloned().unwrap();
    assert_eq!(signal_row.len(), signals.len());
    for (signal, (text, data)) in signals.into_iter().zip(signal_row) {
        assert_eq!(text, signal.to_string().to_uppercase());
        assert_eq!(
            ProcessCallbackAction::decode(&data),
            Some(ProcessCallbackAction::Signal { pid: 1234, signal })
        );
    }
}

#[test]
fn tracked_process_keyboard_decodes_back_to_the_same_pid() {
    let pid = 98_765;
    for data in inline_data(&tracked_process_keyboard(pid)) {
        match ProcessCallbackAction::decode(&data) {
            Some(
                ProcessCallbackAction::Untrack { pid: p }
                | ProcessCallbackAction::KillTree { pid: p }
                | ProcessCallbackAction::Signal { pid: p, .. },
            ) => assert_eq!(p, pid),
            other => panic!("unexpected decode of {data:?}: {other:?}"),
        }
    }
}

#[test]
fn tracked_process_keyboard_fits_telegram_limits_for_the_largest_pid() {
    assert_inline_limits(&tracked_process_keyboard(u32::MAX));
}

#[test]
fn top_process_keyboard_is_a_single_track_button() {
    assert_eq!(
        inline_rows(&top_process_keyboard(4321)),
        vec![vec![("➕ Track".to_string(), "track:4321".to_string())]]
    );
    assert_inline_limits(&top_process_keyboard(u32::MAX));
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline keyboards: system resources
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn system_resources_keyboard_has_four_preset_buttons() {
    let rows = inline_rows(&system_resources_keyboard());
    let texts: Vec<Vec<&str>> = rows
        .iter()
        .map(|r| r.iter().map(|(t, _)| t.as_str()).collect())
        .collect();
    assert_eq!(
        texts,
        vec![
            vec!["⚠️ Thresholds: Default", "⚠️ Thresholds: Strict"],
            vec!["🔄 Mask: All On", "🔄 Mask: CPU+Mem Only"],
        ]
    );
}

#[test]
fn system_resources_keyboard_presets_decode_to_the_advertised_values() {
    let data = inline_data(&system_resources_keyboard());
    assert_eq!(data.len(), 4);
    assert_eq!(
        SystemResourcesCallbackAction::decode(data.first().unwrap()),
        Some(SystemResourcesCallbackAction::SetThresholds(
            Thresholds::default()
        ))
    );
    assert_eq!(
        SystemResourcesCallbackAction::decode(data.get(1).unwrap()),
        Some(SystemResourcesCallbackAction::SetThresholds(Thresholds {
            cpu_warn: 75.0,
            memory_warn: 75.0,
            disk_warn: 75.0,
            battery_low: 25.0,
        }))
    );
    assert_eq!(
        SystemResourcesCallbackAction::decode(data.get(2).unwrap()),
        Some(SystemResourcesCallbackAction::SetRefreshMask(
            RefreshMask::default()
        ))
    );
    assert_eq!(
        SystemResourcesCallbackAction::decode(data.get(3).unwrap()),
        Some(SystemResourcesCallbackAction::SetRefreshMask(RefreshMask {
            cpu: true,
            memory: true,
            disks: false,
            networks: false,
            temperatures: false,
            gpus: false,
        }))
    );
}

#[test]
fn system_resources_keyboard_fits_telegram_limits() {
    assert_inline_limits(&system_resources_keyboard());
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline keyboards: systemd unit control
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_control_keyboard_has_start_stop_restart_reload_in_order() {
    let rows = inline_rows(&unit_control_keyboard("nginx.service"));
    assert_eq!(
        rows,
        vec![vec![
            (
                "▶️ Start".to_string(),
                "sd_control:nginx.service:start".to_string()
            ),
            (
                "⏹️ Stop".to_string(),
                "sd_control:nginx.service:stop".to_string()
            ),
            (
                "🔄 Restart".to_string(),
                "sd_control:nginx.service:restart".to_string()
            ),
            (
                "🔃 Reload".to_string(),
                "sd_control:nginx.service:reload".to_string()
            ),
        ]]
    );
}

#[test]
fn unit_control_keyboard_decodes_to_the_four_actions() {
    let data = inline_data(&unit_control_keyboard("nginx.service"));
    let expected = [
        ServiceAction::Start,
        ServiceAction::Stop,
        ServiceAction::Restart,
        ServiceAction::Reload,
    ];
    assert_eq!(data.len(), expected.len());
    for (d, action) in data.iter().zip(expected) {
        assert_eq!(
            SystemdCallbackAction::decode(d),
            Some(SystemdCallbackAction::Control {
                unit_name: "nginx.service".to_string(),
                action,
            })
        );
    }
}

#[test]
fn unit_control_keyboard_fits_for_a_typical_name() {
    assert_inline_limits(&unit_control_keyboard("nginx.service"));
    assert_inline_limits(&unit_control_keyboard("getty@tty1.service"));
}

#[test]
fn unit_control_keyboard_limit_is_a_unit_name_of_45_bytes() {
    // Pinned (finding 44): "sd_control:" (11) + name + ":restart" (8) must be
    // at most 64 bytes, so the longest usable name is 45 bytes; Telegram
    // rejects the whole message when one button's data is longer.
    let longest_ok = "a".repeat(45);
    assert_inline_limits(&unit_control_keyboard(&longest_ok));
    let too_long = "a".repeat(46);
    let max = inline_data(&unit_control_keyboard(&too_long))
        .iter()
        .map(String::len)
        .max()
        .unwrap();
    assert_eq!(max, 65);
}

#[test]
#[ignore = "finding 44: callback data for long unit names (for example escaped device units) exceeds Telegram's 64-byte limit"]
fn unit_control_keyboard_fits_for_a_long_escaped_unit_name() {
    let name = r"dev-disk-by\x2duuid-0a1b2c3d\x2d4e5f\x2d6789\x2dabcd\x2d0123456789ab.device";
    assert_inline_limits(&unit_control_keyboard(name));
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline keyboards: docker container
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn docker_container_keyboard_layout() {
    let rows = inline_rows(&docker_container_keyboard("abc123def456"));
    let texts: Vec<Vec<&str>> = rows
        .iter()
        .map(|r| r.iter().map(|(t, _)| t.as_str()).collect())
        .collect();
    assert_eq!(
        texts,
        vec![
            vec!["▶️ Start", "⏹ Stop", "💀 Kill"],
            vec!["🔄 Restart", "⏸️ Pause", "▶️ Unpause"],
        ]
    );
}

#[test]
fn docker_container_keyboard_decodes_to_the_matching_actions() {
    let id = "abc123def456";
    let data = inline_data(&docker_container_keyboard(id));
    let expected = [
        DockerCallbackAction::Start { id: id.to_string() },
        DockerCallbackAction::Stop { id: id.to_string() },
        DockerCallbackAction::Kill { id: id.to_string() },
        DockerCallbackAction::Restart { id: id.to_string() },
        DockerCallbackAction::Pause { id: id.to_string() },
        DockerCallbackAction::Unpause { id: id.to_string() },
    ];
    assert_eq!(data.len(), expected.len());
    for (d, action) in data.iter().zip(expected) {
        assert_eq!(DockerCallbackAction::decode(d), Some(action));
    }
}

#[test]
fn docker_container_keyboard_fits_for_a_12_character_short_id() {
    // The handlers pass `container.short_id`.
    assert_inline_limits(&docker_container_keyboard("abc123def456"));
}

#[test]
fn docker_container_keyboard_does_not_fit_a_full_64_character_id() {
    // Pinned: "dc_unpause:" is 11 bytes, so a full container id gives 75.
    // Fine today because the handlers use the short id; do not pass the full id.
    let full = "a".repeat(64);
    let max = inline_data(&docker_container_keyboard(&full))
        .iter()
        .map(String::len)
        .max()
        .unwrap();
    assert_eq!(max, 75);
}
