use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, from_value, json};
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::time::Duration;

use kw_types::socket::*;
use kw_types::{Info, docker, event, history, polling, process, resources, screen, systemd};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// value -> JSON -> value -> JSON. The two JSON values and the two `Debug`
/// strings must match.
fn assert_roundtrip<T: Serialize + DeserializeOwned + Debug>(v: &T) {
    let first = serde_json::to_value(v).unwrap();
    let back: T = serde_json::from_value(first.clone()).unwrap();
    let second = serde_json::to_value(&back).unwrap();
    assert_eq!(first, second, "JSON changed on roundtrip for {v:?}");
    assert_eq!(
        format!("{v:?}"),
        format!("{back:?}"),
        "Debug changed on roundtrip"
    );
}

/// The variant name of an externally-tagged enum value:
/// `"Info"` -> `Info`, `{"Root": {...}}` -> `Root`.
fn tag_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(m) if m.len() == 1 => m.keys().next().unwrap().clone(),
        other => panic!("not an externally tagged enum value: {other}"),
    }
}

fn tags<T: Serialize>(items: &[T]) -> Vec<String> {
    items
        .iter()
        .map(|i| tag_of(&serde_json::to_value(i).unwrap()))
        .collect()
}

fn assert_unique(tags: &[String]) {
    let set: BTreeSet<&String> = tags.iter().collect();
    assert_eq!(
        set.len(),
        tags.len(),
        "duplicate variant in sample list: {tags:?}"
    );
}

fn tag_set<T: Serialize>(items: &[T]) -> BTreeSet<String> {
    tags(items).into_iter().collect()
}

// ---------------------------------------------------------------------------
// builders (small, field values don't matter, only the wiring)
// ---------------------------------------------------------------------------

fn poll_status() -> polling::PollStatus {
    polling::PollStatus {
        interval: 1000,
        paused: false,
    }
}

fn info() -> Info {
    Info {
        auth_enabled: true,
        shutdown_enabled: false,
        blind: false,
        pid: vec![1, 2],
        top_processes: true,
        limit_processes: 10,
        telegram_bot: false,
        system_resources: true,
        systemd: true,
        docker: true,
        allow_process_commands: true,
        allow_screen_commands: false,
        allow_system_resources_commands: true,
        allow_systemd_commands: false,
        allow_docker_commands: true,
    }
}

fn stored_event() -> event::StoredEvent {
    event::StoredEvent {
        event: "systemd.tick".into(),
        timestamp: "2026-01-01T00:00:00Z".into(),
        source: event::EventSource::Systemd,
        data: json!({ "k": 1 }),
    }
}

fn screenshot() -> screen::Screenshot {
    screen::Screenshot {
        image: vec![1, 2, 3],
        monitor_name: "DP-1".into(),
        monitor_id: 1,
        width: 1920,
        height: 1080,
        timestamp: "t".into(),
    }
}

fn process_snapshot() -> process::ProcessSnapshot {
    process::ProcessSnapshot {
        pid: 42,
        name: "bash".into(),
        state: process::ProcessState::Running,
        cpu_usage: 1.5,
        memory_bytes: 1024,
        disk_usage: 0,
        cwd: None,
        cmdline: vec![],
        open_files: vec![],
        io_stats: None,
    }
}

fn process_tree() -> process::ProcessTree {
    process::ProcessTree {
        root_pid: 42,
        root: Some(process_snapshot()),
        children: vec![],
        child_count: 0,
        work_done: false,
        timestamp: "t".into(),
    }
}

fn cpu_snapshot() -> resources::CpuSnapshot {
    resources::CpuSnapshot {
        usage_percent: 10.0,
        cores: vec![],
        frequency_mhz: 3000,
        brand: "X".into(),
        physical_core_count: Some(4),
        load_avg: None,
    }
}

fn memory_snapshot() -> resources::MemorySnapshot {
    resources::MemorySnapshot {
        total_bytes: 100,
        used_bytes: 50,
        available_bytes: 50,
        free_bytes: 50,
        used_percent: 50.0,
        swap_total_bytes: 0,
        swap_used_bytes: 0,
        swap_free_bytes: 0,
        swap_used_percent: None,
    }
}

fn host_info() -> resources::HostInfo {
    resources::HostInfo {
        hostname: Some("box".into()),
        os_name: None,
        kernel_version: None,
        cpu_arch: None,
        uptime_secs: 10,
        process_count: 5,
    }
}

fn disk_snapshot() -> resources::DiskSnapshot {
    resources::DiskSnapshot {
        name: "/dev/sda1".into(),
        mount_point: "/".into(),
        file_system: "ext4".into(),
        kind: resources::DiskKind::Ssd,
        is_removable: false,
        total_bytes: 100,
        used_bytes: 50,
        available_bytes: 50,
        used_percent: 50.0,
    }
}

fn network_snapshot() -> resources::NetworkSnapshot {
    resources::NetworkSnapshot {
        interface: "eth0".into(),
        rx_bytes_per_sec: 1,
        tx_bytes_per_sec: 2,
        rx_total_bytes: 3,
        tx_total_bytes: 4,
        rx_packets_per_sec: 5,
        tx_packets_per_sec: 6,
        rx_errors: 0,
        tx_errors: 0,
    }
}

fn gpu_snapshot() -> resources::GpuSnapshot {
    resources::GpuSnapshot {
        name: "GPU".into(),
        usage_percent: Some(5.0),
        vram_used_bytes: None,
        vram_total_bytes: None,
        vram_used_percent: None,
        temperature_celsius: None,
        power_draw_watts: None,
        power_limit_watts: None,
        fan_speed_percent: vec![],
    }
}

fn battery_snapshot() -> resources::BatterySnapshot {
    resources::BatterySnapshot {
        charge_percent: 80.0,
        state: resources::BatteryState::Charging,
        time_to_empty_secs: None,
        time_to_full_secs: Some(600),
        power_draw_watts: None,
        cycle_count: None,
        health_percent: None,
    }
}

fn thermal_snapshot() -> resources::ThermalSnapshot {
    resources::ThermalSnapshot {
        label: "coretemp".into(),
        temperature_celsius: Some(45.0),
        temperature_max_celsius: None,
        temperature_critical_celsius: None,
    }
}

fn alarm_snapshot() -> resources::AlarmSnapshot {
    let clear = || resources::AlarmStatus {
        active: false,
        since: None,
    };
    resources::AlarmSnapshot {
        cpu: clear(),
        memory: clear(),
        disks: vec![("/".into(), clear())],
        battery_low: clear(),
    }
}

fn system_snapshot() -> resources::SystemSnapshot {
    resources::SystemSnapshot {
        timestamp: "t".into(),
        cpu: cpu_snapshot(),
        memory: memory_snapshot(),
        disks: vec![],
        networks: vec![],
        gpus: vec![],
        battery: None,
        temperatures: vec![],
        host: host_info(),
        health: resources::SystemHealth::Healthy,
    }
}

fn unit_snapshot() -> systemd::UnitSnapshot {
    systemd::UnitSnapshot {
        unit_name: "nginx.service".into(),
        unit_type: systemd::UnitType::Service,
        load_state: systemd::UnitLoadState::Loaded,
        active_state: systemd::UnitActiveState::Active,
        sub_state: "running".into(),
        description: "web".into(),
        main_pid: Some(1),
        memory_bytes: None,
        cpu_usage_ns: None,
        restart_count: None,
        since: None,
        fragment_path: None,
    }
}

fn systemd_snapshot() -> systemd::SystemdSnapshot {
    systemd::SystemdSnapshot {
        timestamp: "t".into(),
        units: vec![unit_snapshot()],
        failed_count: 0,
        active_count: 1,
        inactive_count: 0,
    }
}

fn container_snapshot() -> docker::ContainerSnapshot {
    docker::ContainerSnapshot {
        id: "abc123def4567890".into(),
        short_id: "abc123def456".into(),
        name: "web".into(),
        image: "nginx".into(),
        status: docker::ContainerStatus::Running,
        health: docker::ContainerHealth::Healthy,
        stats: None,
    }
}

// ---------------------------------------------------------------------------
// sample lists: one value of every variant
// ---------------------------------------------------------------------------
// Each `_exhaustive` function fails to COMPILE when a variant is added to the
// enum, which is the reminder to add it to the list (and bump the count in the
// test) too.

#[allow(dead_code)]
fn query_exhaustive(q: &SocketQuery) {
    use SocketQuery as Q;
    match q {
        Q::Info
        | Q::Screenshots
        | Q::ScreenCapturePollStatus
        | Q::RootPids
        | Q::AllProcessTrees
        | Q::ProcessTrackerPollStatus
        | Q::SystemSnapshot
        | Q::Cpu
        | Q::Memory
        | Q::Disks
        | Q::Networks
        | Q::Gpus
        | Q::Battery
        | Q::HostInfo
        | Q::Temperatures
        | Q::Alarms
        | Q::Thresholds
        | Q::RefreshMask
        | Q::SystemResourcesPollStatus
        | Q::SystemdSnapshot
        | Q::FailedUnits
        | Q::SystemdPollStatus
        | Q::ListContainers
        | Q::DockerTrackerPollStatus => {}
        Q::History { .. }
        | Q::Root { .. }
        | Q::Children { .. }
        | Q::IsProcessDone { .. }
        | Q::ProcessTree { .. }
        | Q::ProcessStatus { .. }
        | Q::TopProcesses { .. }
        | Q::Unit { .. }
        | Q::UnitsByActiveState { .. }
        | Q::Container { .. }
        | Q::TopContainers { .. } => {}
    }
}

const QUERY_COUNT: usize = 35;

fn queries() -> Vec<SocketQuery> {
    use SocketQuery as Q;
    vec![
        Q::Info,
        Q::History {
            query: history::HistoryQuery::default(),
        },
        Q::Screenshots,
        Q::ScreenCapturePollStatus,
        Q::RootPids,
        Q::Root { root_pid: 1 },
        Q::Children { root_pid: 1 },
        Q::IsProcessDone { root_pid: 1 },
        Q::ProcessTree { root_pid: 1 },
        Q::AllProcessTrees,
        Q::ProcessStatus { root_pid: 1 },
        Q::TopProcesses {
            by: process::ProcessesSortKey::Cpu,
            limit: 10,
        },
        Q::ProcessTrackerPollStatus,
        Q::SystemSnapshot,
        Q::Cpu,
        Q::Memory,
        Q::Disks,
        Q::Networks,
        Q::Gpus,
        Q::Battery,
        Q::HostInfo,
        Q::Temperatures,
        Q::Alarms,
        Q::Thresholds,
        Q::RefreshMask,
        Q::SystemResourcesPollStatus,
        Q::SystemdSnapshot,
        Q::Unit {
            unit_name: "nginx.service".into(),
        },
        Q::UnitsByActiveState {
            state: systemd::UnitActiveState::Failed,
        },
        Q::FailedUnits,
        Q::SystemdPollStatus,
        Q::ListContainers,
        Q::Container {
            id_or_name: "web".into(),
        },
        Q::TopContainers {
            by: docker::DockerSortKey::Memory,
            limit: 5,
        },
        Q::DockerTrackerPollStatus,
    ]
}

#[allow(dead_code)]
fn query_response_exhaustive(r: &SocketQueryResponse) {
    use SocketQueryResponse as R;
    match r {
        R::Info { .. }
        | R::History { .. }
        | R::Screenshots { .. }
        | R::ScreenCapturePollStatus { .. }
        | R::RootPids { .. }
        | R::Root { .. }
        | R::Children { .. }
        | R::IsProcessDone { .. }
        | R::ProcessTree { .. }
        | R::AllProcessTrees { .. }
        | R::ProcessStatus { .. }
        | R::TopProcesses { .. }
        | R::ProcessTrackerPollStatus { .. }
        | R::SystemSnapshot { .. }
        | R::Cpu { .. }
        | R::Memory { .. }
        | R::Disks { .. }
        | R::Networks { .. }
        | R::Gpus { .. }
        | R::Battery { .. }
        | R::HostInfo { .. }
        | R::Temperatures { .. }
        | R::Alarms { .. }
        | R::Thresholds { .. }
        | R::RefreshMask { .. }
        | R::SystemResourcesPollStatus { .. }
        | R::SystemdSnapshot { .. }
        | R::Unit { .. }
        | R::UnitsByActiveState { .. }
        | R::FailedUnits { .. }
        | R::SystemdPollStatus { .. }
        | R::ListContainers { .. }
        | R::Container { .. }
        | R::TopContainers { .. }
        | R::DockerTrackerPollStatus { .. } => {}
    }
}

const QUERY_RESPONSE_COUNT: usize = 35;

fn query_responses() -> Vec<SocketQueryResponse> {
    use SocketQueryResponse as R;
    vec![
        R::Info { info: info() },
        R::History {
            events: vec![stored_event()],
        },
        R::Screenshots {
            screenshots: vec![screenshot()],
        },
        R::ScreenCapturePollStatus {
            status: Some(poll_status()),
        },
        R::RootPids { pids: vec![1, 2] },
        R::Root {
            snapshot: Some(process_snapshot()),
        },
        R::Children {
            snapshots: vec![process_snapshot()],
        },
        R::IsProcessDone { done: Some(true) },
        R::ProcessTree {
            tree: Some(process_tree()),
        },
        R::AllProcessTrees {
            trees: vec![process_tree()],
        },
        R::ProcessStatus {
            status: Some(process::ProcessStatus::default()),
        },
        R::TopProcesses {
            snapshots: vec![process_snapshot()],
        },
        R::ProcessTrackerPollStatus {
            status: Some(poll_status()),
        },
        R::SystemSnapshot {
            snapshot: Box::new(Some(system_snapshot())),
        },
        R::Cpu {
            snapshot: Some(cpu_snapshot()),
        },
        R::Memory {
            snapshot: Some(memory_snapshot()),
        },
        R::Disks {
            snapshots: vec![disk_snapshot()],
        },
        R::Networks {
            snapshots: vec![network_snapshot()],
        },
        R::Gpus {
            snapshots: vec![gpu_snapshot()],
        },
        R::Battery {
            snapshot: Some(battery_snapshot()),
        },
        R::HostInfo {
            info: Some(host_info()),
        },
        R::Temperatures {
            snapshots: vec![thermal_snapshot()],
        },
        R::Alarms {
            snapshot: Some(alarm_snapshot()),
        },
        R::Thresholds {
            thresholds: Some(resources::Thresholds::default()),
        },
        R::RefreshMask {
            refresh_mask: Some(resources::RefreshMask::default()),
        },
        R::SystemResourcesPollStatus { status: None },
        R::SystemdSnapshot {
            snapshot: Some(systemd_snapshot()),
        },
        R::Unit {
            snapshot: Some(unit_snapshot()),
        },
        R::UnitsByActiveState {
            snapshots: vec![unit_snapshot()],
        },
        R::FailedUnits { snapshots: vec![] },
        R::SystemdPollStatus {
            status: Some(poll_status()),
        },
        R::ListContainers {
            snapshots: vec![container_snapshot()],
        },
        R::Container {
            snapshot: Some(container_snapshot()),
        },
        R::TopContainers {
            snapshots: vec![container_snapshot()],
        },
        R::DockerTrackerPollStatus {
            status: Some(poll_status()),
        },
    ]
}

#[allow(dead_code)]
fn command_exhaustive(c: &SocketCommand) {
    use SocketCommand as C;
    match c {
        C::ScreenCapturePollPause
        | C::ScreenCapturePollResume
        | C::ProcessTrackerPollPause
        | C::ProcessTrackerPollResume
        | C::SystemResourcesPollPause
        | C::SystemResourcesPollResume
        | C::SystemdPollPause
        | C::SystemdPollResume
        | C::DockerTrackerPollPause
        | C::DockerTrackerPollResume => {}
        C::ScreenCapturePollInterval { .. }
        | C::KillProcess { .. }
        | C::KillTree { .. }
        | C::TrackPid { .. }
        | C::UntrackPid { .. }
        | C::ProcessTrackerPollInterval { .. }
        | C::SetThresholds { .. }
        | C::SetRefreshMask { .. }
        | C::SystemResourcesPollInterval { .. }
        | C::ControlUnit { .. }
        | C::SystemdPollInterval { .. }
        | C::StopContainer { .. }
        | C::KillContainer { .. }
        | C::StartContainer { .. }
        | C::RestartContainer { .. }
        | C::PauseContainer { .. }
        | C::UnpauseContainer { .. }
        | C::DockerTrackerPollInterval { .. } => {}
    }
}

const COMMAND_COUNT: usize = 28;

fn commands() -> Vec<SocketCommand> {
    use SocketCommand as C;
    let secs = |s| Duration::from_secs(s);
    vec![
        C::ScreenCapturePollInterval { interval: secs(5) },
        C::ScreenCapturePollPause,
        C::ScreenCapturePollResume,
        C::KillProcess {
            pid: 10,
            signal: process::ProcessSignal::Term,
        },
        C::KillTree { root_pid: 10 },
        C::TrackPid { pid: 10 },
        C::UntrackPid { pid: 10 },
        C::ProcessTrackerPollInterval { interval: secs(1) },
        C::ProcessTrackerPollPause,
        C::ProcessTrackerPollResume,
        C::SetThresholds {
            thresholds: resources::Thresholds::default(),
        },
        C::SetRefreshMask {
            refresh_mask: resources::RefreshMask::default(),
        },
        C::SystemResourcesPollInterval { interval: secs(2) },
        C::SystemResourcesPollPause,
        C::SystemResourcesPollResume,
        C::ControlUnit {
            unit_name: "nginx.service".into(),
            action: systemd::ServiceAction::Restart,
        },
        C::SystemdPollInterval { interval: secs(3) },
        C::SystemdPollPause,
        C::SystemdPollResume,
        C::StopContainer {
            id_or_name: "web".into(),
            timeout_secs: Some(10),
        },
        C::KillContainer {
            id_or_name: "web".into(),
            signal: Some("SIGKILL".into()),
        },
        C::StartContainer {
            id_or_name: "web".into(),
        },
        C::RestartContainer {
            id_or_name: "web".into(),
            timeout_secs: None,
        },
        C::PauseContainer {
            id_or_name: "web".into(),
        },
        C::UnpauseContainer {
            id_or_name: "web".into(),
        },
        C::DockerTrackerPollInterval { interval: secs(4) },
        C::DockerTrackerPollPause,
        C::DockerTrackerPollResume,
    ]
}

#[allow(dead_code)]
fn command_response_exhaustive(c: &SocketCommandResponse) {
    use SocketCommandResponse as R;
    match c {
        R::KillProcess { .. } | R::KillTree { .. } => {}
        R::ScreenPollInterval
        | R::ScreenPollPause
        | R::ScreenPollResume
        | R::TrackPid
        | R::UntrackPid
        | R::ProcessTrackerPollInterval
        | R::ProcessTrackerPollPause
        | R::ProcessTrackerPollResume
        | R::SetThresholds
        | R::SetRefreshMask
        | R::SystemResourcesPollInterval
        | R::SystemResourcesPollPause
        | R::SystemResourcesPollResume
        | R::ControlUnit
        | R::SystemdPollInterval
        | R::SystemdPollPause
        | R::SystemdPollResume
        | R::StopContainer
        | R::KillContainer
        | R::StartContainer
        | R::RestartContainer
        | R::PauseContainer
        | R::UnpauseContainer
        | R::DockerTrackerPollInterval
        | R::DockerTrackerPollPause
        | R::DockerTrackerPollResume => {}
    }
}

const COMMAND_RESPONSE_COUNT: usize = 28;

fn command_responses() -> Vec<SocketCommandResponse> {
    use SocketCommandResponse as R;
    vec![
        R::ScreenPollInterval,
        R::ScreenPollPause,
        R::ScreenPollResume,
        R::KillProcess { os_result: true },
        R::KillTree {
            killed_pids: vec![10, 11],
        },
        R::TrackPid,
        R::UntrackPid,
        R::ProcessTrackerPollInterval,
        R::ProcessTrackerPollPause,
        R::ProcessTrackerPollResume,
        R::SetThresholds,
        R::SetRefreshMask,
        R::SystemResourcesPollInterval,
        R::SystemResourcesPollPause,
        R::SystemResourcesPollResume,
        R::ControlUnit,
        R::SystemdPollInterval,
        R::SystemdPollPause,
        R::SystemdPollResume,
        R::StopContainer,
        R::KillContainer,
        R::StartContainer,
        R::RestartContainer,
        R::PauseContainer,
        R::UnpauseContainer,
        R::DockerTrackerPollInterval,
        R::DockerTrackerPollPause,
        R::DockerTrackerPollResume,
    ]
}

fn actions() -> Vec<SocketAction> {
    vec![
        SocketAction::Login {
            username: "admin".into(),
            password: "pw".into(),
        },
        SocketAction::Logout,
        SocketAction::Shutdown,
        SocketAction::SetEventPreferences {
            events_enabled: Some(true),
            ticks_enabled: None,
        },
    ]
}

/// `EventSource::ScreenCapture` is used on purpose: `source` is `#[serde(skip)]`,
/// so any other source would not compare equal after a roundtrip
/// (see `event_message_loses_source_on_the_wire`).
fn event_payload() -> event::EventPayload {
    event::EventPayload::new(
        "1".into(),
        event::EventSource::ScreenCapture,
        "screen.captured",
        "2026-01-01T00:00:00Z".into(),
        json!({ "monitors": 2 }),
    )
}

#[allow(dead_code)]
fn message_exhaustive(m: &SocketMessage) {
    use SocketMessage as M;
    match m {
        M::Handshake
        | M::HandshakeResponse
        | M::AuthenticationSucceed
        | M::ShutdownNotEnabled
        | M::Unauthorized
        | M::LoggedOut
        | M::NotLoggedIn
        | M::AlreadyAuthenticated
        | M::ShuttingDown
        | M::SetEventPreferences => {}
        M::Action { .. }
        | M::Query { .. }
        | M::QueryResponse { .. }
        | M::Command { .. }
        | M::CommandResponse { .. }
        | M::Event { .. }
        | M::AuthenticationFailed { .. } => {}
    }
}

const MESSAGE_COUNT: usize = 15;

fn unit_messages() -> Vec<(SocketMessage, &'static str)> {
    use SocketMessage as M;
    vec![
        (M::Handshake, "Handshake"),
        (M::HandshakeResponse, "HandshakeResponse"),
        (M::AuthenticationSucceed, "AuthenticationSucceed"),
        (M::ShutdownNotEnabled, "ShutdownNotEnabled"),
        (M::Unauthorized, "Unauthorized"),
        (M::AlreadyAuthenticated, "AlreadyAuthenticated"),
        (M::ShuttingDown, "ShuttingDown"),
        (M::SetEventPreferences, "SetEventPreferences"),
    ]
}

fn messages() -> Vec<SocketMessage> {
    use SocketMessage as M;
    let mut all: Vec<SocketMessage> = unit_messages().into_iter().map(|(m, _)| m).collect();
    all.extend([
        M::Action {
            action: SocketAction::Logout,
        },
        M::Query {
            query: SocketQuery::Info,
        },
        M::QueryResponse {
            response: SocketQueryResponse::RootPids { pids: vec![1] },
        },
        M::Command {
            command: SocketCommand::TrackPid { pid: 1 },
        },
        M::CommandResponse {
            success: false,
            err: Some("boom".into()),
            response: SocketCommandResponse::TrackPid,
        },
        M::Event {
            event: event_payload(),
        },
        M::AuthenticationFailed {
            reason: AuthFailedReason::WrongCredentials,
        },
    ]);
    all
}

// ---------------------------------------------------------------------------
// SocketMessage
// ---------------------------------------------------------------------------

mod message {
    use super::*;

    #[test]
    fn sample_list_covers_every_variant() {
        let all = messages();
        assert_eq!(all.len(), MESSAGE_COUNT, "update the sample list");
        assert_unique(&tags(&all));
    }

    #[test]
    fn every_variant_roundtrips() {
        for m in messages() {
            assert_roundtrip(&m);
        }
    }

    #[test]
    fn unit_variants_are_bare_strings() {
        for (m, name) in unit_messages() {
            assert_eq!(serde_json::to_value(&m).unwrap(), json!(name));
        }
    }

    #[test]
    fn struct_variants_have_the_expected_shape() {
        let q = SocketMessage::Query {
            query: SocketQuery::Info,
        };
        assert_eq!(
            serde_json::to_value(&q).unwrap(),
            json!({ "Query": { "query": "Info" } })
        );

        let c = SocketMessage::CommandResponse {
            success: true,
            err: None,
            response: SocketCommandResponse::TrackPid,
        };
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({ "CommandResponse": { "success": true, "err": null, "response": "TrackPid" } })
        );

        let a = SocketMessage::AuthenticationFailed {
            reason: AuthFailedReason::InternalServerError,
        };
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            json!({ "AuthenticationFailed": { "reason": "InternalServerError" } })
        );

        let login = SocketMessage::Action {
            action: SocketAction::Login {
                username: "admin".into(),
                password: "pw".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(&login).unwrap(),
            json!({ "Action": { "action": { "Login": { "username": "admin", "password": "pw" } } } })
        );
    }

    #[test]
    fn event_message_shape_has_no_source() {
        let m = SocketMessage::Event {
            event: event_payload(),
        };
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            json!({
                "Event": { "event": {
                    "version": "1",
                    "event": "screen.captured",
                    "timestamp": "2026-01-01T00:00:00Z",
                    "data": { "monitors": 2 }
                } }
            })
        );
    }

    #[test]
    fn event_message_loses_source_on_the_wire() {
        // Same `#[serde(skip)]` behaviour as in step 2, seen through the socket:
        // a client receiving a systemd event sees it as `ScreenCapture`.
        let sent = SocketMessage::Event {
            event: event::EventPayload::new(
                "1".into(),
                event::EventSource::Systemd,
                "systemd.unit_failed",
                "t".into(),
                json!({}),
            ),
        };
        let received: SocketMessage = from_value(serde_json::to_value(&sent).unwrap()).unwrap();
        match received {
            SocketMessage::Event { event } => {
                assert_eq!(event.event, "systemd.unit_failed");
                assert!(event.is_screen_capture());
                assert!(!event.is_systemd());
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn command_response_carries_err_text() {
        let m = SocketMessage::CommandResponse {
            success: false,
            err: Some("unit not found".into()),
            response: SocketCommandResponse::ControlUnit,
        };
        let back: SocketMessage = from_value(serde_json::to_value(&m).unwrap()).unwrap();
        match back {
            SocketMessage::CommandResponse { success, err, .. } => {
                assert!(!success);
                assert_eq!(err.as_deref(), Some("unit not found"));
            }
            other => panic!("expected CommandResponse, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_and_malformed_messages() {
        assert!(from_value::<SocketMessage>(json!("Nope")).is_err());
        assert!(from_value::<SocketMessage>(json!("handshake")).is_err());
        assert!(from_value::<SocketMessage>(json!({})).is_err());
        assert!(from_value::<SocketMessage>(json!(null)).is_err());
        assert!(from_value::<SocketMessage>(json!(["Handshake"])).is_err());
        // struct variant sent without its payload
        assert!(from_value::<SocketMessage>(json!("Query")).is_err());
        assert!(from_value::<SocketMessage>(json!({ "Query": {} })).is_err());
        // two variants in one object
        assert!(
            from_value::<SocketMessage>(json!({ "Handshake": null, "Unauthorized": null }))
                .is_err()
        );
    }

    #[test]
    fn messages_survive_the_text_encoding_used_on_a_line_or_frame() {
        // to_string -> from_str, not just to_value -> from_value
        for m in messages() {
            let text = serde_json::to_string(&m).unwrap();
            assert!(
                !text.contains('\n'),
                "compact JSON must be single-line: {text}"
            );
            let back: SocketMessage = serde_json::from_str(&text).unwrap();
            assert_eq!(
                tag_of(&serde_json::to_value(&back).unwrap()),
                tag_of(&serde_json::to_value(&m).unwrap())
            );
        }
    }

    #[test]
    fn auth_failed_reason_names() {
        for (r, name) in [
            (AuthFailedReason::WrongCredentials, "WrongCredentials"),
            (AuthFailedReason::InternalServerError, "InternalServerError"),
        ] {
            assert_eq!(serde_json::to_value(&r).unwrap(), json!(name));
            assert_roundtrip(&r);
        }
        assert!(from_value::<AuthFailedReason>(json!("Other")).is_err());
    }
}

// ---------------------------------------------------------------------------
// SocketAction
// ---------------------------------------------------------------------------

mod action {
    use super::*;

    #[test]
    fn every_variant_roundtrips() {
        for a in actions() {
            assert_roundtrip(&a);
        }
        assert_unique(&tags(&actions()));
    }

    #[test]
    fn shapes() {
        assert_eq!(
            serde_json::to_value(SocketAction::Logout).unwrap(),
            json!("Logout")
        );
        assert_eq!(
            serde_json::to_value(SocketAction::Shutdown).unwrap(),
            json!("Shutdown")
        );
        assert_eq!(
            serde_json::to_value(SocketAction::SetEventPreferences {
                events_enabled: Some(true),
                ticks_enabled: None,
            })
            .unwrap(),
            json!({ "SetEventPreferences": { "events_enabled": true, "ticks_enabled": null } })
        );
    }

    #[test]
    fn event_preferences_fields_are_optional_per_field() {
        // A client can change one preference and leave the other alone.
        let a: SocketAction =
            from_value(json!({ "SetEventPreferences": { "ticks_enabled": false } })).unwrap();
        match a {
            SocketAction::SetEventPreferences {
                events_enabled,
                ticks_enabled,
            } => {
                assert_eq!(events_enabled, None);
                assert_eq!(ticks_enabled, Some(false));
            }
            other => panic!("expected SetEventPreferences, got {other:?}"),
        }
    }

    #[test]
    fn login_requires_both_credentials() {
        assert!(from_value::<SocketAction>(json!({ "Login": { "username": "a" } })).is_err());
        assert!(from_value::<SocketAction>(json!({ "Login": { "password": "a" } })).is_err());
        assert!(from_value::<SocketAction>(json!("Login")).is_err());
    }

    #[test]
    #[ignore = "known issue: derive(Debug) on SocketAction::Login prints the password"]
    fn login_debug_does_not_leak_password() {
        let a = SocketAction::Login {
            username: "admin".into(),
            password: "hunter2".into(),
        };
        assert!(!format!("{a:?}").contains("hunter2"));
        // and therefore neither does the message that wraps it
        let m = SocketMessage::Action { action: a };
        assert!(!format!("{m:?}").contains("hunter2"));
    }
}

// ---------------------------------------------------------------------------
// SocketQuery / SocketQueryResponse
// ---------------------------------------------------------------------------

mod query {
    use super::*;

    #[test]
    fn query_sample_list_covers_every_variant() {
        let all = queries();
        assert_eq!(all.len(), QUERY_COUNT, "update the sample list");
        assert_unique(&tags(&all));
    }

    #[test]
    fn every_query_roundtrips() {
        for q in queries() {
            assert_roundtrip(&q);
        }
    }

    #[test]
    fn query_shapes() {
        assert_eq!(
            serde_json::to_value(SocketQuery::Info).unwrap(),
            json!("Info")
        );
        assert_eq!(
            serde_json::to_value(SocketQuery::Root { root_pid: 5 }).unwrap(),
            json!({ "Root": { "root_pid": 5 } })
        );
        assert_eq!(
            serde_json::to_value(SocketQuery::TopProcesses {
                by: process::ProcessesSortKey::Cpu,
                limit: 10,
            })
            .unwrap(),
            json!({ "TopProcesses": { "by": "cpu", "limit": 10 } })
        );
        assert_eq!(
            serde_json::to_value(SocketQuery::UnitsByActiveState {
                state: systemd::UnitActiveState::Failed,
            })
            .unwrap(),
            json!({ "UnitsByActiveState": { "state": "failed" } })
        );
        assert_eq!(
            serde_json::to_value(SocketQuery::TopContainers {
                by: docker::DockerSortKey::Memory,
                limit: 3,
            })
            .unwrap(),
            json!({ "TopContainers": { "by": "memory", "limit": 3 } })
        );
        assert_eq!(
            serde_json::to_value(SocketQuery::History {
                query: history::HistoryQuery::default(),
            })
            .unwrap(),
            json!({ "History": { "query": {
                "since": null, "until": null, "event": null, "source": null, "limit": null
            } } })
        );
    }

    #[test]
    fn history_query_accepts_a_partial_object() {
        let q: SocketQuery = from_value(json!({ "History": { "query": { "limit": 5 } } })).unwrap();
        match q {
            SocketQuery::History { query } => {
                assert_eq!(query.limit, Some(5));
                assert!(query.since.is_none());
            }
            other => panic!("expected History, got {other:?}"),
        }
    }

    #[test]
    fn rejects_bad_queries() {
        assert!(from_value::<SocketQuery>(json!("Nope")).is_err());
        assert!(from_value::<SocketQuery>(json!("info")).is_err());
        assert!(
            from_value::<SocketQuery>(json!("Root")).is_err(),
            "struct variant as bare string"
        );
        assert!(from_value::<SocketQuery>(json!({ "Root": {} })).is_err());
        assert!(from_value::<SocketQuery>(json!({ "Root": { "root_pid": -1 } })).is_err());
        assert!(from_value::<SocketQuery>(json!({ "Root": { "root_pid": "1" } })).is_err());
        assert!(
            from_value::<SocketQuery>(json!({ "TopProcesses": { "by": "gpu", "limit": 1 } }))
                .is_err()
        );
        assert!(
            from_value::<SocketQuery>(json!({ "TopProcesses": { "by": "cpu", "limit": -1 } }))
                .is_err()
        );
        // "disk" sorts processes, not containers
        assert!(
            from_value::<SocketQuery>(json!({ "TopContainers": { "by": "disk", "limit": 1 } }))
                .is_err()
        );
    }

    // ---- responses ---------------------------------------------------------

    #[test]
    fn response_sample_list_covers_every_variant() {
        let all = query_responses();
        assert_eq!(all.len(), QUERY_RESPONSE_COUNT, "update the sample list");
        assert_unique(&tags(&all));
    }

    #[test]
    fn every_response_roundtrips() {
        for r in query_responses() {
            assert_roundtrip(&r);
        }
    }

    #[test]
    fn every_query_has_a_response_with_the_same_name() {
        assert_eq!(tag_set(&queries()), tag_set(&query_responses()));
    }

    #[test]
    fn response_shapes() {
        assert_eq!(
            serde_json::to_value(SocketQueryResponse::RootPids { pids: vec![1, 2] }).unwrap(),
            json!({ "RootPids": { "pids": [1, 2] } })
        );
        assert_eq!(
            serde_json::to_value(SocketQueryResponse::Root { snapshot: None }).unwrap(),
            json!({ "Root": { "snapshot": null } })
        );
        assert_eq!(
            serde_json::to_value(SocketQueryResponse::IsProcessDone { done: Some(true) }).unwrap(),
            json!({ "IsProcessDone": { "done": true } })
        );
        assert_eq!(
            serde_json::to_value(SocketQueryResponse::Disks { snapshots: vec![] }).unwrap(),
            json!({ "Disks": { "snapshots": [] } })
        );
    }

    #[test]
    fn boxed_system_snapshot_serializes_like_a_plain_option() {
        let none = SocketQueryResponse::SystemSnapshot {
            snapshot: Box::new(None),
        };
        assert_eq!(
            serde_json::to_value(&none).unwrap(),
            json!({ "SystemSnapshot": { "snapshot": null } })
        );

        let some = SocketQueryResponse::SystemSnapshot {
            snapshot: Box::new(Some(system_snapshot())),
        };
        let v = serde_json::to_value(&some).unwrap();
        assert_eq!(v["SystemSnapshot"]["snapshot"]["timestamp"], json!("t"));
        assert_roundtrip(&some);
        assert_roundtrip(&none);
    }

    #[test]
    fn option_responses_roundtrip_as_none_too() {
        let nones = vec![
            SocketQueryResponse::ScreenCapturePollStatus { status: None },
            SocketQueryResponse::Root { snapshot: None },
            SocketQueryResponse::IsProcessDone { done: None },
            SocketQueryResponse::ProcessTree { tree: None },
            SocketQueryResponse::ProcessStatus { status: None },
            SocketQueryResponse::Cpu { snapshot: None },
            SocketQueryResponse::Memory { snapshot: None },
            SocketQueryResponse::Battery { snapshot: None },
            SocketQueryResponse::HostInfo { info: None },
            SocketQueryResponse::Alarms { snapshot: None },
            SocketQueryResponse::Thresholds { thresholds: None },
            SocketQueryResponse::RefreshMask { refresh_mask: None },
            SocketQueryResponse::SystemdSnapshot { snapshot: None },
            SocketQueryResponse::Unit { snapshot: None },
            SocketQueryResponse::Container { snapshot: None },
        ];
        for r in nones {
            assert_roundtrip(&r);
        }
    }

    #[test]
    fn screenshots_over_the_socket_serialize_bytes_as_a_number_array() {
        // `screen::Screenshot.image` is a Vec<u8>, which JSON writes as
        // [1,2,3,...] (~3-4 bytes of text per image byte), unlike the REST
        // `ScreenshotImage`, which carries base64. If you switch the socket to
        // base64 (or a binary frame), this test should flip.
        let r = SocketQueryResponse::Screenshots {
            screenshots: vec![screenshot()],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(
            v["Screenshots"]["screenshots"][0]["image"],
            json!([1, 2, 3])
        );
    }

    #[test]
    fn poll_status_interval_is_a_u128_that_roundtrips_within_u64_range() {
        for interval in [0_u128, 1, 1500, u128::from(u64::MAX)] {
            let r = SocketQueryResponse::SystemdPollStatus {
                status: Some(polling::PollStatus {
                    interval,
                    paused: true,
                }),
            };
            assert_roundtrip(&r);
        }
    }

    #[test]
    fn rejects_bad_responses() {
        assert!(from_value::<SocketQueryResponse>(json!("Info")).is_err());
        assert!(from_value::<SocketQueryResponse>(json!({ "RootPids": {} })).is_err());
        assert!(
            from_value::<SocketQueryResponse>(json!({ "RootPids": { "pids": [-1] } })).is_err()
        );
        assert!(from_value::<SocketQueryResponse>(json!({ "Nope": {} })).is_err());
    }
}

// ---------------------------------------------------------------------------
// SocketCommand / SocketCommandResponse
// ---------------------------------------------------------------------------

mod command {
    use super::*;

    #[test]
    fn command_sample_list_covers_every_variant() {
        let all = commands();
        assert_eq!(all.len(), COMMAND_COUNT, "update the sample list");
        assert_unique(&tags(&all));
    }

    #[test]
    fn every_command_roundtrips() {
        for c in commands() {
            assert_roundtrip(&c);
        }
    }

    #[test]
    fn command_shapes() {
        assert_eq!(
            serde_json::to_value(SocketCommand::ScreenCapturePollPause).unwrap(),
            json!("ScreenCapturePollPause")
        );
        assert_eq!(
            serde_json::to_value(SocketCommand::KillProcess {
                pid: 10,
                signal: process::ProcessSignal::Interrupt,
            })
            .unwrap(),
            json!({ "KillProcess": { "pid": 10, "signal": "int" } })
        );
        assert_eq!(
            serde_json::to_value(SocketCommand::ControlUnit {
                unit_name: "nginx.service".into(),
                action: systemd::ServiceAction::Reload,
            })
            .unwrap(),
            json!({ "ControlUnit": { "unit_name": "nginx.service", "action": "reload" } })
        );
        assert_eq!(
            serde_json::to_value(SocketCommand::StopContainer {
                id_or_name: "web".into(),
                timeout_secs: None,
            })
            .unwrap(),
            json!({ "StopContainer": { "id_or_name": "web", "timeout_secs": null } })
        );
        assert_eq!(
            serde_json::to_value(SocketCommand::SetThresholds {
                thresholds: resources::Thresholds::default(),
            })
            .unwrap(),
            json!({ "SetThresholds": { "thresholds": {
                "cpu_warn": 90.0, "memory_warn": 90.0, "disk_warn": 90.0, "battery_low": 15.0
            } } })
        );
    }

    #[test]
    fn poll_intervals_are_serde_durations_not_milliseconds() {
        // Non-Rust clients must send {secs, nanos}; a plain number is rejected.
        let c = SocketCommand::SystemdPollInterval {
            interval: Duration::from_millis(2500),
        };
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({ "SystemdPollInterval": { "interval": { "secs": 2, "nanos": 500_000_000 } } })
        );
        assert!(
            from_value::<SocketCommand>(json!({ "SystemdPollInterval": { "interval": 2500 } }))
                .is_err()
        );
        assert!(
            from_value::<SocketCommand>(
                json!({ "SystemdPollInterval": { "interval": { "secs": -1, "nanos": 0 } } })
            )
            .is_err()
        );
    }

    #[test]
    fn every_interval_command_uses_the_same_duration_shape() {
        let interval = json!({ "secs": 7, "nanos": 0 });
        for name in [
            "ScreenCapturePollInterval",
            "ProcessTrackerPollInterval",
            "SystemResourcesPollInterval",
            "SystemdPollInterval",
            "DockerTrackerPollInterval",
        ] {
            let c: SocketCommand =
                from_value(json!({ (name): { "interval": interval.clone() } })).unwrap();
            let v = serde_json::to_value(&c).unwrap();
            assert_eq!(v[name]["interval"], interval, "{name}");
        }
    }

    #[test]
    fn zero_interval_is_accepted_by_the_type_so_the_server_must_reject_it() {
        // tokio::time::interval panics on a zero period.
        let c: SocketCommand = from_value(
            json!({ "ProcessTrackerPollInterval": { "interval": { "secs": 0, "nanos": 0 } } }),
        )
        .unwrap();
        match c {
            SocketCommand::ProcessTrackerPollInterval { interval } => {
                assert_eq!(interval, Duration::ZERO)
            }
            other => panic!("expected ProcessTrackerPollInterval, got {other:?}"),
        }
    }

    #[test]
    fn optional_container_arguments_default_to_none_when_null() {
        let c: SocketCommand =
            from_value(json!({ "KillContainer": { "id_or_name": "web", "signal": null } }))
                .unwrap();
        match c {
            SocketCommand::KillContainer { signal, .. } => assert_eq!(signal, None),
            other => panic!("expected KillContainer, got {other:?}"),
        }
        let c: SocketCommand =
            from_value(json!({ "RestartContainer": { "id_or_name": "web" } })).unwrap();
        match c {
            SocketCommand::RestartContainer { timeout_secs, .. } => assert_eq!(timeout_secs, None),
            other => panic!("expected RestartContainer, got {other:?}"),
        }
    }

    #[test]
    fn rejects_bad_commands() {
        assert!(from_value::<SocketCommand>(json!("Nope")).is_err());
        assert!(from_value::<SocketCommand>(json!("KillProcess")).is_err());
        assert!(from_value::<SocketCommand>(json!({ "KillProcess": { "pid": 1 } })).is_err());
        assert!(
            from_value::<SocketCommand>(
                json!({ "KillProcess": { "pid": 1, "signal": "interrupt" } })
            )
            .is_err()
        );
        assert!(from_value::<SocketCommand>(json!({ "TrackPid": { "pid": -5 } })).is_err());
        assert!(
            from_value::<SocketCommand>(
                json!({ "ControlUnit": { "unit_name": "x", "action": "enable" } })
            )
            .is_err()
        );
        assert!(from_value::<SocketCommand>(json!({ "StartContainer": {} })).is_err());
    }

    // ---- responses ---------------------------------------------------------

    #[test]
    fn response_sample_list_covers_every_variant() {
        let all = command_responses();
        assert_eq!(all.len(), COMMAND_RESPONSE_COUNT, "update the sample list");
        assert_unique(&tags(&all));
    }

    #[test]
    fn every_command_response_roundtrips() {
        for r in command_responses() {
            assert_roundtrip(&r);
        }
    }

    #[test]
    fn every_command_has_a_response() {
        // The screen-capture responses are named `ScreenPoll*` while the commands
        // are `ScreenCapturePoll*`; everything else matches one to one.
        let expected: BTreeSet<String> = tags(&commands())
            .into_iter()
            .map(|t| t.replace("ScreenCapturePoll", "ScreenPoll"))
            .collect();
        assert_eq!(expected, tag_set(&command_responses()));
    }

    #[test]
    fn command_response_shapes() {
        assert_eq!(
            serde_json::to_value(SocketCommandResponse::TrackPid).unwrap(),
            json!("TrackPid")
        );
        assert_eq!(
            serde_json::to_value(SocketCommandResponse::KillProcess { os_result: true }).unwrap(),
            json!({ "KillProcess": { "os_result": true } })
        );
        assert_eq!(
            serde_json::to_value(SocketCommandResponse::KillTree {
                killed_pids: vec![10, 11],
            })
            .unwrap(),
            json!({ "KillTree": { "killed_pids": [10, 11] } })
        );
    }

    #[test]
    fn rejects_bad_command_responses() {
        assert!(from_value::<SocketCommandResponse>(json!("Nope")).is_err());
        assert!(from_value::<SocketCommandResponse>(json!("KillProcess")).is_err());
        assert!(from_value::<SocketCommandResponse>(json!({ "KillTree": {} })).is_err());
    }
}

// ---------------------------------------------------------------------------
// systemd_helper.rs
// ---------------------------------------------------------------------------

mod helper {
    use super::*;
    use kw_types::systemd_helper::{HelperRequest, HelperResponse};

    fn requests() -> Vec<HelperRequest> {
        vec![
            HelperRequest::Control {
                unit_name: "nginx.service".into(),
                action: systemd::ServiceAction::Restart,
            },
            HelperRequest::Ping,
            HelperRequest::Shutdown,
        ]
    }

    fn responses() -> Vec<HelperResponse> {
        vec![
            HelperResponse::Ok,
            HelperResponse::Pong,
            HelperResponse::Err {
                message: "boom".into(),
            },
        ]
    }

    #[test]
    fn requests_are_internally_tagged_with_type() {
        assert_eq!(
            serde_json::to_value(HelperRequest::Control {
                unit_name: "nginx.service".into(),
                action: systemd::ServiceAction::Restart,
            })
            .unwrap(),
            json!({ "type": "Control", "unit_name": "nginx.service", "action": "restart" })
        );
        assert_eq!(
            serde_json::to_value(HelperRequest::Ping).unwrap(),
            json!({ "type": "Ping" })
        );
        assert_eq!(
            serde_json::to_value(HelperRequest::Shutdown).unwrap(),
            json!({ "type": "Shutdown" })
        );
    }

    #[test]
    fn responses_are_internally_tagged_with_type() {
        assert_eq!(
            serde_json::to_value(HelperResponse::Ok).unwrap(),
            json!({ "type": "Ok" })
        );
        assert_eq!(
            serde_json::to_value(HelperResponse::Pong).unwrap(),
            json!({ "type": "Pong" })
        );
        assert_eq!(
            serde_json::to_value(HelperResponse::Err {
                message: "boom".into()
            })
            .unwrap(),
            json!({ "type": "Err", "message": "boom" })
        );
    }

    #[test]
    fn everything_roundtrips() {
        for r in requests() {
            assert_roundtrip(&r);
        }
        for r in responses() {
            assert_roundtrip(&r);
        }
    }

    #[test]
    fn every_service_action_roundtrips_inside_control() {
        for action in [
            systemd::ServiceAction::Start,
            systemd::ServiceAction::Stop,
            systemd::ServiceAction::Restart,
            systemd::ServiceAction::Reload,
        ] {
            assert_roundtrip(&HelperRequest::Control {
                unit_name: "x.service".into(),
                action,
            });
        }
    }

    #[test]
    fn one_json_object_per_line_even_with_newlines_in_the_payload() {
        // The protocol is newline-delimited JSON. A raw '\n' inside a value
        // would split one message into two, so serde must escape it.
        let nasty = HelperResponse::Err {
            message: "line1\nline2\r\nline3".into(),
        };
        let line = serde_json::to_string(&nasty).unwrap();
        assert!(!line.contains('\n'), "raw newline in: {line}");
        assert!(!line.contains('\r'), "raw CR in: {line}");

        let back: HelperResponse = serde_json::from_str(&line).unwrap();
        match back {
            HelperResponse::Err { message } => assert_eq!(message, "line1\nline2\r\nline3"),
            other => panic!("expected Err, got {other:?}"),
        }

        let nasty_unit = HelperRequest::Control {
            unit_name: "weird\nname.service".into(),
            action: systemd::ServiceAction::Stop,
        };
        assert!(!serde_json::to_string(&nasty_unit).unwrap().contains('\n'));
    }

    #[test]
    fn a_line_with_trailing_newline_parses_after_trimming() {
        let wire = format!("{}\n", serde_json::to_string(&HelperRequest::Ping).unwrap());
        let back: HelperRequest = serde_json::from_str(wire.trim_end()).unwrap();
        assert!(matches!(back, HelperRequest::Ping));
    }

    #[test]
    fn rejects_malformed_requests() {
        // missing or unknown tag
        assert!(from_value::<HelperRequest>(json!({})).is_err());
        assert!(from_value::<HelperRequest>(json!({ "type": "Nope" })).is_err());
        assert!(from_value::<HelperRequest>(json!({ "type": "ping" })).is_err());
        assert!(from_value::<HelperRequest>(json!("Ping")).is_err());
        // Control needs both fields and a valid action
        assert!(from_value::<HelperRequest>(json!({ "type": "Control" })).is_err());
        assert!(
            from_value::<HelperRequest>(json!({ "type": "Control", "unit_name": "x.service" }))
                .is_err()
        );
        assert!(
            from_value::<HelperRequest>(json!({ "type": "Control", "action": "stop" })).is_err()
        );
        assert!(
            from_value::<HelperRequest>(
                json!({ "type": "Control", "unit_name": "x.service", "action": "enable" })
            )
            .is_err()
        );
        assert!(
            from_value::<HelperRequest>(
                json!({ "type": "Control", "unit_name": "x.service", "action": "Stop" })
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_malformed_responses() {
        assert!(from_value::<HelperResponse>(json!({})).is_err());
        assert!(from_value::<HelperResponse>(json!({ "type": "Err" })).is_err());
        assert!(from_value::<HelperResponse>(json!({ "type": "Nope" })).is_err());
    }

    #[test]
    fn unknown_extra_fields_are_ignored() {
        // serde's default; worth knowing if you ever want the privileged helper
        // to be strict about what it accepts (`#[serde(deny_unknown_fields)]`).
        let r: HelperRequest = from_value(json!({ "type": "Ping", "extra": "ignored" })).unwrap();
        assert!(matches!(r, HelperRequest::Ping));
    }

    #[test]
    fn invalid_json_text_is_an_error_not_a_panic() {
        for bad in ["", "{", "not json", "null", "[]", "{\"type\":"] {
            assert!(
                serde_json::from_str::<HelperRequest>(bad).is_err(),
                "'{bad}'"
            );
            assert!(
                serde_json::from_str::<HelperResponse>(bad).is_err(),
                "'{bad}'"
            );
        }
    }
}
