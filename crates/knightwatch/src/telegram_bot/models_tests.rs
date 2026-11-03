#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use teloxide::{types::ChatId, utils::command::BotCommands};

use super::models::{
    AuthState, Chat, ChatState, Command, DockerCallbackAction, ProcessCallbackAction, State,
    Subsystem, SystemResourcesCallbackAction, SystemdCallbackAction,
};

use crate::{
    process_tracker::ProcessSignal,
    system_resources::{RefreshMask, Thresholds},
    systemd::ServiceAction,
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

/// A `State` that does not touch the global config or users.
fn state(auth_enabled: bool) -> State {
    State {
        chats: Arc::new(Mutex::new(HashMap::new())),
        auth_enabled,
    }
}

fn sorted(ids: Vec<ChatId>) -> Vec<i64> {
    let mut out: Vec<i64> = ids.into_iter().map(|c| c.0).collect();
    out.sort_unstable();
    out
}

fn sig(name: &str) -> ProcessSignal {
    ProcessSignal::try_from(name)
        .ok()
        .expect("known process signal")
}

fn service_actions() -> [ServiceAction; 4] {
    [
        ServiceAction::Start,
        ServiceAction::Stop,
        ServiceAction::Restart,
        ServiceAction::Reload,
    ]
}

fn mask_from_bits(bits: u8) -> RefreshMask {
    RefreshMask {
        cpu: bits & 1 != 0,
        memory: bits & 2 != 0,
        disks: bits & 4 != 0,
        networks: bits & 8 != 0,
        temperatures: bits & 16 != 0,
        gpus: bits & 32 != 0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Command (teloxide BotCommands)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn every_command_parses_from_its_lowercase_name() {
    assert!(matches!(
        Command::parse("/start", "kw_bot"),
        Ok(Command::Start)
    ));
    assert!(matches!(
        Command::parse("/menu", "kw_bot"),
        Ok(Command::Menu)
    ));
    assert!(matches!(
        Command::parse("/help", "kw_bot"),
        Ok(Command::Help)
    ));
    assert!(matches!(
        Command::parse("/auth", "kw_bot"),
        Ok(Command::Auth)
    ));
    assert!(matches!(
        Command::parse("/screenshot", "kw_bot"),
        Ok(Command::Screenshot)
    ));
    assert!(matches!(
        Command::parse("/process", "kw_bot"),
        Ok(Command::Process)
    ));
    assert!(matches!(
        Command::parse("/topprocesses", "kw_bot"),
        Ok(Command::TopProcesses)
    ));
    assert!(matches!(
        Command::parse("/systemsnapshot", "kw_bot"),
        Ok(Command::SystemSnapshot)
    ));
    assert!(matches!(
        Command::parse("/systemalarms", "kw_bot"),
        Ok(Command::SystemAlarms)
    ));
    assert!(matches!(
        Command::parse("/stopknightwatch", "kw_bot"),
        Ok(Command::StopKnightWatch)
    ));
}

#[test]
fn command_with_own_bot_suffix_parses() {
    assert!(matches!(
        Command::parse("/start@kw_bot", "kw_bot"),
        Ok(Command::Start)
    ));
}

#[test]
fn command_addressed_to_another_bot_is_rejected() {
    assert!(Command::parse("/start@other_bot", "kw_bot").is_err());
}

#[test]
fn unknown_commands_and_plain_text_are_rejected() {
    assert!(Command::parse("/unknown", "kw_bot").is_err());
    assert!(Command::parse("start", "kw_bot").is_err());
    assert!(Command::parse("", "kw_bot").is_err());
}

#[test]
fn command_descriptions_list_the_commands() {
    let text = Command::descriptions().to_string();
    assert!(text.contains("Available commands:"));
    assert!(text.contains("/start"));
    assert!(text.contains("/stopknightwatch"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Chat, ChatState, Subsystem
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn new_chat_is_idle_and_unauthenticated() {
    let c = Chat::new();
    assert_eq!(c.chat_state, ChatState::Idle);
    assert_eq!(c.auth_state, AuthState::Unauthenticated);
}

#[test]
fn new_authed_chat_is_idle_and_authenticated() {
    let c = Chat::new_authed();
    assert_eq!(c.chat_state, ChatState::Idle);
    assert_eq!(c.auth_state, AuthState::Authenticated);
}

#[test]
fn subsystem_labels_are_exact() {
    assert_eq!(Subsystem::ProcessTracker.label(), "Process Tracker");
    assert_eq!(Subsystem::ScreenCapture.label(), "Screen Capture");
    assert_eq!(Subsystem::SystemResources.label(), "System Resources");
    assert_eq!(Subsystem::Systemd.label(), "Systemd");
    assert_eq!(Subsystem::DockerTracker.label(), "Docker Tracker");
}

#[test]
fn awaiting_poll_interval_compares_by_subsystem() {
    assert_eq!(
        ChatState::AwaitingPollInterval {
            subsystem: Subsystem::Systemd
        },
        ChatState::AwaitingPollInterval {
            subsystem: Subsystem::Systemd
        }
    );
    assert_ne!(
        ChatState::AwaitingPollInterval {
            subsystem: Subsystem::Systemd
        },
        ChatState::AwaitingPollInterval {
            subsystem: Subsystem::DockerTracker
        }
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// State: defaults
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_state_knows_no_chats() {
    let s = state(true);
    assert!(s.get_chat_ids().is_empty());
    assert!(s.get_authed_chat_ids().is_empty());
    assert!(s.get_relevant_chat_ids().is_empty());
}

#[test]
fn unknown_chat_is_idle_and_unauthenticated() {
    let s = state(true);
    assert_eq!(s.get_chat_state(ChatId(1)), ChatState::Idle);
    assert_eq!(s.get_chat_auth(ChatId(1)), AuthState::Unauthenticated);
}

#[test]
fn reading_an_unknown_chat_does_not_register_it() {
    let s = state(true);
    let _ = s.get_chat_state(ChatId(1));
    let _ = s.get_chat_auth(ChatId(1));
    let _ = s.is_authorized(ChatId(1));
    assert!(s.get_chat_ids().is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// State: setters
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn set_chat_state_registers_an_unauthenticated_chat() {
    let s = state(true);
    s.set_chat_state(ChatId(7), ChatState::AwaitingUnitName);
    assert_eq!(s.get_chat_state(ChatId(7)), ChatState::AwaitingUnitName);
    assert_eq!(s.get_chat_auth(ChatId(7)), AuthState::Unauthenticated);
    assert_eq!(sorted(s.get_chat_ids()), vec![7]);
}

#[test]
fn set_chat_auth_registers_an_idle_chat() {
    let s = state(true);
    s.set_chat_auth(ChatId(7), AuthState::Authenticated);
    assert_eq!(s.get_chat_auth(ChatId(7)), AuthState::Authenticated);
    assert_eq!(s.get_chat_state(ChatId(7)), ChatState::Idle);
}

#[test]
fn state_and_auth_are_independent() {
    let s = state(true);
    s.set_chat_auth(ChatId(7), AuthState::Authenticated);
    s.set_chat_state(
        ChatId(7),
        ChatState::AwaitingPollInterval {
            subsystem: Subsystem::Systemd,
        },
    );
    assert_eq!(s.get_chat_auth(ChatId(7)), AuthState::Authenticated);
    s.set_chat_state_idle(ChatId(7));
    assert_eq!(s.get_chat_state(ChatId(7)), ChatState::Idle);
    assert_eq!(s.get_chat_auth(ChatId(7)), AuthState::Authenticated);
}

#[test]
fn set_chat_state_idle_resets_a_waiting_state() {
    let s = state(true);
    s.set_chat_state(ChatId(1), ChatState::AwaitingAuthToken);
    s.set_chat_state_idle(ChatId(1));
    assert_eq!(s.get_chat_state(ChatId(1)), ChatState::Idle);
}

#[test]
fn deauthenticating_works() {
    let s = state(true);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.set_chat_auth(ChatId(1), AuthState::Unauthenticated);
    assert_eq!(s.get_chat_auth(ChatId(1)), AuthState::Unauthenticated);
}

#[test]
fn chats_do_not_affect_each_other() {
    let s = state(true);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.set_chat_state(ChatId(2), ChatState::AwaitingAuthToken);
    assert_eq!(s.get_chat_auth(ChatId(2)), AuthState::Unauthenticated);
    assert_eq!(s.get_chat_state(ChatId(1)), ChatState::Idle);
    assert_eq!(sorted(s.get_chat_ids()), vec![1, 2]);
}

#[test]
fn negative_chat_ids_work() {
    // Group chats have negative ids.
    let s = state(true);
    s.set_chat_auth(ChatId(-100_123), AuthState::Authenticated);
    assert_eq!(sorted(s.get_authed_chat_ids()), vec![-100_123]);
}

#[test]
fn remove_chat_forgets_everything_about_it() {
    let s = state(true);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.set_chat_state(ChatId(1), ChatState::AwaitingUnitName);
    s.remove_chat(ChatId(1));
    assert!(s.get_chat_ids().is_empty());
    assert_eq!(s.get_chat_auth(ChatId(1)), AuthState::Unauthenticated);
    assert_eq!(s.get_chat_state(ChatId(1)), ChatState::Idle);
}

#[test]
fn removing_an_unknown_chat_is_a_no_op() {
    let s = state(true);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.remove_chat(ChatId(99));
    assert_eq!(sorted(s.get_chat_ids()), vec![1]);
}

#[test]
fn clones_share_the_same_chats() {
    let a = state(true);
    let b = a.clone();
    a.set_chat_auth(ChatId(1), AuthState::Authenticated);
    assert_eq!(b.get_chat_auth(ChatId(1)), AuthState::Authenticated);
    b.remove_chat(ChatId(1));
    assert!(a.get_chat_ids().is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// State: who gets events, who is authorised
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn authed_chat_ids_only_lists_authenticated_chats() {
    let s = state(true);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.set_chat_auth(ChatId(2), AuthState::Unauthenticated);
    s.set_chat_state(ChatId(3), ChatState::Idle);
    assert_eq!(sorted(s.get_authed_chat_ids()), vec![1]);
}

#[test]
fn relevant_chats_with_auth_on_are_only_the_authenticated_ones() {
    let s = state(true);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.set_chat_auth(ChatId(2), AuthState::Unauthenticated);
    assert_eq!(sorted(s.get_relevant_chat_ids()), vec![1]);
}

#[test]
fn relevant_chats_with_auth_off_are_every_known_chat() {
    // Pinned (finding 42): with auth off, any chat that registered
    // (for example by sending /start) receives every event.
    let s = state(false);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    s.set_chat_auth(ChatId(2), AuthState::Unauthenticated);
    assert_eq!(sorted(s.get_relevant_chat_ids()), vec![1, 2]);
}

#[test]
fn is_authorized_with_auth_on_needs_authentication() {
    let s = state(true);
    assert!(!s.is_authorized(ChatId(1)));
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    assert!(s.is_authorized(ChatId(1)));
    assert!(!s.is_authorized(ChatId(2)));
}

#[test]
fn is_authorized_with_auth_off_is_true_for_everyone() {
    // Pinned (finding 42): the bot is public on Telegram, so with
    // `--enable-auth` off any user who finds it can use the read-only menus.
    let s = state(false);
    assert!(s.is_authorized(ChatId(1)));
    assert!(s.is_authorized(ChatId(-5)));
}

#[test]
fn is_authorized_to_command_ignores_the_auth_flag() {
    // Commands (inline buttons, pause/resume, intervals) always need a login,
    // even when `--enable-auth` is off (same family as finding 18).
    let off = state(false);
    assert!(!off.is_authorized_to_commmand(ChatId(1)));
    off.set_chat_auth(ChatId(1), AuthState::Authenticated);
    assert!(off.is_authorized_to_commmand(ChatId(1)));

    let on = state(true);
    assert!(!on.is_authorized_to_commmand(ChatId(1)));
    on.set_chat_auth(ChatId(1), AuthState::Authenticated);
    assert!(on.is_authorized_to_commmand(ChatId(1)));
}

#[test]
fn get_user_is_none_when_no_users_are_loaded() {
    // Nothing in the test process calls `load_users()`.
    assert!(crate::prelude::get_users().is_none());
    assert!(State::get_user(ChatId(1)).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// State: poisoned lock
// ─────────────────────────────────────────────────────────────────────────────

fn poisoned(auth_enabled: bool) -> State {
    let s = state(auth_enabled);
    s.set_chat_auth(ChatId(1), AuthState::Authenticated);
    let chats = Arc::clone(&s.chats);
    let _ = std::thread::spawn(move || {
        let _guard = chats.lock().unwrap();
        panic!("poison the lock");
    })
    .join();
    assert!(s.chats.lock().is_err(), "the lock should be poisoned");
    s
}

#[test]
fn poisoned_lock_hides_every_chat() {
    // Pinned: a panic while the lock is held makes every read return the
    // default, so all chats look unauthenticated and idle.
    let s = poisoned(true);
    assert!(s.get_chat_ids().is_empty());
    assert!(s.get_authed_chat_ids().is_empty());
    assert!(s.get_relevant_chat_ids().is_empty());
    assert_eq!(s.get_chat_auth(ChatId(1)), AuthState::Unauthenticated);
    assert_eq!(s.get_chat_state(ChatId(1)), ChatState::Idle);
    assert!(!s.is_authorized(ChatId(1)));
    assert!(!s.is_authorized_to_commmand(ChatId(1)));
}

#[test]
fn poisoned_lock_makes_writes_silent_no_ops() {
    let s = poisoned(true);
    s.set_chat_auth(ChatId(2), AuthState::Authenticated);
    s.set_chat_state(ChatId(2), ChatState::AwaitingAuthToken);
    s.remove_chat(ChatId(1));
    assert!(s.get_chat_ids().is_empty());
}

#[test]
fn poisoned_lock_with_auth_off_still_authorises_everyone() {
    let s = poisoned(false);
    assert!(s.is_authorized(ChatId(1)));
}

// ─────────────────────────────────────────────────────────────────────────────
// ProcessCallbackAction
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn process_actions_encode_exactly() {
    assert_eq!(
        ProcessCallbackAction::Track { pid: 1234 }.encode(),
        "track:1234"
    );
    assert_eq!(
        ProcessCallbackAction::Untrack { pid: 1234 }.encode(),
        "untrack:1234"
    );
    assert_eq!(
        ProcessCallbackAction::KillTree { pid: 1234 }.encode(),
        "killtree:1234"
    );
    assert_eq!(
        ProcessCallbackAction::Signal {
            pid: 1234,
            signal: sig("kill")
        }
        .encode(),
        "signal:1234:kill"
    );
}

#[test]
fn process_actions_round_trip() {
    for pid in [0_u32, 1, 1234, u32::MAX] {
        let actions = [
            ProcessCallbackAction::Track { pid },
            ProcessCallbackAction::Untrack { pid },
            ProcessCallbackAction::KillTree { pid },
            ProcessCallbackAction::Signal {
                pid,
                signal: sig("kill"),
            },
        ];
        for action in actions {
            let encoded = action.encode();
            assert_eq!(ProcessCallbackAction::decode(&encoded), Some(action));
        }
    }
}

#[test]
fn every_supported_signal_round_trips() {
    let signals = ProcessSignal::get_supported_signals();
    assert!(!signals.is_empty());
    for signal in signals {
        let action = ProcessCallbackAction::Signal { pid: 42, signal };
        let encoded = action.encode();
        assert_eq!(ProcessCallbackAction::decode(&encoded), Some(action));
    }
}

#[test]
fn process_decode_rejects_bad_input() {
    for bad in [
        "",
        "track",
        "track:",
        "track:abc",
        "track:-1",
        "track:1.5",
        "track:4294967296",
        "track:1:2",
        "TRACK:1",
        " track:1",
        "track:1 ",
        "untrack",
        "killtree:",
        "signal",
        "signal:1",
        "signal:1:",
        "signal:1:KILL",
        "signal:1:nope",
        "signal:x:kill",
        "signal:1:kill:extra",
        "noop",
        "dc_stop:1",
    ] {
        assert_eq!(ProcessCallbackAction::decode(bad), None, "input {bad:?}");
    }
}

#[test]
fn process_decode_accepts_a_leading_plus_in_the_pid() {
    // Pinned: `u32::from_str` accepts "+5". Harmless, buttons never send it.
    assert_eq!(
        ProcessCallbackAction::decode("track:+5"),
        Some(ProcessCallbackAction::Track { pid: 5 })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemResourcesCallbackAction
// ─────────────────────────────────────────────────────────────────────────────

fn thresholds(cpu: f32, mem: f32, disk: f32, batt: f32) -> Thresholds {
    Thresholds {
        cpu_warn: cpu,
        memory_warn: mem,
        disk_warn: disk,
        battery_low: batt,
    }
}

#[test]
fn thresholds_encode_exactly() {
    let a = SystemResourcesCallbackAction::SetThresholds(thresholds(75.0, 75.0, 75.0, 25.0));
    assert_eq!(a.encode(), "sr_set_thresholds:75:75:75:25");
    let b = SystemResourcesCallbackAction::SetThresholds(thresholds(12.5, 50.0, 75.0, 10.0));
    assert_eq!(b.encode(), "sr_set_thresholds:12.5:50:75:10");
}

#[test]
fn thresholds_round_trip() {
    for t in [
        thresholds(75.0, 75.0, 75.0, 25.0),
        thresholds(12.5, 50.0, 75.25, 10.0),
        thresholds(0.0, 0.0, 0.0, 0.0),
        thresholds(100.0, 100.0, 100.0, 100.0),
    ] {
        let action = SystemResourcesCallbackAction::SetThresholds(t);
        let encoded = action.encode();
        assert_eq!(
            SystemResourcesCallbackAction::decode(&encoded),
            Some(action)
        );
    }
}

#[test]
fn default_thresholds_round_trip() {
    let action = SystemResourcesCallbackAction::SetThresholds(Thresholds::default());
    let encoded = action.encode();
    assert_eq!(
        SystemResourcesCallbackAction::decode(&encoded),
        Some(action)
    );
}

#[test]
fn thresholds_decode_accepts_out_of_range_values() {
    // Pinned (finding 7 family): no range check on the decoded numbers.
    for text in [
        "sr_set_thresholds:-5:50:50:50",
        "sr_set_thresholds:1000:50:50:50",
        "sr_set_thresholds:inf:50:50:50",
    ] {
        assert!(
            matches!(
                SystemResourcesCallbackAction::decode(text),
                Some(SystemResourcesCallbackAction::SetThresholds(_))
            ),
            "input {text:?}"
        );
    }
    match SystemResourcesCallbackAction::decode("sr_set_thresholds:nan:50:50:50") {
        Some(SystemResourcesCallbackAction::SetThresholds(t)) => assert!(t.cpu_warn.is_nan()),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn refresh_mask_cpu_and_memory_only_encodes_exactly() {
    let m = RefreshMask {
        cpu: true,
        memory: true,
        disks: false,
        networks: false,
        temperatures: false,
        gpus: false,
    };
    assert_eq!(
        SystemResourcesCallbackAction::SetRefreshMask(m).encode(),
        "sr_set_refresh_mask:1:1:0:0:0:0"
    );
}

#[test]
fn every_refresh_mask_combination_round_trips() {
    for bits in 0_u8..64 {
        let encoded = SystemResourcesCallbackAction::SetRefreshMask(mask_from_bits(bits)).encode();
        assert_eq!(
            SystemResourcesCallbackAction::decode(&encoded),
            Some(SystemResourcesCallbackAction::SetRefreshMask(
                mask_from_bits(bits)
            )),
            "bits {bits:#08b}"
        );
    }
}

#[test]
fn default_refresh_mask_round_trips() {
    let action = SystemResourcesCallbackAction::SetRefreshMask(RefreshMask::default());
    let encoded = action.encode();
    assert_eq!(
        SystemResourcesCallbackAction::decode(&encoded),
        Some(action)
    );
}

#[test]
fn refresh_mask_decode_treats_anything_but_zero_as_on() {
    // Pinned: `*flag != "0"`, so "", "2", "false" and "x" all mean on.
    for on in ["1", "2", "", "false", "x"] {
        let text = format!("sr_set_refresh_mask:{on}:0:0:0:0:0");
        match SystemResourcesCallbackAction::decode(&text) {
            Some(SystemResourcesCallbackAction::SetRefreshMask(m)) => {
                assert!(m.cpu, "flag {on:?} should read as on");
                assert!(!m.memory);
            }
            other => panic!("unexpected for {on:?}: {other:?}"),
        }
    }
}

#[test]
fn refresh_mask_decode_with_a_seventh_flag_folds_it_into_the_last() {
    // Pinned: `splitn(7)` leaves "1:0" in the last slot, which is not "0".
    match SystemResourcesCallbackAction::decode("sr_set_refresh_mask:0:0:0:0:0:0:1") {
        Some(SystemResourcesCallbackAction::SetRefreshMask(m)) => assert!(m.gpus),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn system_resources_decode_rejects_bad_input() {
    for bad in [
        "",
        "sr_set_thresholds",
        "sr_set_thresholds:1:2:3",
        "sr_set_thresholds:1:2:3:4:5",
        "sr_set_thresholds:a:2:3:4",
        "sr_set_thresholds::2:3:4",
        "SR_SET_THRESHOLDS:1:2:3:4",
        "sr_set_refresh_mask",
        "sr_set_refresh_mask:1:1:1:1:1",
        "sr_other:1:2:3:4",
        "noop",
    ] {
        assert!(
            SystemResourcesCallbackAction::decode(bad).is_none(),
            "input {bad:?}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemdCallbackAction
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn systemd_action_encodes_exactly() {
    let a = SystemdCallbackAction::Control {
        unit_name: "nginx.service".to_string(),
        action: ServiceAction::Restart,
    };
    assert_eq!(a.encode(), "sd_control:nginx.service:restart");
}

#[test]
fn systemd_actions_round_trip_for_every_service_action() {
    for action in service_actions() {
        let a = SystemdCallbackAction::Control {
            unit_name: "nginx.service".to_string(),
            action,
        };
        let encoded = a.encode();
        assert_eq!(SystemdCallbackAction::decode(&encoded), Some(a));
    }
}

#[test]
fn systemd_round_trips_templated_and_escaped_unit_names() {
    for name in [
        "getty@tty1.service",
        r"dev-disk-by\x2duuid-abcd.device",
        "user@1000.service",
        "a-b_c.timer",
    ] {
        let a = SystemdCallbackAction::Control {
            unit_name: name.to_string(),
            action: ServiceAction::Start,
        };
        let encoded = a.encode();
        assert_eq!(SystemdCallbackAction::decode(&encoded), Some(a), "{name}");
    }
}

#[test]
fn systemd_decode_rejects_bad_input() {
    for bad in [
        "",
        "sd_control",
        "sd_control:nginx.service",
        "sd_control:nginx.service:explode",
        "SD_CONTROL:nginx.service:restart",
        "sd_other:nginx.service:restart",
        "noop",
    ] {
        assert_eq!(SystemdCallbackAction::decode(bad), None, "input {bad:?}");
    }
}

#[test]
fn systemd_decode_accepts_an_empty_unit_name() {
    // Pinned: no check on the unit name.
    assert_eq!(
        SystemdCallbackAction::decode("sd_control::restart"),
        Some(SystemdCallbackAction::Control {
            unit_name: String::new(),
            action: ServiceAction::Restart,
        })
    );
}

#[test]
fn systemd_unit_name_with_a_colon_cannot_be_decoded() {
    // Pinned (finding 44): `splitn(3, ':')` puts "a:b.service:restart" apart
    // wrongly, so the action becomes "b.service:restart" and parsing fails.
    // Systemd allows ':' in unit names. Flip this if the format changes.
    let a = SystemdCallbackAction::Control {
        unit_name: "a:b.service".to_string(),
        action: ServiceAction::Restart,
    };
    assert_eq!(SystemdCallbackAction::decode(&a.encode()), None);
}

#[test]
#[ignore = "finding 44: unit names containing ':' do not survive the callback encoding"]
fn systemd_unit_name_with_a_colon_round_trips() {
    let a = SystemdCallbackAction::Control {
        unit_name: "a:b.service".to_string(),
        action: ServiceAction::Restart,
    };
    assert_eq!(SystemdCallbackAction::decode(&a.encode()), Some(a));
}

// ─────────────────────────────────────────────────────────────────────────────
// DockerCallbackAction
// ─────────────────────────────────────────────────────────────────────────────

fn docker_actions(id: &str) -> [DockerCallbackAction; 6] {
    let id = id.to_string();
    [
        DockerCallbackAction::Stop { id: id.clone() },
        DockerCallbackAction::Start { id: id.clone() },
        DockerCallbackAction::Kill { id: id.clone() },
        DockerCallbackAction::Restart { id: id.clone() },
        DockerCallbackAction::Pause { id: id.clone() },
        DockerCallbackAction::Unpause { id },
    ]
}

#[test]
fn docker_actions_encode_exactly() {
    let encoded: Vec<String> = docker_actions("abc123")
        .iter()
        .map(DockerCallbackAction::encode)
        .collect();
    assert_eq!(
        encoded,
        vec![
            "dc_stop:abc123",
            "dc_start:abc123",
            "dc_kill:abc123",
            "dc_restart:abc123",
            "dc_pause:abc123",
            "dc_unpause:abc123",
        ]
    );
}

#[test]
fn docker_actions_round_trip() {
    for id in ["abc123def456", "x", "with:colon", "UPPER", ""] {
        for action in docker_actions(id) {
            let encoded = action.encode();
            assert_eq!(
                DockerCallbackAction::decode(&encoded),
                Some(action),
                "{id:?}"
            );
        }
    }
}

#[test]
fn docker_decode_rejects_bad_input() {
    for bad in [
        "",
        "dc_stop",
        "dc_explode:abc",
        "DC_STOP:abc",
        "stop:abc",
        "noop",
        "track:1",
    ] {
        assert_eq!(DockerCallbackAction::decode(bad), None, "input {bad:?}");
    }
}

#[test]
fn docker_decode_accepts_an_empty_id() {
    // Pinned: "dc_stop:" decodes to `Stop { id: "" }` (the handler then asks
    // Docker to stop the container named "").
    assert_eq!(
        DockerCallbackAction::decode("dc_stop:"),
        Some(DockerCallbackAction::Stop { id: String::new() })
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Dispatch order in `handle_callback_query`
// ─────────────────────────────────────────────────────────────────────────────

/// Which of the four decoders (process, system resources, systemd, docker)
/// accept the text.
fn accepting_decoders(text: &str) -> [bool; 4] {
    [
        ProcessCallbackAction::decode(text).is_some(),
        SystemResourcesCallbackAction::decode(text).is_some(),
        SystemdCallbackAction::decode(text).is_some(),
        DockerCallbackAction::decode(text).is_some(),
    ]
}

#[test]
fn each_encoded_action_is_accepted_by_exactly_its_own_decoder() {
    // `handle_callback_query` tries the decoders in order, so a prefix clash
    // would send a button to the wrong handler.
    let mut cases: Vec<(String, usize)> = Vec::new();
    cases.push((ProcessCallbackAction::Track { pid: 1 }.encode(), 0));
    cases.push((ProcessCallbackAction::Untrack { pid: 1 }.encode(), 0));
    cases.push((ProcessCallbackAction::KillTree { pid: 1 }.encode(), 0));
    cases.push((
        ProcessCallbackAction::Signal {
            pid: 1,
            signal: sig("kill"),
        }
        .encode(),
        0,
    ));
    cases.push((
        SystemResourcesCallbackAction::SetThresholds(thresholds(1.0, 2.0, 3.0, 4.0)).encode(),
        1,
    ));
    cases.push((
        SystemResourcesCallbackAction::SetRefreshMask(mask_from_bits(5)).encode(),
        1,
    ));
    for action in service_actions() {
        cases.push((
            SystemdCallbackAction::Control {
                unit_name: "x.service".to_string(),
                action,
            }
            .encode(),
            2,
        ));
    }
    for action in docker_actions("abc") {
        cases.push((action.encode(), 3));
    }
    for (text, owner) in cases {
        let got = accepting_decoders(&text);
        for (i, accepted) in got.iter().enumerate() {
            assert_eq!(*accepted, i == owner, "text {text:?}, decoder {i}");
        }
    }
}

#[test]
fn the_noop_button_data_matches_no_decoder() {
    // Pinned (finding 44): the "✅ Tracking" / "🚫 Untracked" buttons carry
    // "noop", which no decoder knows, so `handle_callback_query` answers
    // "Unknown action" when one is pressed.
    assert_eq!(accepting_decoders("noop"), [false; 4]);
}
