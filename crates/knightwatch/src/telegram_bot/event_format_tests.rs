#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use serde_json::{Value, json};

use kw_types::{
    process::ProcessState,
    systemd::{UnitLoadState, UnitType},
};

use super::{
    display::TelegramDisplay,
    utils::{
        format_docker_tracker_event, format_process_tracker_event, format_screen_capture_event,
        format_system_resources_event, format_systemd_event,
    },
};
use crate::{
    docker_tracker::{ContainerHealth, ContainerSnapshot, ContainerStatus, DockerTrackerEvent},
    prelude::DisplayUser,
    process_tracker::{ProcessSnapshot, ProcessTrackerEvent},
    screen_capture::ScreenCaptureEvent,
    system_resources::{
        CpuSnapshot, DiskSnapshot, GpuSnapshot, HostInfo, MemorySnapshot, NetworkSnapshot,
        SystemHealth, SystemResourcesEvent, SystemSnapshot, ThermalSnapshot,
    },
    systemd::{SystemdEvent, SystemdSnapshot, UnitActiveState, UnitSnapshot},
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

const HOSTILE: &str = "x`y*z_w\\[a](b).c-d!";

// What a command message shows for `user()` today (see finding 47).
const USER: &str = "DisplayUser \\{ username: \"bob\", telegram\\_chat\\_id: Some\\(42\\) \\}";

// Telegram's limit for one message, counted after the markup is parsed.
const TELEGRAM_LIMIT: usize = 4_096;

// The same small MarkdownV2 checker as `utils_tests.rs`.
fn assert_valid_mdv2(s: &str) {
    let mut in_code = false;
    let mut escaped = false;
    let mut stars = 0_u32;
    let mut unders = 0_u32;
    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '`' {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        match c {
            '*' => stars += 1,
            '_' => unders += 1,
            '[' | ']' | '(' | ')' | '~' | '>' | '#' | '+' | '-' | '=' | '|' | '{' | '}' | '.'
            | '!' => panic!("unescaped {c:?} outside a code span in {s:?}"),
            _ => {}
        }
    }
    assert!(!escaped, "dangling backslash in {s:?}");
    assert!(!in_code, "unterminated code span in {s:?}");
    assert_eq!(stars % 2, 0, "unbalanced * in {s:?}");
    assert_eq!(unders % 2, 0, "unbalanced _ in {s:?}");
}

// Length of the text a chat would see: escapes and markers are not counted.
// It counts `char`s, which is close enough here because every test sits far from the limit.
fn parsed_len(s: &str) -> usize {
    let mut in_code = false;
    let mut escaped = false;
    let mut len = 0_usize;
    for c in s.chars() {
        if escaped {
            escaped = false;
            len += 1;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '`' => in_code = !in_code,
            '*' | '_' if !in_code => {}
            _ => len += 1,
        }
    }
    len
}

// The command action types live in private `commands` modules, so they cannot be named from
// here. They do derive `Deserialize`, and the event field decides the type.
fn action<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn user() -> DisplayUser {
    DisplayUser {
        username: "bob".to_string(),
        telegram_chat_id: Some(42),
    }
}

fn executed(action_text: &str) -> String {
    format!("⚙️ *Command Executed*\n├ User: `{USER}`\n└ Action: `{action_text}`")
}

fn failed(action_text: &str, error_text: &str) -> String {
    format!(
        "⚠️ *Command Failed*\n├ User: `{USER}`\n├ Action: `{action_text}`\n└ Error: `{error_text}`"
    )
}

fn process_command(value: Value, success: bool, error: Option<&str>) -> String {
    format_process_tracker_event(&ProcessTrackerEvent::CommandExecuted {
        user: user(),
        action: action(value),
        success,
        error: error.map(str::to_string),
    })
}

fn docker_command(value: Value, success: bool, error: Option<&str>) -> String {
    format_docker_tracker_event(&DockerTrackerEvent::CommandExecuted {
        user: user(),
        action: action(value),
        success,
        error: error.map(str::to_string),
    })
}

fn systemd_command(value: Value, success: bool, error: Option<&str>) -> String {
    format_systemd_event(&SystemdEvent::CommandExecuted {
        user: user(),
        action: action(value),
        success,
        error: error.map(str::to_string),
    })
    .unwrap()
}

fn resources_command(value: Value, success: bool, error: Option<&str>) -> String {
    format_system_resources_event(&SystemResourcesEvent::CommandExecuted {
        user: user(),
        action: action(value),
        success,
        error: error.map(str::to_string),
    })
    .unwrap()
}

fn screen_command(value: Value, success: bool, error: Option<&str>) -> String {
    format_screen_capture_event(&ScreenCaptureEvent::CommandExecuted {
        user: user(),
        action: action(value),
        success,
        error: error.map(str::to_string),
    })
}

// Runs one command formatter over the shared success and failure shapes.
fn check_success_and_failure(format: fn(Value, bool, Option<&str>) -> String, value: &Value) {
    let text = "pause polling";
    assert_eq!(format(value.clone(), true, None), executed(text));
    assert_eq!(
        format(value.clone(), false, Some("no such: a.b")),
        failed(text, "no such: a\\.b")
    );
    assert_eq!(
        format(value.clone(), false, None),
        failed(text, "unknown error")
    );
    assert_valid_mdv2(&format(value.clone(), true, None));
    assert_valid_mdv2(&format(value.clone(), false, Some(HOSTILE)));
}

fn process(pid: u32, name: &str) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        name: name.to_string(),
        state: ProcessState::Running,
        cpu_usage: 12.5,
        memory_bytes: 1_536,
        disk_usage: 0,
        cwd: None,
        cmdline: vec![],
        open_files: vec![],
        io_stats: None,
    }
}

fn container(name: &str) -> ContainerSnapshot {
    ContainerSnapshot {
        id: "abc123def456789".to_string(),
        short_id: "abc123def456".to_string(),
        name: name.to_string(),
        image: "nginx:1.25".to_string(),
        status: ContainerStatus::Running,
        health: ContainerHealth::None,
        stats: None,
    }
}

fn unit(name: &str, active_state: UnitActiveState) -> UnitSnapshot {
    UnitSnapshot {
        unit_name: name.to_string(),
        unit_type: UnitType::Service,
        load_state: UnitLoadState::Loaded,
        active_state,
        sub_state: "running".to_string(),
        description: String::new(),
        main_pid: None,
        memory_bytes: None,
        cpu_usage_ns: None,
        restart_count: None,
        since: None,
        fragment_path: None,
    }
}

fn systemd_snapshot(failed_count: u32, units: Vec<UnitSnapshot>) -> SystemdSnapshot {
    SystemdSnapshot {
        timestamp: "2024-05-01".to_string(),
        units,
        failed_count,
        active_count: 2,
        inactive_count: 1,
    }
}

fn snapshot() -> SystemSnapshot {
    SystemSnapshot {
        timestamp: "2024-05-01T10:00:00+00:00".to_string(),
        cpu: CpuSnapshot {
            usage_percent: 12.5,
            cores: vec![],
            frequency_mhz: 3_600,
            brand: "Intel(R) i7".to_string(),
            physical_core_count: Some(4),
            load_avg: None,
        },
        memory: MemorySnapshot {
            total_bytes: 8_589_934_592,
            used_bytes: 4_294_967_296,
            available_bytes: 4_294_967_296,
            free_bytes: 4_294_967_296,
            used_percent: 50.0,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            swap_free_bytes: 0,
            swap_used_percent: None,
        },
        disks: vec![],
        networks: vec![],
        gpus: vec![],
        battery: None,
        temperatures: vec![],
        host: HostInfo {
            hostname: Some("box-1".to_string()),
            os_name: Some("Ubuntu 24.04".to_string()),
            kernel_version: Some("6.8.0".to_string()),
            cpu_arch: Some("x86_64".to_string()),
            uptime_secs: 3_661,
            process_count: 200,
        },
        health: SystemHealth::Healthy,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// the length helper (so a broken helper cannot make the tests pass)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn parsed_len_skips_markers_and_escapes() {
    assert_eq!(parsed_len(""), 0);
    assert_eq!(parsed_len("abc"), 3);
    assert_eq!(parsed_len("*a* _b_"), 3);
    assert_eq!(parsed_len("`b\\.c`"), 3);
    assert_eq!(parsed_len("\\_"), 1);
    assert_eq!(parsed_len("`a_b*c`"), 5);
}

// ─────────────────────────────────────────────────────────────────────────────
// CommandExecuted: shapes shared by all five trackers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn process_command_success_and_failure() {
    check_success_and_failure(process_command, &json!("PausePoll"));
}

#[test]
fn docker_command_success_and_failure() {
    check_success_and_failure(docker_command, &json!("PausePoll"));
}

#[test]
fn systemd_command_success_and_failure() {
    check_success_and_failure(systemd_command, &json!("PausePoll"));
}

#[test]
fn system_resources_command_success_and_failure() {
    check_success_and_failure(resources_command, &json!("PausePoll"));
}

#[test]
fn screen_capture_command_success_and_failure() {
    check_success_and_failure(screen_command, &json!("PausePoll"));
}

#[test]
fn a_success_message_ignores_any_error_text() {
    let msg = process_command(json!("PausePoll"), true, Some("stale"));
    assert_eq!(msg, executed("pause polling"));
}

#[test]
fn command_message_prints_the_user_with_debug_text() {
    // Pinned (finding 47): the Rust struct text reaches every chat, chat id included.
    let msg = process_command(json!("PausePoll"), true, None);
    assert!(msg.contains("DisplayUser"));
    assert!(msg.contains("Some\\(42\\)"));
}

#[test]
fn command_message_for_a_user_with_no_chat_says_none() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::CommandExecuted {
        user: DisplayUser {
            username: "a_b".to_string(),
            telegram_chat_id: None,
        },
        action: action(json!("PausePoll")),
        success: true,
        error: None,
    });
    assert!(
        msg.contains(
            "├ User: `DisplayUser \\{ username: \"a\\_b\", telegram\\_chat\\_id: None \\}`\n"
        ),
        "{msg}"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn command_message_with_a_hostile_username_is_valid_markdown() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::CommandExecuted {
        user: DisplayUser {
            username: HOSTILE.to_string(),
            telegram_chat_id: Some(1),
        },
        action: action(json!("PausePoll")),
        success: false,
        error: Some(HOSTILE.to_string()),
    });
    assert_valid_mdv2(&msg);
}

#[test]
#[ignore = "finding 47: command messages should name the user, without Debug text or the chat id"]
fn command_message_should_only_name_the_user() {
    let msg = process_command(json!("PausePoll"), true, None);
    assert!(msg.contains("bob"));
    assert!(!msg.contains("DisplayUser"));
    assert!(!msg.contains("42"));
}

// ─────────────────────────────────────────────────────────────────────────────
// CommandExecuted: the action text of each tracker
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn process_command_action_text() {
    for (value, text) in [
        (
            json!({"KillTree": {"root_pid": 5}}),
            "kill tree with root pid 5",
        ),
        (json!({"TrackPid": {"pid": 5}}), "track process with pid 5"),
        (
            json!({"UntrackPid": {"pid": 5}}),
            "untrack process with pid 5",
        ),
        (
            json!({"SetPollInterval": {"interval": {"secs": 2, "nanos": 500_000_000}}}),
            "set poll interval to 2500ms",
        ),
        (json!("PausePoll"), "pause polling"),
        (json!("ResumePoll"), "resume polling"),
    ] {
        assert_eq!(process_command(value, true, None), executed(text));
    }
}

#[test]
fn process_kill_action_text_has_a_typo() {
    // Pinned (finding 47): flips with the ignored test below when `singal` is fixed.
    let value = json!({"KillProcess": {"pid": 5, "signal": "kill"}});
    assert_eq!(
        process_command(value, true, None),
        executed("singal kill process 5")
    );
}

#[test]
#[ignore = "finding 47: `singal` is misspelled in ProcessCommandAction's Display"]
fn process_kill_action_text_should_say_signal() {
    let value = json!({"KillProcess": {"pid": 5, "signal": "kill"}});
    assert!(process_command(value, true, None).contains("signal kill process 5"));
}

#[test]
fn docker_command_action_text() {
    for (value, text) in [
        (
            json!({"Stop": {"id_or_name": "abc", "container_name": "web", "timeout_secs": 5}}),
            "stop web \\(abc\\) \\(timeout 5s\\)",
        ),
        (
            json!({"Stop": {"id_or_name": "abc", "container_name": null, "timeout_secs": null}}),
            "stop abc",
        ),
        (
            json!({"Kill": {"id_or_name": "abc", "container_name": null, "signal": "SIGKILL"}}),
            "kill abc \\(signal SIGKILL\\)",
        ),
        (
            json!({"Kill": {"id_or_name": "abc", "container_name": "web", "signal": null}}),
            "kill web \\(abc\\)",
        ),
        (
            json!({"Start": {"id_or_name": "web", "container_name": "web"}}),
            "start web",
        ),
        (
            json!({"Restart": {"id_or_name": "abc", "container_name": null, "timeout_secs": 10}}),
            "restart abc \\(timeout 10s\\)",
        ),
        (
            json!({"Restart": {"id_or_name": "abc", "container_name": null, "timeout_secs": null}}),
            "restart abc",
        ),
        (
            json!({"Pause": {"id_or_name": "my_app", "container_name": null}}),
            "pause my\\_app",
        ),
        (
            json!({"Unpause": {"id_or_name": "abc", "container_name": "a.b"}}),
            "unpause a\\.b \\(abc\\)",
        ),
        (
            json!({"SetPollInterval": {"interval": {"secs": 1, "nanos": 0}}}),
            "set poll interval to 1000ms",
        ),
        (json!("ResumePoll"), "resume polling"),
    ] {
        let msg = docker_command(value, true, None);
        assert_eq!(msg, executed(text));
        assert_valid_mdv2(&msg);
    }
}

#[test]
fn systemd_command_action_text() {
    for (value, text) in [
        // Pinned (finding 47): the unit comes first, so it reads "<unit> on <action>".
        (
            json!({"Control": {"unit_name": "nginx.service", "action": "restart"}}),
            "nginx\\.service on restart",
        ),
        (
            json!({"SetPollInterval": {"interval": {"secs": 0, "nanos": 1_000_000}}}),
            "set poll interval to 1ms",
        ),
        (json!("ResumePoll"), "resume polling"),
    ] {
        let msg = systemd_command(value, true, None);
        assert_eq!(msg, executed(text));
        assert_valid_mdv2(&msg);
    }
}

#[test]
fn systemd_command_with_a_hostile_unit_is_valid_markdown() {
    let value = json!({"Control": {"unit_name": HOSTILE, "action": "stop"}});
    assert_valid_mdv2(&systemd_command(value, false, Some(HOSTILE)));
}

#[test]
fn system_resources_command_action_text() {
    for (value, text) in [
        (
            json!({"SetThresholds": {"thresholds": {
                "cpu_warn": 90.0, "memory_warn": 80.0, "disk_warn": 70.5, "battery_low": 15.0
            }}}),
            "set\\_thresholds cpu\\_warn\\=90\\.0%, memory\\_warn\\=80\\.0%, disk\\_warn\\=70\\.5%, battery\\_low\\=15\\.0%",
        ),
        (
            json!({"SetRefreshMask": {"refresh_mask": {
                "cpu": true, "memory": false, "disks": false,
                "networks": false, "temperatures": false, "gpus": true
            }}}),
            "set\\_refresh\\_mask cpu, gpus",
        ),
        (
            json!({"SetRefreshMask": {"refresh_mask": {
                "cpu": false, "memory": false, "disks": false,
                "networks": false, "temperatures": false, "gpus": false
            }}}),
            "set\\_refresh\\_mask none",
        ),
        (
            json!({"SetPollInterval": {"interval": {"secs": 3, "nanos": 0}}}),
            "set poll interval to 3000ms",
        ),
        (json!("ResumePoll"), "resume polling"),
    ] {
        let msg = resources_command(value, true, None);
        assert_eq!(msg, executed(text));
        assert_valid_mdv2(&msg);
    }
}

#[test]
fn screen_capture_command_action_text() {
    for (value, text) in [
        (
            json!({"SetPollInterval": {"interval": {"secs": 3, "nanos": 0}}}),
            "set poll interval to 3000ms",
        ),
        (json!("ResumePoll"), "resume polling"),
    ] {
        assert_eq!(screen_command(value, true, None), executed(text));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// process events with snapshots
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn process_initial_snapshot_with_root_and_children() {
    let (root, first, second) = (
        process(1, "root"),
        process(2, "first"),
        process(3, "second"),
    );
    let msg = format_process_tracker_event(&ProcessTrackerEvent::InitialSnapshot {
        root: Some(root.clone()),
        children: vec![first.clone(), second.clone()],
    });
    let (root, first, second) = (
        TelegramDisplay(&root).to_string(),
        TelegramDisplay(&first).to_string(),
        TelegramDisplay(&second).to_string(),
    );
    assert_eq!(
        msg,
        format!(
            "🟢 *Initial Snapshot*\n\n*Root:*\n{root}\n\n*Children* \\(2\\):\n{first}\n\n{second}\n"
        )
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn process_initial_snapshot_with_root_only() {
    let root = process(1, "root");
    let msg = format_process_tracker_event(&ProcessTrackerEvent::InitialSnapshot {
        root: Some(root.clone()),
        children: vec![],
    });
    let root = TelegramDisplay(&root).to_string();
    assert_eq!(
        msg,
        format!("🟢 *Initial Snapshot*\n\n*Root:*\n{root}\n\n*Children:* _none_")
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn process_initial_snapshot_with_children_only() {
    let child = process(2, "child");
    let msg = format_process_tracker_event(&ProcessTrackerEvent::InitialSnapshot {
        root: None,
        children: vec![child.clone()],
    });
    let child = TelegramDisplay(&child).to_string();
    assert_eq!(
        msg,
        format!("🟢 *Initial Snapshot*\n\n*Root:*\n_none_\n\n*Children* \\(1\\):\n{child}\n")
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn process_children_appeared_with_items() {
    let (first, second) = (process(2, "first"), process(3, "second"));
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ChildrenAppeared {
        pid: 10,
        children: vec![first.clone(), second.clone()],
    });
    let (first, second) = (
        TelegramDisplay(&first).to_string(),
        TelegramDisplay(&second).to_string(),
    );
    assert_eq!(
        msg,
        format!("🆕 *New Children Appeared* \\(2\\)\n*Root:* `10`\n{first}\n\n{second}\n")
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn process_events_with_hostile_names_are_valid_markdown() {
    assert_valid_mdv2(&format_process_tracker_event(
        &ProcessTrackerEvent::InitialSnapshot {
            root: Some(process(1, HOSTILE)),
            children: vec![process(2, HOSTILE)],
        },
    ));
    assert_valid_mdv2(&format_process_tracker_event(
        &ProcessTrackerEvent::ChildrenAppeared {
            pid: 1,
            children: vec![process(2, HOSTILE)],
        },
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// system resources events with snapshots
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn system_resources_initial_snapshot_wraps_the_display() {
    let snap = snapshot();
    let msg = format_system_resources_event(&SystemResourcesEvent::InitialSnapshot {
        snapshot: snap.clone(),
    })
    .unwrap();
    let body = TelegramDisplay(&snap).to_string();
    // Pinned (finding 45): flips with the ignored test below when `Resouces` is fixed.
    assert_eq!(msg, format!("🟢 *System Resouces Started*\n\n{body}"));
    assert_valid_mdv2(&msg);
}

#[test]
#[ignore = "finding 45: `Resouces` is misspelled in the system resources initial snapshot"]
fn system_resources_initial_snapshot_should_be_spelled_correctly() {
    let msg = format_system_resources_event(&SystemResourcesEvent::InitialSnapshot {
        snapshot: snapshot(),
    })
    .unwrap();
    assert!(msg.starts_with("🟢 *System Resources Started*\n"), "{msg}");
}

#[test]
fn system_resources_tick_sends_nothing_even_with_data() {
    let event = SystemResourcesEvent::Tick {
        snapshot: snapshot(),
    };
    assert!(format_system_resources_event(&event).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// systemd events with snapshots
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn systemd_initial_snapshot_wraps_the_display() {
    let snap = systemd_snapshot(1, vec![unit("a.service", UnitActiveState::Failed)]);
    let msg = format_systemd_event(&SystemdEvent::InitialSnapshot {
        snapshot: snap.clone(),
    })
    .unwrap();
    let body = TelegramDisplay(&snap).to_string();
    assert_eq!(msg, format!("🟢 *Systemd Started*\n\n{body}"));
    assert_valid_mdv2(&msg);
}

#[test]
fn systemd_tick_sends_nothing_even_with_data() {
    let event = SystemdEvent::Tick {
        snapshot: systemd_snapshot(1, vec![unit("a.service", UnitActiveState::Failed)]),
    };
    assert!(format_systemd_event(&event).is_none());
}

#[test]
fn systemd_unit_appeared_wraps_the_display() {
    let snap = unit("a.service", UnitActiveState::Active);
    let msg = format_systemd_event(&SystemdEvent::UnitAppeared { unit: snap.clone() }).unwrap();
    let body = TelegramDisplay(&snap).to_string();
    assert_eq!(msg, format!("🆕 *Unit Appeared*\n{body}"));
    assert_valid_mdv2(&msg);
}

#[test]
fn systemd_events_with_hostile_names_are_valid_markdown() {
    let snap = systemd_snapshot(1, vec![unit(HOSTILE, UnitActiveState::Failed)]);
    assert_valid_mdv2(
        &format_systemd_event(&SystemdEvent::InitialSnapshot { snapshot: snap }).unwrap(),
    );
    assert_valid_mdv2(
        &format_systemd_event(&SystemdEvent::UnitAppeared {
            unit: unit(HOSTILE, UnitActiveState::Active),
        })
        .unwrap(),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// docker events with snapshots
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn docker_initial_snapshot_joins_the_containers() {
    let (first, second) = (container("first"), container("second"));
    let msg = format_docker_tracker_event(&DockerTrackerEvent::InitialSnapshot {
        containers: vec![first.clone(), second.clone()],
    });
    let (first, second) = (
        TelegramDisplay(&first).to_string(),
        TelegramDisplay(&second).to_string(),
    );
    assert_eq!(
        msg,
        format!("🐳 *Docker Started* \\(2\\)\n\n{first}\n\n{second}")
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_containers_appeared_joins_the_containers() {
    let (first, second) = (container("first"), container("second"));
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainersAppeared {
        containers: vec![first.clone(), second.clone()],
    });
    let (first, second) = (
        TelegramDisplay(&first).to_string(),
        TelegramDisplay(&second).to_string(),
    );
    assert_eq!(
        msg,
        format!("🆕 *Containers Appeared* \\(2\\)\n\n{first}\n\n{second}")
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_containers_disappeared_lists_names_and_short_ids() {
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainersDisappeared {
        containers: vec![container("web_1"), container("db.2")],
    });
    assert_eq!(
        msg,
        "👻 *Containers Disappeared* \\(2\\)\n• `web\\_1` `abc123def456`\n• `db\\.2` `abc123def456`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_status_changed_shows_the_old_and_new_status() {
    let current = ContainerSnapshot {
        status: ContainerStatus::Exited,
        ..container("web_1")
    };
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainerStatusChanged {
        container: current,
        previous: ContainerStatus::Running,
    });
    assert_eq!(
        msg,
        "🔄 *Container Status Changed*\n├ Name: `web\\_1`\n├ Was: `running`\n└ Now: `exited`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_status_changed_escapes_an_unknown_status() {
    // Uses Display, so unlike the container display it keeps the case and has no quotes.
    let current = ContainerSnapshot {
        status: ContainerStatus::Unknown("Weird".to_string()),
        ..container("a")
    };
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainerStatusChanged {
        container: current,
        previous: ContainerStatus::Unknown("empty".to_string()),
    });
    assert_eq!(
        msg,
        "🔄 *Container Status Changed*\n├ Name: `a`\n├ Was: `unknown\\(empty\\)`\n└ Now: `unknown\\(Weird\\)`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_health_changed_shows_the_old_and_new_health() {
    let current = ContainerSnapshot {
        health: ContainerHealth::Unhealthy,
        ..container("web_1")
    };
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainerHealthChanged {
        container: current,
        previous: ContainerHealth::Healthy,
    });
    assert_eq!(
        msg,
        "🏥 *Container Health Changed*\n├ Name: `web\\_1`\n├ Was: `healthy`\n└ Now: `unhealthy`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_events_with_hostile_names_are_valid_markdown() {
    let hostile = ContainerSnapshot {
        short_id: HOSTILE.to_string(),
        image: HOSTILE.to_string(),
        status: ContainerStatus::Unknown(HOSTILE.to_string()),
        ..container(HOSTILE)
    };
    for event in [
        DockerTrackerEvent::InitialSnapshot {
            containers: vec![hostile.clone()],
        },
        DockerTrackerEvent::ContainersAppeared {
            containers: vec![hostile.clone()],
        },
        DockerTrackerEvent::ContainersDisappeared {
            containers: vec![hostile.clone()],
        },
        DockerTrackerEvent::ContainerStatusChanged {
            container: hostile.clone(),
            previous: ContainerStatus::Unknown(HOSTILE.to_string()),
        },
        DockerTrackerEvent::ContainerHealthChanged {
            container: hostile,
            previous: ContainerHealth::Unknown,
        },
    ] {
        assert_valid_mdv2(&format_docker_tracker_event(&event));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Telegram's length limit (finding 43)
// ─────────────────────────────────────────────────────────────────────────────

fn many_containers() -> DockerTrackerEvent {
    DockerTrackerEvent::InitialSnapshot {
        containers: (0..100).map(|i| container(&format!("web_{i}"))).collect(),
    }
}

fn many_children() -> ProcessTrackerEvent {
    ProcessTrackerEvent::ChildrenAppeared {
        pid: 1,
        children: (0..100).map(|i| process(i, "worker")).collect(),
    }
}

fn many_failed_units() -> SystemdEvent {
    let units = (0..300)
        .map(|i| unit(&format!("job-{i}.service"), UnitActiveState::Failed))
        .collect();
    SystemdEvent::InitialSnapshot {
        snapshot: systemd_snapshot(300, units),
    }
}

fn many_exited_children() -> ProcessTrackerEvent {
    ProcessTrackerEvent::ChildrenExited {
        pid: 1,
        children: (100_000..101_000).collect(),
    }
}

#[test]
fn nothing_splits_or_truncates_long_messages() {
    // Pinned (finding 43): `broadcast_message` sends these as they are, Telegram refuses them
    // with MESSAGE_TOO_LONG, and the chat is then removed.
    for msg in [
        format_docker_tracker_event(&many_containers()),
        format_process_tracker_event(&many_children()),
        format_process_tracker_event(&many_exited_children()),
        format_systemd_event(&many_failed_units()).unwrap(),
    ] {
        assert!(parsed_len(&msg) > TELEGRAM_LIMIT, "{}", parsed_len(&msg));
        assert_valid_mdv2(&msg);
    }
}

#[test]
#[ignore = "finding 43: event messages should be split or truncated to Telegram's limit"]
fn long_messages_should_fit_the_limit() {
    // Assumes truncation. If the fix splits messages instead, the formatters' return type
    // changes and this test should check each part.
    for msg in [
        format_docker_tracker_event(&many_containers()),
        format_process_tracker_event(&many_children()),
        format_process_tracker_event(&many_exited_children()),
        format_systemd_event(&many_failed_units()).unwrap(),
    ] {
        assert!(parsed_len(&msg) <= TELEGRAM_LIMIT, "{}", parsed_len(&msg));
    }
}

#[test]
fn a_typical_system_snapshot_fits_the_limit() {
    let snap = SystemSnapshot {
        disks: (0..4)
            .map(|i| DiskSnapshot {
                name: format!("sda{i}"),
                mount_point: format!("/mnt/disk{i}"),
                file_system: "ext4".to_string(),
                kind: kw_types::resources::DiskKind::Ssd,
                is_removable: false,
                total_bytes: 2_147_483_648,
                used_bytes: 1_073_741_824,
                available_bytes: 1_073_741_824,
                used_percent: 50.0,
            })
            .collect(),
        networks: (0..2)
            .map(|i| NetworkSnapshot {
                interface: format!("eth{i}"),
                rx_bytes_per_sec: 1_024,
                tx_bytes_per_sec: 1_024,
                rx_total_bytes: 0,
                tx_total_bytes: 0,
                rx_packets_per_sec: 0,
                tx_packets_per_sec: 0,
                rx_errors: 0,
                tx_errors: 0,
            })
            .collect(),
        gpus: vec![GpuSnapshot {
            name: "RTX 4090".to_string(),
            usage_percent: Some(75.0),
            vram_used_bytes: Some(1_073_741_824),
            vram_total_bytes: Some(2_147_483_648),
            vram_used_percent: Some(50.0),
            temperature_celsius: Some(65.0),
            power_draw_watts: Some(200.5),
            power_limit_watts: Some(450.0),
            fan_speed_percent: vec![30.0, 40.0],
        }],
        temperatures: (0..8)
            .map(|i| ThermalSnapshot {
                label: format!("coretemp Core {i}"),
                temperature_celsius: Some(65.0),
                temperature_max_celsius: None,
                temperature_critical_celsius: Some(100.0),
            })
            .collect(),
        ..snapshot()
    };
    let msg =
        format_system_resources_event(&SystemResourcesEvent::InitialSnapshot { snapshot: snap })
            .unwrap();
    assert!(parsed_len(&msg) <= TELEGRAM_LIMIT, "{}", parsed_len(&msg));
    assert_valid_mdv2(&msg);
}

#[test]
fn a_few_containers_fit_the_limit() {
    let msg = format_docker_tracker_event(&DockerTrackerEvent::InitialSnapshot {
        containers: (0..5).map(|i| container(&format!("web_{i}"))).collect(),
    });
    assert!(parsed_len(&msg) <= TELEGRAM_LIMIT, "{}", parsed_len(&msg));
}
