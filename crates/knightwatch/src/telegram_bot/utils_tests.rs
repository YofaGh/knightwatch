#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use super::utils::{
    container_health_emoji, container_status_emoji, escape_mdv2, format_docker_tracker_event,
    format_process_tracker_event, format_system_resources_event, format_systemd_event,
    health_emoji, unit_state_emoji,
};

use crate::{
    docker_tracker::{ContainerHealth, ContainerStatus, DockerTrackerEvent},
    process_tracker::ProcessTrackerEvent,
    system_resources::{BatteryState, SystemHealth, SystemResourcesEvent},
    systemd::{SystemdEvent, UnitActiveState},
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Characters Telegram requires to be escaped in MarkdownV2 (plus backslash).
const SPECIAL: &str = "_*[]()~`>#+-=|{}.!\\";

/// A small MarkdownV2 checker. Outside code spans, only `*`, `_` and `` ` ``
/// may appear unescaped, and bold and italic markers must pair up. Inside a
/// code span only the backtick and the backslash need escaping.
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

// ─────────────────────────────────────────────────────────────────────────────
// escape_mdv2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn escape_of_empty_is_empty() {
    assert_eq!(escape_mdv2(""), "");
}

#[test]
fn escape_leaves_plain_text_alone() {
    assert_eq!(escape_mdv2("hello world 123"), "hello world 123");
    assert_eq!(escape_mdv2("café ünïcode"), "café ünïcode");
    assert_eq!(escape_mdv2("🔹 🖥️ ✅"), "🔹 🖥️ ✅");
}

#[test]
fn escape_prefixes_every_special_character_with_a_backslash() {
    for c in SPECIAL.chars() {
        assert_eq!(escape_mdv2(&c.to_string()), format!("\\{c}"), "char {c:?}");
    }
}

#[test]
fn escape_only_touches_the_special_characters() {
    for byte in 1_u8..=126 {
        let c = char::from(byte);
        if SPECIAL.contains(c) {
            continue;
        }
        assert_eq!(escape_mdv2(&c.to_string()), c.to_string(), "char {c:?}");
    }
}

#[test]
fn escape_doubles_a_backslash() {
    assert_eq!(escape_mdv2("a\\b"), "a\\\\b");
    assert_eq!(escape_mdv2("\\"), "\\\\");
}

#[test]
fn escape_is_not_idempotent() {
    assert_eq!(escape_mdv2("a.b"), "a\\.b");
    assert_eq!(escape_mdv2(&escape_mdv2("a.b")), "a\\\\\\.b");
}

#[test]
fn escape_handles_typical_values() {
    assert_eq!(escape_mdv2("nginx.service"), "nginx\\.service");
    assert_eq!(escape_mdv2("my_app-1"), "my\\_app\\-1");
    assert_eq!(escape_mdv2("(PID 5)"), "\\(PID 5\\)");
    assert_eq!(escape_mdv2("C:\\Windows"), "C:\\\\Windows");
    assert_eq!(escape_mdv2("a+b=c"), "a\\+b\\=c");
    assert_eq!(escape_mdv2("50%"), "50%");
    assert_eq!(escape_mdv2("a/b"), "a/b");
}

#[test]
fn escape_keeps_newlines_and_spaces() {
    assert_eq!(escape_mdv2("a\nb c"), "a\nb c");
}

#[test]
fn removing_the_escapes_gives_back_the_input() {
    let input = "x_y*z[1](2)~`>#+-=|{}.!\\end";
    let escaped = escape_mdv2(input);
    let mut back = String::new();
    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            back.push(chars.next().unwrap());
        } else {
            back.push(c);
        }
    }
    assert_eq!(back, input);
}

#[test]
fn escaped_text_is_always_valid_markdown_v2() {
    for input in [
        SPECIAL,
        "`*_",
        "trailing backslash \\",
        "a_b_c",
        "*bold?*",
        "[link](http://x.y)",
        "plain",
        "",
    ] {
        assert_valid_mdv2(&escape_mdv2(input));
    }
}

#[test]
fn escaped_text_is_valid_inside_a_code_span_too() {
    for input in [SPECIAL, "`tick`", "a\\b"] {
        assert_valid_mdv2(&format!("`{}`", escape_mdv2(input)));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// checker self-test (so a broken checker cannot make the tests pass)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "unescaped")]
fn checker_rejects_an_unescaped_dot_outside_code() {
    assert_valid_mdv2("version 1.2");
}

#[test]
#[should_panic(expected = "unbalanced")]
fn checker_rejects_an_odd_number_of_stars() {
    assert_valid_mdv2("*bold");
}

#[test]
#[should_panic(expected = "unterminated")]
fn checker_rejects_an_open_code_span() {
    assert_valid_mdv2("`code");
}

#[test]
fn checker_accepts_dots_inside_a_code_span() {
    assert_valid_mdv2("*Usage:* `93.5%`");
}

// ─────────────────────────────────────────────────────────────────────────────
// emoji helpers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_state_emojis() {
    assert_eq!(unit_state_emoji(&UnitActiveState::Active), "🟢");
    assert_eq!(unit_state_emoji(&UnitActiveState::Reloading), "🔄");
    assert_eq!(unit_state_emoji(&UnitActiveState::Inactive), "⚫");
    assert_eq!(unit_state_emoji(&UnitActiveState::Failed), "🔴");
    assert_eq!(unit_state_emoji(&UnitActiveState::Activating), "🟡");
    assert_eq!(unit_state_emoji(&UnitActiveState::Deactivating), "🟠");
}

#[test]
fn container_status_emojis() {
    assert_eq!(container_status_emoji(&ContainerStatus::Running), "🟢");
    assert_eq!(container_status_emoji(&ContainerStatus::Paused), "🟡");
    assert_eq!(container_status_emoji(&ContainerStatus::Restarting), "🔄");
    assert_eq!(container_status_emoji(&ContainerStatus::Exited), "⚫");
    assert_eq!(container_status_emoji(&ContainerStatus::Dead), "🔴");
    assert_eq!(container_status_emoji(&ContainerStatus::Created), "🔵");
    assert_eq!(container_status_emoji(&ContainerStatus::Removing), "🟠");
    assert_eq!(container_status_emoji(&ContainerStatus::Stopping), "🟠");
    assert_eq!(
        container_status_emoji(&ContainerStatus::Unknown(Default::default())),
        "❓"
    );
}

#[test]
fn container_health_emojis() {
    assert_eq!(container_health_emoji(&ContainerHealth::Healthy), "💚");
    assert_eq!(container_health_emoji(&ContainerHealth::Unhealthy), "❤️");
    assert_eq!(container_health_emoji(&ContainerHealth::Starting), "🤍");
    assert_eq!(container_health_emoji(&ContainerHealth::None), "🩶");
    assert_eq!(container_health_emoji(&ContainerHealth::Unknown), "🩶");
}

#[test]
fn system_health_emojis() {
    assert_eq!(health_emoji(&SystemHealth::Healthy), "✅");
    assert_eq!(health_emoji(&SystemHealth::Warning), "⚠️");
    assert_eq!(health_emoji(&SystemHealth::Critical), "🔴");
}

// ─────────────────────────────────────────────────────────────────────────────
// format_process_tracker_event
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn initial_snapshot_without_root_or_children() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::InitialSnapshot {
        root: None,
        children: vec![],
    });
    assert_eq!(
        msg,
        "🟢 *Initial Snapshot*\n\n*Root:*\n_none_\n\n*Children:* _none_"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn children_appeared_with_no_children_lists_a_zero_count() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ChildrenAppeared {
        pid: 10,
        children: vec![],
    });
    assert_eq!(msg, "🆕 *New Children Appeared* \\(0\\)\n*Root:* `10`");
    assert_valid_mdv2(&msg);
}

#[test]
fn children_exited_lists_each_pid_in_code() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ChildrenExited {
        pid: 10,
        children: vec![11, 12, 13],
    });
    assert_eq!(
        msg,
        "🔴 *Children Exited*\n*Root:* `10`\nchildren PIDs: `11`, `12`, `13`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn children_exited_with_one_pid_has_no_separator() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ChildrenExited {
        pid: 10,
        children: vec![11],
    });
    assert!(msg.ends_with("children PIDs: `11`"));
}

#[test]
fn children_exited_with_no_pids_leaves_the_list_empty() {
    // Pinned: nothing special-cased, the line just ends after the colon.
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ChildrenExited {
        pid: 10,
        children: vec![],
    });
    assert!(msg.ends_with("children PIDs: "));
}

#[test]
fn all_children_gone_message() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::AllChildrenGone { pid: 10 });
    assert_eq!(
        msg,
        "✅ *All children have exited*\n*Root:* `10`\nThe root process may still be running\\."
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn root_exited_message() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::RootExited { pid: 10 });
    assert_eq!(msg, "💀 *Root Process Exited*\nPID: `10`");
    assert_valid_mdv2(&msg);
}

#[test]
fn work_complete_message() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::WorkComplete { pid: 10 });
    assert_eq!(msg, "✅ *Work Complete*\nPID: `10`");
    assert_valid_mdv2(&msg);
}

#[test]
fn process_killed_success_message() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ProcessKilled {
        pid: 10,
        success: true,
    });
    assert_eq!(msg, "✅ *Successfully killed process*\nPID: `10`");
    assert_valid_mdv2(&msg);
}

#[test]
fn process_killed_failure_message_today() {
    // Pinned (finding 45): the failure text uses the success emoji and has a
    // grammar slip ("Failed to killed process").
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ProcessKilled {
        pid: 10,
        success: false,
    });
    assert_eq!(msg, "✅ *Failed to killed process*\nPID: `10`");
    assert_valid_mdv2(&msg);
}

#[test]
#[ignore = "finding 45: a failed kill is reported with the success emoji and bad grammar"]
fn process_killed_failure_message_looks_like_a_failure() {
    let msg = format_process_tracker_event(&ProcessTrackerEvent::ProcessKilled {
        pid: 10,
        success: false,
    });
    assert!(msg.starts_with('❌'), "got {msg:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// format_system_resources_event
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cpu_threshold_exceeded_message() {
    let msg = format_system_resources_event(&SystemResourcesEvent::CpuThresholdExceeded {
        usage_percent: 93.46,
        threshold: 90.0,
    })
    .unwrap();
    assert_eq!(
        msg,
        "⚠️ *CPU Threshold Exceeded*\n├ Usage: `93.5%`\n└ Threshold: `90.0%`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn memory_threshold_exceeded_message() {
    let msg = format_system_resources_event(&SystemResourcesEvent::MemoryThresholdExceeded {
        used_percent: 81.04,
        threshold: 80.0,
    })
    .unwrap();
    assert_eq!(
        msg,
        "⚠️ *Memory Threshold Exceeded*\n├ Usage: `81.0%`\n└ Threshold: `80.0%`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn disk_threshold_exceeded_escapes_the_mount_point() {
    let msg = format_system_resources_event(&SystemResourcesEvent::DiskThresholdExceeded {
        mount_point: "/mnt/data-1.x".to_string(),
        used_percent: 95.0,
        threshold: 90.0,
    })
    .unwrap();
    assert_eq!(
        msg,
        "⚠️ *Disk Threshold Exceeded*\n├ Mount: `/mnt/data\\-1\\.x`\n├ Usage: `95.0%`\n└ Threshold: `90.0%`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn disk_threshold_exceeded_survives_hostile_mount_names() {
    let msg = format_system_resources_event(&SystemResourcesEvent::DiskThresholdExceeded {
        mount_point: "a`b*c_d\\e".to_string(),
        used_percent: 95.0,
        threshold: 90.0,
    })
    .unwrap();
    assert_valid_mdv2(&msg);
}

#[test]
fn battery_low_message() {
    let msg = format_system_resources_event(&SystemResourcesEvent::BatteryLow {
        charge_percent: 12.0,
        threshold: 15.0,
    })
    .unwrap();
    assert_eq!(
        msg,
        "🪫 *Battery Low*\n├ Charge: `12.0%`\n└ Threshold: `15.0%`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn battery_state_changed_messages() {
    let cases = [
        (BatteryState::Charging, "⚡", "Charging"),
        (BatteryState::Discharging, "🔋", "Discharging"),
        (BatteryState::Full, "✅", "Full"),
        (BatteryState::Unknown, "🔌", "Unknown"),
    ];
    for (state, emoji, label) in cases {
        let msg =
            format_system_resources_event(&SystemResourcesEvent::BatteryStateChanged { state })
                .unwrap();
        assert_eq!(
            msg,
            format!("{emoji} *Battery State Changed*\n└ State: `{label}`")
        );
        assert_valid_mdv2(&msg);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// format_systemd_event
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_failed_message() {
    let previous = UnitActiveState::Active;
    let was = previous.as_str().to_string();
    let msg = format_systemd_event(&SystemdEvent::UnitFailed {
        unit_name: "nginx.service".to_string(),
        previous_state: previous,
    })
    .unwrap();
    assert_eq!(
        msg,
        format!("🔴 *Unit Failed*\n├ Unit: `nginx\\.service`\n└ Was: `{was}`")
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn unit_recovered_message() {
    let msg = format_systemd_event(&SystemdEvent::UnitRecovered {
        unit_name: "nginx.service".to_string(),
    })
    .unwrap();
    assert_eq!(msg, "✅ *Unit Recovered*\n└ Unit: `nginx\\.service`");
    assert_valid_mdv2(&msg);
}

#[test]
fn unit_disappeared_message() {
    let msg = format_systemd_event(&SystemdEvent::UnitDisappeared {
        unit_name: "my_job.timer".to_string(),
    })
    .unwrap();
    assert_eq!(msg, "👻 *Unit Disappeared*\n└ Unit: `my\\_job\\.timer`");
    assert_valid_mdv2(&msg);
}

#[test]
fn systemd_messages_survive_hostile_unit_names() {
    let name = "a`b*c_d\\e-f.g".to_string();
    for msg in [
        format_systemd_event(&SystemdEvent::UnitRecovered {
            unit_name: name.clone(),
        })
        .unwrap(),
        format_systemd_event(&SystemdEvent::UnitDisappeared {
            unit_name: name.clone(),
        })
        .unwrap(),
        format_systemd_event(&SystemdEvent::UnitFailed {
            unit_name: name,
            previous_state: UnitActiveState::Activating,
        })
        .unwrap(),
    ] {
        assert_valid_mdv2(&msg);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// format_docker_tracker_event
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn docker_initial_snapshot_without_containers() {
    let msg =
        format_docker_tracker_event(&DockerTrackerEvent::InitialSnapshot { containers: vec![] });
    assert_eq!(msg, "🐳 *Docker Started* — no containers found\\.");
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_containers_appeared_with_an_empty_list() {
    // Pinned: an empty list still produces a message with a zero count.
    let msg =
        format_docker_tracker_event(&DockerTrackerEvent::ContainersAppeared { containers: vec![] });
    assert_eq!(msg, "🆕 *Containers Appeared* \\(0\\)\n\n");
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_containers_disappeared_with_an_empty_list() {
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainersDisappeared {
        containers: vec![],
    });
    assert_eq!(msg, "👻 *Containers Disappeared* \\(0\\)\n");
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_oom_killed_escapes_name_and_id() {
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainerOomKilled {
        id: "abc123".to_string(),
        name: "my_app".to_string(),
    });
    assert_eq!(
        msg,
        "💥 *Container OOM Killed*\n├ Name: `my\\_app`\n└ ID: `abc123`"
    );
    assert_valid_mdv2(&msg);
}

#[test]
fn docker_oom_killed_survives_hostile_names() {
    let msg = format_docker_tracker_event(&DockerTrackerEvent::ContainerOomKilled {
        id: "a.b".to_string(),
        name: "x`y*z_w\\".to_string(),
    });
    assert_valid_mdv2(&msg);
}
