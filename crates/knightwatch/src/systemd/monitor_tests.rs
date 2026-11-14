#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::oneshot,
};
use zbus::zvariant::OwnedObjectPath;

use kw_types::{event::EventPayload, polling::PollStatus, systemd::ServiceAction};

use super::{
    super::{
        source::since_from_usec,
        types::{RawUnit, ServiceDetails},
    },
    *,
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Fake {
    units: Vec<RawUnit>,
    fail_list: bool,
    details: HashMap<String, ServiceDetails>,
    detail_calls: Mutex<Vec<String>>,
}

impl UnitSource for Fake {
    async fn list_units(&self) -> Result<Vec<RawUnit>> {
        if self.fail_list {
            return Err(Error::Systemd("bus is down".into()));
        }
        Ok(self.units.clone())
    }

    fn service_details(&self, unit_name: &str, _object_path: &OwnedObjectPath) -> ServiceDetails {
        self.detail_calls
            .lock()
            .unwrap()
            .push(unit_name.to_string());
        self.details
            .get(unit_name)
            .cloned()
            .unwrap_or((None, None, None, None, None, None))
    }
}

fn raw(name: &str, active_state: &str) -> RawUnit {
    let path = OwnedObjectPath::try_from("/unit").unwrap();
    (
        name.to_string(),
        format!("about {name}"),
        "loaded".to_string(),
        active_state.to_string(),
        "running".to_string(),
        String::new(),
        path.clone(),
        0,
        String::new(),
        path,
    )
}

fn monitor() -> SystemdMonitor<Fake> {
    SystemdMonitor {
        state: SystemdMonitorState::new(),
        channels: SystemdMonitorChannels::new(),
        source: Fake::default(),
        poll: Poll::new(5),
        filter: UnitFilter::default(),
        first_tick: true,
        helper: Arc::new(OnceCell::new()),
    }
}

fn user() -> DisplayUser {
    DisplayUser {
        username: "alice".into(),
        telegram_chat_id: None,
    }
}

impl SystemdMonitor<Fake> {
    fn unit(&mut self, name: &str, active_state: &str) {
        self.source.units.retain(|u| u.0 != name);
        self.source.units.push(raw(name, active_state));
    }

    fn without(&mut self, name: &str) {
        self.source.units.retain(|u| u.0 != name);
    }

    fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> SystemdQuery) -> T {
        let (response, mut rx) = oneshot::channel();
        self.handle_query(make(response));
        rx.try_recv().unwrap()
    }

    fn snapshot(&self) -> Option<SystemdSnapshot> {
        self.ask(|response| SystemdQuery::Snapshot { response })
    }

    fn poll_status(&self) -> PollStatus {
        self.ask(|response| SystemdQuery::PollStatus { response })
            .unwrap()
    }

    fn run(
        &mut self,
        make: impl FnOnce(DisplayUser, oneshot::Sender<Result<()>>) -> SystemdCommand,
    ) -> Result<()> {
        let (response, mut rx) = oneshot::channel();
        self.handle_command(make(user(), response));
        rx.try_recv().unwrap()
    }
}

/// Events as sorted short strings, leaving out the `tick` every tick emits. They are
/// sorted because units are diffed through a `HashSet`.
fn drain(rx: &mut broadcast::Receiver<SystemdEvent>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(match event {
            SystemdEvent::InitialSnapshot { .. } => "initial".to_string(),
            SystemdEvent::Tick { .. } => continue,
            SystemdEvent::UnitFailed {
                unit_name,
                previous_state,
            } => format!("failed {unit_name} {previous_state:?}"),
            SystemdEvent::UnitRecovered { unit_name } => format!("recovered {unit_name}"),
            SystemdEvent::UnitAppeared { unit } => format!("appeared {}", unit.unit_name),
            SystemdEvent::UnitDisappeared { unit_name } => format!("disappeared {unit_name}"),
            SystemdEvent::CommandExecuted {
                action, success, ..
            } => format!("command {} {success}", action.name()),
        });
    }
    out.sort();
    out
}

/// A monitor with `a.service` active, past its first tick.
async fn running() -> (SystemdMonitor<Fake>, broadcast::Receiver<SystemdEvent>) {
    let mut m = monitor();
    m.unit("a.service", "active");
    m.handle_tick().await;
    let rx = m.channels.event_tx.subscribe();
    (m, rx)
}

// ─────────────────────────────────────────────────────────────────────────────
// diff to events
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_first_tick_is_the_snapshot_and_later_ones_are_ticks() {
    let mut m = monitor();
    let mut rx = m.channels.event_tx.subscribe();

    m.handle_tick().await;
    assert!(matches!(
        rx.try_recv().unwrap(),
        SystemdEvent::InitialSnapshot { .. }
    ));

    m.handle_tick().await;
    assert!(matches!(rx.try_recv().unwrap(), SystemdEvent::Tick { .. }));
}

#[tokio::test]
#[ignore = "finding 63: the first tick also emits UnitAppeared for every unit it finds"]
async fn the_first_tick_does_not_announce_every_unit_as_new() {
    let mut m = monitor();
    let mut rx = m.channels.event_tx.subscribe();
    m.unit("a.service", "active");
    m.unit("b.service", "active");

    m.handle_tick().await;

    assert_eq!(drain(&mut rx), ["initial"]);
}

#[tokio::test]
async fn a_unit_failed_at_startup_is_reported_as_coming_from_inactive() {
    // Pinned: the real previous state is unknown, and `Inactive` stands in for it.
    let mut m = monitor();
    let mut rx = m.channels.event_tx.subscribe();
    m.unit("a.service", "failed");

    m.handle_tick().await;

    assert!(drain(&mut rx).contains(&"failed a.service Inactive".to_string()));
}

#[tokio::test]
async fn an_unchanged_tick_emits_nothing_but_the_tick() {
    let (mut m, mut rx) = running().await;

    m.handle_tick().await;

    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn units_appearing_and_disappearing_are_reported_once() {
    let (mut m, mut rx) = running().await;
    m.without("a.service");
    m.unit("b.service", "active");

    m.handle_tick().await;
    m.handle_tick().await;

    assert_eq!(
        drain(&mut rx),
        ["appeared b.service", "disappeared a.service"]
    );
}

#[tokio::test]
async fn entering_failed_reports_the_real_previous_state_once() {
    let (mut m, mut rx) = running().await;
    m.unit("a.service", "failed");

    m.handle_tick().await;
    m.handle_tick().await;

    assert_eq!(drain(&mut rx), ["failed a.service Active"]);
}

#[tokio::test]
async fn leaving_failed_reports_recovery_once() {
    let (mut m, mut rx) = running().await;
    m.unit("a.service", "failed");
    m.handle_tick().await;
    drain(&mut rx);

    m.unit("a.service", "active");
    m.handle_tick().await;
    m.handle_tick().await;

    assert_eq!(drain(&mut rx), ["recovered a.service"]);
}

#[tokio::test]
async fn other_state_changes_emit_nothing() {
    let (mut m, mut rx) = running().await;

    m.unit("a.service", "inactive");
    m.handle_tick().await;

    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
#[ignore = "finding 64: a failed non-service unit leaves the filter on recovery, so it reads as gone"]
async fn a_failed_socket_that_recovers_is_reported_as_recovered() {
    let mut m = monitor();
    m.unit("a.socket", "failed");
    m.handle_tick().await;
    let mut rx = m.channels.event_tx.subscribe();

    m.unit("a.socket", "active");
    m.handle_tick().await;

    assert_eq!(drain(&mut rx), ["recovered a.socket"]);
}

#[tokio::test]
async fn a_failed_list_keeps_the_last_snapshot_and_emits_nothing() {
    let (mut m, mut rx) = running().await;
    m.source.fail_list = true;

    m.handle_tick().await;

    assert!(rx.try_recv().is_err());
    assert_eq!(m.snapshot().unwrap().units.len(), 1);
}

#[tokio::test]
async fn a_failed_first_list_leaves_the_snapshot_for_the_next_tick() {
    let mut m = monitor();
    let mut rx = m.channels.event_tx.subscribe();
    m.source.fail_list = true;
    m.handle_tick().await;
    assert!(m.snapshot().is_none());

    m.source.fail_list = false;
    m.handle_tick().await;

    assert_eq!(drain(&mut rx), ["initial"]);
}

// ─────────────────────────────────────────────────────────────────────────────
// snapshot construction
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn units_are_sorted_by_state_rank_then_name_and_counted() {
    let mut m = monitor();
    for (name, state) in [
        ("z.service", "failed"),
        ("c.service", "active"),
        ("a.service", "active"),
        ("r.service", "reloading"),
        ("g.service", "activating"),
        ("d.service", "deactivating"),
        ("i.service", "inactive"),
    ] {
        m.unit(name, state);
    }

    m.handle_tick().await;

    let snapshot = m.snapshot().unwrap();
    let names: Vec<&str> = snapshot
        .units
        .iter()
        .map(|u| u.unit_name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "z.service",
            "a.service",
            "c.service",
            "r.service",
            "g.service",
            "d.service",
            "i.service",
        ]
    );
    assert_eq!(snapshot.failed_count, 1);
    assert_eq!(snapshot.active_count, 2);
    assert_eq!(snapshot.inactive_count, 1);
}

#[tokio::test]
async fn details_are_only_fetched_for_active_or_reloading_services() {
    let mut m = monitor();
    m.filter.types = None;
    m.unit("up.service", "active");
    m.unit("reload.service", "reloading");
    m.unit("starting.service", "activating");
    m.unit("down.service", "inactive");
    m.unit("broken.service", "failed");
    m.unit("up.socket", "active");

    m.handle_tick().await;

    let mut calls = m.source.detail_calls.lock().unwrap().clone();
    calls.sort();
    assert_eq!(calls, ["reload.service", "up.service"]);
}

#[tokio::test]
async fn the_raw_unit_and_its_details_land_in_the_snapshot() {
    let mut m = monitor();
    m.unit("a.service", "active");
    m.source.details.insert(
        "a.service".into(),
        (
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some("since".into()),
            Some("/path".into()),
        ),
    );

    m.handle_tick().await;

    let snapshot = m.snapshot().unwrap();
    let unit = &snapshot.units[0];
    assert_eq!(unit.unit_type, UnitType::Service);
    assert_eq!(unit.load_state, UnitLoadState::Loaded);
    assert_eq!(unit.active_state, UnitActiveState::Active);
    assert_eq!(unit.sub_state, "running");
    assert_eq!(unit.description, "about a.service");
    assert_eq!(unit.main_pid, Some(1));
    assert_eq!(unit.memory_bytes, Some(2));
    assert_eq!(unit.cpu_usage_ns, Some(3));
    assert_eq!(unit.restart_count, Some(4));
    assert_eq!(unit.since.as_deref(), Some("since"));
    assert_eq!(unit.fragment_path.as_deref(), Some("/path"));
}

#[tokio::test]
async fn systemds_maintenance_state_reads_as_inactive() {
    // Pinned (finding 66): `UnitActiveState` has no `maintenance` or `refreshing`.
    let mut m = monitor();
    m.unit("a.service", "maintenance");

    m.handle_tick().await;

    let snapshot = m.snapshot().unwrap();
    assert_eq!(snapshot.units[0].active_state, UnitActiveState::Inactive);
    assert_eq!(snapshot.inactive_count, 1);
}

#[test]
fn the_active_enter_timestamp_converts_from_microseconds() {
    assert_eq!(since_from_usec(0), None, "never entered active");
    assert_eq!(
        since_from_usec(1_500_000).as_deref(),
        Some("1970-01-01T00:00:01.500+00:00")
    );
    assert_eq!(since_from_usec(u64::MAX), None, "out of range");
}

// ─────────────────────────────────────────────────────────────────────────────
// unit filter
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_default_filter_takes_services_and_anything_failed() {
    let filter = UnitFilter::default();

    assert!(filter.matches(&UnitType::Service, "inactive", "a.service"));
    assert!(!filter.matches(&UnitType::Socket, "active", "a.socket"));
    assert!(filter.matches(&UnitType::Socket, "failed", "a.socket"));
}

#[test]
fn include_failed_off_applies_the_filters_to_failed_units_too() {
    let filter = UnitFilter {
        include_failed: false,
        ..UnitFilter::default()
    };

    assert!(!filter.matches(&UnitType::Socket, "failed", "a.socket"));
    assert!(filter.matches(&UnitType::Service, "failed", "a.service"));
}

#[test]
fn the_state_and_name_filters_each_narrow_the_match() {
    let by_state = UnitFilter {
        active_states: Some(vec![UnitActiveState::Active]),
        ..UnitFilter::default()
    };
    assert!(by_state.matches(&UnitType::Service, "active", "a.service"));
    assert!(!by_state.matches(&UnitType::Service, "inactive", "a.service"));

    let by_name = UnitFilter {
        name_prefix: Some("nginx".into()),
        ..UnitFilter::default()
    };
    assert!(by_name.matches(&UnitType::Service, "active", "nginx.service"));
    assert!(!by_name.matches(&UnitType::Service, "active", "apache.service"));
}

#[test]
fn no_type_filter_takes_every_type() {
    let filter = UnitFilter {
        types: None,
        ..UnitFilter::default()
    };

    assert!(filter.matches(&UnitType::Timer, "active", "a.timer"));
}

#[tokio::test]
async fn with_filter_replaces_the_filter_used_by_ticks() {
    let mut m = monitor().with_filter(UnitFilter {
        types: None,
        ..UnitFilter::default()
    });
    m.unit("a.timer", "active");

    m.handle_tick().await;

    assert_eq!(m.snapshot().unwrap().units[0].unit_name, "a.timer");
}

// ─────────────────────────────────────────────────────────────────────────────
// queries
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn queries_before_the_first_tick_are_empty() {
    let m = monitor();

    assert!(m.snapshot().is_none());
    assert!(
        m.ask(|response| SystemdQuery::Unit {
            unit_name: "a.service".into(),
            response,
        })
        .is_none()
    );
    assert!(
        m.ask(|response| SystemdQuery::ByActiveState {
            state: UnitActiveState::Active,
            response,
        })
        .is_empty()
    );
}

#[tokio::test]
async fn units_are_found_by_exact_name_or_by_state() {
    let (mut m, _rx) = running().await;
    m.unit("b.service", "inactive");
    m.handle_tick().await;

    let found = m.ask(|response| SystemdQuery::Unit {
        unit_name: "b.service".into(),
        response,
    });
    assert_eq!(found.unwrap().active_state, UnitActiveState::Inactive);
    assert!(
        m.ask(|response| SystemdQuery::Unit {
            unit_name: "b".into(),
            response,
        })
        .is_none()
    );

    let active = m.ask(|response| SystemdQuery::ByActiveState {
        state: UnitActiveState::Active,
        response,
    });
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].unit_name, "a.service");
}

// ─────────────────────────────────────────────────────────────────────────────
// control commands
// ─────────────────────────────────────────────────────────────────────────────

/// Sends a `Control` command and waits, because it runs on a spawned task.
async fn control(m: &mut SystemdMonitor<Fake>) -> Result<()> {
    let (response, answer) = oneshot::channel();
    m.handle_command(SystemdCommand::Control {
        user: user(),
        unit_name: "a.service".into(),
        action: ServiceAction::Restart,
        response,
    });
    tokio::time::timeout(Duration::from_secs(5), answer)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn control_without_a_helper_fails_and_is_reported() {
    let (mut m, mut rx) = running().await;

    let err = control(&mut m).await.unwrap_err();

    assert!(err.to_string().contains("helper is not ready"), "{err}");
    assert_eq!(drain(&mut rx), ["command control false"]);
}

#[tokio::test]
async fn control_goes_through_the_helper_and_is_reported() {
    let (mut m, mut rx) = running().await;
    let (ours, theirs) = UnixStream::pair().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let child = tokio::process::Command::new("true").spawn().unwrap();
    let client = SystemdHelperClient::from_parts(ours, dir.path().join("helper.sock"), child);
    assert!(m.helper_handle().set(client).is_ok());
    let helper = tokio::spawn(async move {
        let mut theirs = BufReader::new(theirs);
        let mut request = String::new();
        theirs.read_line(&mut request).await.unwrap();
        theirs
            .get_mut()
            .write_all(b"{\"type\":\"Ok\"}\n")
            .await
            .unwrap();
        request
    });

    control(&mut m).await.unwrap();

    assert!(helper.await.unwrap().contains("a.service"));
    assert_eq!(drain(&mut rx), ["command control true"]);
}

// ─────────────────────────────────────────────────────────────────────────────
// polling
// ─────────────────────────────────────────────────────────────────────────────

fn set_interval(m: &mut SystemdMonitor<Fake>, interval: Duration) -> Result<()> {
    m.run(|user, response| SystemdCommand::SetPollInterval {
        user,
        interval,
        response,
    })
}

#[tokio::test]
async fn poll_commands_change_the_poll_and_report_it() {
    let mut m = monitor();
    let mut rx = m.channels.event_tx.subscribe();
    assert!(m.poll_status().paused, "paused until the loop starts");

    m.run(|user, response| SystemdCommand::ResumePoll { user, response })
        .unwrap();
    assert!(!m.poll_status().paused);

    set_interval(&mut m, Duration::from_millis(250)).unwrap();
    let status = m.poll_status();
    assert_eq!(status.interval, 250);
    assert!(!status.paused);

    m.run(|user, response| SystemdCommand::PausePoll { user, response })
        .unwrap();
    assert!(m.poll_status().paused);

    assert_eq!(
        drain(&mut rx),
        [
            "command pause_poll true",
            "command resume_poll true",
            "command set_poll_interval true",
        ]
    );
}

#[tokio::test]
async fn setting_the_interval_while_paused_stays_paused() {
    let mut m = monitor();

    set_interval(&mut m, Duration::from_millis(250)).unwrap();

    let status = m.poll_status();
    assert_eq!(status.interval, 250);
    assert!(status.paused);
}

#[tokio::test]
async fn a_zero_interval_is_rejected_while_running() {
    let mut m = monitor();
    m.poll.resume();

    assert!(set_interval(&mut m, Duration::ZERO).is_err());
    assert_eq!(m.poll_status().interval, 5000);
}

#[tokio::test]
async fn a_zero_interval_is_rejected_while_paused() {
    let mut m = monitor();

    assert!(set_interval(&mut m, Duration::ZERO).is_err());
    assert_eq!(m.poll_status().interval, 5000);
}

// ─────────────────────────────────────────────────────────────────────────────
// the running loop
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_loop_ticks_and_answers_queries_and_commands() {
    let mut m = monitor();
    m.unit("a.service", "active");
    let query_tx = m.channels.query_tx.clone();
    let command_tx = m.channels.command_tx.clone();
    let mut rx = m.channels.event_tx.subscribe();
    let task = tokio::spawn(m.start_monitor_loop());
    let wait = Duration::from_secs(5);

    loop {
        let event = tokio::time::timeout(wait, rx.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(event, SystemdEvent::InitialSnapshot { .. }) {
            break;
        }
    }

    let (response, answer) = oneshot::channel();
    query_tx
        .send(SystemdQuery::Snapshot { response })
        .await
        .unwrap();
    let snapshot = tokio::time::timeout(wait, answer).await.unwrap().unwrap();
    assert_eq!(snapshot.unwrap().units.len(), 1);

    let (response, answer) = oneshot::channel();
    command_tx
        .send(SystemdCommand::PausePoll {
            user: user(),
            response,
        })
        .await
        .unwrap();
    tokio::time::timeout(wait, answer)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    task.abort();
}

// ─────────────────────────────────────────────────────────────────────────────
// event payloads
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn events_convert_to_their_public_names_and_data() {
    let (m, _rx) = running().await;
    let unit = m.snapshot().unwrap().units[0].clone();
    let cases = [
        (
            SystemdEvent::UnitFailed {
                unit_name: "a.service".into(),
                previous_state: UnitActiveState::Active,
            },
            "systemd.unit_failed",
            serde_json::json!({ "unit_name": "a.service", "previous_state": "active" }),
        ),
        (
            SystemdEvent::UnitRecovered {
                unit_name: "a.service".into(),
            },
            "systemd.unit_recovered",
            serde_json::json!({ "unit_name": "a.service" }),
        ),
        (
            SystemdEvent::UnitAppeared { unit },
            "systemd.unit_appeared",
            serde_json::json!({
                "unit_name": "a.service",
                "active_state": "active",
                "sub_state": "running",
                "description": "about a.service",
            }),
        ),
        (
            SystemdEvent::UnitDisappeared {
                unit_name: "a.service".into(),
            },
            "systemd.unit_disappeared",
            serde_json::json!({ "unit_name": "a.service" }),
        ),
    ];

    for (event, name, data) in cases {
        let payload = EventPayload::from(&event);
        assert_eq!(payload.event, name);
        assert_eq!(payload.data, data, "{name}");
    }
}

#[tokio::test]
async fn snapshot_events_carry_only_the_counts() {
    let (m, _rx) = running().await;
    let snapshot = m.snapshot().unwrap();

    let initial = EventPayload::from(&SystemdEvent::InitialSnapshot {
        snapshot: snapshot.clone(),
    });
    let tick = EventPayload::from(&SystemdEvent::Tick { snapshot });

    assert_eq!(initial.event, "systemd.initial_snapshot");
    assert_eq!(tick.event, "systemd.tick");
    assert_eq!(tick.data["unit_count"], 1);
    assert_eq!(tick.data["active_count"], 1);
    assert!(tick.data.get("units").is_none());
}

#[test]
fn command_events_carry_the_action_and_the_error() {
    let event = SystemdEvent::CommandExecuted {
        user: user(),
        action: SystemdCommandAction::PausePoll,
        success: false,
        error: Some("nope".into()),
    };

    let payload = EventPayload::from(&event);

    assert_eq!(payload.event, "systemd.command_executed");
    assert_eq!(payload.data["action"], "pause_poll");
    assert_eq!(payload.data["success"], false);
    assert_eq!(payload.data["error"], "nope");
}
