#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use futures::future::BoxFuture;
use std::time::Duration;
use tokio::sync::oneshot;

use bollard::models::{EventActor, EventMessage};
use kw_types::{
    docker::{ContainerHealth, ContainerStats},
    event::EventPayload,
    polling::PollStatus,
};

use super::{super::source::forward_oom_events, *};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Fake {
    containers: Vec<ContainerSnapshot>,
    /// Running containers missing from here fail their stats read.
    stats: HashMap<String, ContainerStats>,
    fail_list: bool,
    fail_commands: bool,
    hang_commands: bool,
    oom: Vec<(String, String)>,
    calls: Mutex<Vec<String>>,
}

impl Fake {
    fn call(&self, call: String) -> Result<()> {
        self.calls.lock().unwrap().push(call);
        if self.fail_commands {
            return Err(Error::DockerTracker("daemon said no".into()));
        }
        Ok(())
    }

    async fn command(&self, call: String) -> Result<()> {
        if self.hang_commands {
            std::future::pending::<()>().await;
        }
        self.call(call)
    }
}

impl DockerSource for Fake {
    async fn list(&self) -> Result<Vec<ContainerSnapshot>> {
        if self.fail_list {
            return Err(Error::DockerTracker("daemon is down".into()));
        }
        Ok(self.containers.clone())
    }

    async fn stats(&self, id: &str) -> Option<ContainerStats> {
        self.calls.lock().unwrap().push(format!("stats {id}"));
        self.stats.get(id).cloned()
    }

    async fn stop(&self, id: &str, timeout_secs: Option<i32>) -> Result<()> {
        self.command(format!("stop {id} {timeout_secs:?}")).await
    }

    async fn kill(&self, id: &str, signal: Option<&str>) -> Result<()> {
        self.command(format!("kill {id} {signal:?}")).await
    }

    async fn start(&self, id: &str) -> Result<()> {
        self.command(format!("start {id}")).await
    }

    async fn restart(&self, id: &str, timeout_secs: Option<i32>) -> Result<()> {
        self.command(format!("restart {id} {timeout_secs:?}")).await
    }

    async fn pause(&self, id: &str) -> Result<()> {
        self.command(format!("pause {id}")).await
    }

    async fn unpause(&self, id: &str) -> Result<()> {
        self.command(format!("unpause {id}")).await
    }

    fn oom_listener(&self, oom_tx: mpsc::Sender<(String, String)>) -> BoxFuture<'static, ()> {
        let oom = self.oom.clone();
        Box::pin(async move {
            for event in oom {
                oom_tx.send(event).await.unwrap();
            }
        })
    }
}

fn tracker() -> DockerTracker<Fake> {
    DockerTracker {
        source: Fake::default(),
        state: DockerTrackerState::new(),
        channels: DockerTrackerChannels::new(),
        poll: Poll::new(5),
        limit_containers: 20,
    }
}

/// A running container with no health check and the id `<name>-id`.
fn container(name: &str) -> ContainerSnapshot {
    ContainerSnapshot {
        id: format!("{name}-id"),
        short_id: format!("{name}-id"),
        name: name.into(),
        image: "image".into(),
        status: ContainerStatus::Running,
        health: ContainerHealth::None,
        stats: None,
    }
}

fn stats(cpu_percent: f64, memory_bytes: u64) -> ContainerStats {
    ContainerStats {
        cpu_percent,
        memory_bytes,
        memory_limit_bytes: 0,
        memory_percent: None,
        net_rx_bytes: 0,
        net_tx_bytes: 0,
        block_read_bytes: 0,
        block_write_bytes: 0,
        pid_count: 0,
    }
}

fn user() -> DisplayUser {
    DisplayUser {
        username: "alice".into(),
        telegram_chat_id: None,
    }
}

impl DockerTracker<Fake> {
    fn with(&mut self, name: &str) {
        self.source.containers.push(container(name));
    }

    fn without(&mut self, name: &str) {
        self.source.containers.retain(|c| c.name != name);
    }

    fn status(&mut self, name: &str, status: ContainerStatus) {
        let found = self.source.containers.iter_mut().find(|c| c.name == name);
        found.unwrap().status = status;
    }

    fn set_stats(&mut self, name: &str, cpu: f64, memory: u64) {
        self.source
            .stats
            .insert(format!("{name}-id"), stats(cpu, memory));
    }

    fn calls(&self) -> Vec<String> {
        self.source.calls.lock().unwrap().clone()
    }

    fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> DockerTrackerQuery) -> T {
        let (response, mut rx) = oneshot::channel();
        self.handle_query(make(response));
        rx.try_recv().unwrap()
    }

    fn get(&self, id_or_name: &str) -> Option<ContainerSnapshot> {
        self.ask(|response| DockerTrackerQuery::GetContainer {
            id_or_name: id_or_name.into(),
            response,
        })
    }

    fn top(&self, by: DockerSortKey, limit: usize) -> Vec<String> {
        let top = self.ask(|response| DockerTrackerQuery::GetTopContainers {
            by,
            limit,
            response,
        });
        top.into_iter().map(|c| c.name).collect()
    }

    fn poll_status(&self) -> PollStatus {
        self.ask(|response| DockerTrackerQuery::PollStatus { response })
            .unwrap()
    }

    async fn run(
        &mut self,
        make: impl FnOnce(DisplayUser, oneshot::Sender<Result<()>>) -> DockerTrackerCommand,
    ) -> Result<()> {
        let (response, mut rx) = oneshot::channel();
        self.handle_command(make(user(), response)).await;
        rx.try_recv().unwrap()
    }

    async fn kill(&mut self, id_or_name: &str) -> Result<()> {
        self.run(|user, response| DockerTrackerCommand::KillContainer {
            user,
            id_or_name: id_or_name.into(),
            signal: None,
            response,
        })
        .await
    }
}

fn names(containers: Vec<ContainerSnapshot>) -> Vec<String> {
    let mut out: Vec<String> = containers.into_iter().map(|c| c.name).collect();
    out.sort();
    out
}

/// Events as short strings, because `DockerTrackerEvent` has no `PartialEq`.
fn drain(rx: &mut broadcast::Receiver<DockerTrackerEvent>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(match event {
            DockerTrackerEvent::InitialSnapshot { containers } => {
                let in_order: Vec<String> = containers.into_iter().map(|c| c.name).collect();
                format!("initial {in_order:?}")
            }
            DockerTrackerEvent::ContainersAppeared { containers } => {
                format!("appeared {:?}", names(containers))
            }
            DockerTrackerEvent::ContainersDisappeared { containers } => {
                format!("disappeared {:?}", names(containers))
            }
            DockerTrackerEvent::ContainerStatusChanged {
                container,
                previous,
            } => format!(
                "status {} {previous:?} -> {:?}",
                container.name, container.status
            ),
            DockerTrackerEvent::ContainerHealthChanged {
                container,
                previous,
            } => format!(
                "health {} {previous:?} -> {:?}",
                container.name, container.health
            ),
            DockerTrackerEvent::ContainerOomKilled { id, name } => format!("oom {id} {name}"),
            DockerTrackerEvent::CommandExecuted {
                action,
                success,
                error,
                ..
            } => format!("command {} {success} {}", action.name(), error.is_some()),
        });
    }
    out
}

/// A tracker with containers `a` and `b`, past its first tick.
async fn running() -> (DockerTracker<Fake>, broadcast::Receiver<DockerTrackerEvent>) {
    let mut t = tracker();
    t.with("a");
    t.with("b");
    t.handle_tick().await.unwrap();
    let rx = t.channels.event_tx.subscribe();
    (t, rx)
}

// ─────────────────────────────────────────────────────────────────────────────
// diff to events
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_first_tick_is_one_snapshot_sorted_by_name() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();
    t.with("b");
    t.with("a");

    t.handle_tick().await.unwrap();

    assert_eq!(drain(&mut rx), [r#"initial ["a", "b"]"#]);
}

#[tokio::test]
async fn the_first_tick_with_no_containers_is_still_a_snapshot() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();

    t.handle_tick().await.unwrap();

    assert_eq!(drain(&mut rx), ["initial []"]);
}

#[tokio::test]
async fn an_unchanged_tick_emits_nothing() {
    let (mut t, mut rx) = running().await;

    t.handle_tick().await.unwrap();

    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn new_containers_are_reported_once() {
    let (mut t, mut rx) = running().await;
    t.with("c");
    t.with("d");

    t.handle_tick().await.unwrap();
    t.handle_tick().await.unwrap();

    assert_eq!(drain(&mut rx), [r#"appeared ["c", "d"]"#]);
}

#[tokio::test]
async fn removed_containers_carry_their_last_snapshot() {
    let (mut t, _) = running().await;
    t.set_stats("a", 12.0, 34);
    t.handle_tick().await.unwrap();
    let mut rx = t.channels.event_tx.subscribe();
    t.without("a");

    t.handle_tick().await.unwrap();

    let event = rx.try_recv().unwrap();
    let DockerTrackerEvent::ContainersDisappeared { containers } = event else {
        panic!("wrong event: {event:?}");
    };
    assert_eq!(containers[0].name, "a");
    assert_eq!(containers[0].stats.as_ref().unwrap().memory_bytes, 34);
}

#[tokio::test]
async fn appearing_is_reported_before_disappearing() {
    let (mut t, mut rx) = running().await;
    t.without("a");
    t.with("c");

    t.handle_tick().await.unwrap();

    assert_eq!(
        drain(&mut rx),
        [r#"appeared ["c"]"#, r#"disappeared ["a"]"#]
    );
}

#[tokio::test]
async fn a_status_change_reports_the_old_and_new_status() {
    let (mut t, mut rx) = running().await;
    t.status("a", ContainerStatus::Exited);

    t.handle_tick().await.unwrap();
    t.handle_tick().await.unwrap();

    assert_eq!(drain(&mut rx), ["status a Running -> Exited"]);
}

#[tokio::test]
async fn a_health_change_reports_the_old_and_new_health() {
    let (mut t, mut rx) = running().await;
    t.source.containers[0].health = ContainerHealth::Unhealthy;

    t.handle_tick().await.unwrap();
    t.handle_tick().await.unwrap();

    assert_eq!(drain(&mut rx), ["health a None -> Unhealthy"]);
}

#[tokio::test]
async fn changing_stats_emit_no_events() {
    let (mut t, mut rx) = running().await;
    t.set_stats("a", 99.0, 1);

    t.handle_tick().await.unwrap();

    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn a_failed_list_keeps_the_last_state_and_emits_nothing() {
    let (mut t, mut rx) = running().await;
    t.source.fail_list = true;

    assert!(t.handle_tick().await.is_err());

    assert!(drain(&mut rx).is_empty());
    assert!(t.get("a").is_some());

    // The diff on recovery is against the state before the failure.
    t.source.fail_list = false;
    t.without("a");
    t.handle_tick().await.unwrap();
    assert_eq!(drain(&mut rx), [r#"disappeared ["a"]"#]);
}

#[tokio::test]
async fn a_failed_first_list_leaves_the_snapshot_for_the_next_tick() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();
    t.with("a");
    t.source.fail_list = true;
    assert!(t.handle_tick().await.is_err());

    t.source.fail_list = false;
    t.handle_tick().await.unwrap();

    assert_eq!(drain(&mut rx), [r#"initial ["a"]"#]);
}

// ─────────────────────────────────────────────────────────────────────────────
// stats and top containers
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stats_are_only_read_for_running_containers() {
    let mut t = tracker();
    t.with("up");
    t.with("down");
    t.status("down", ContainerStatus::Exited);

    t.handle_tick().await.unwrap();

    assert_eq!(t.calls(), ["stats up-id"]);
}

#[tokio::test]
async fn a_failed_stats_read_leaves_the_container_out_of_the_top_list() {
    let mut t = tracker();
    t.with("ok");
    t.with("broken");
    t.set_stats("ok", 1.0, 1);

    t.handle_tick().await.unwrap();

    assert!(t.get("broken").unwrap().stats.is_none());
    assert_eq!(t.top(DockerSortKey::Cpu, 0), ["ok"]);
}

/// `a` is highest on cpu, `b` on memory, and `c` is lowest on both.
async fn top_tracker(limit_containers: usize) -> DockerTracker<Fake> {
    let mut t = tracker();
    t.limit_containers = limit_containers;
    for (name, cpu, memory) in [("a", 90.0, 20), ("b", 50.0, 30), ("c", 10.0, 10)] {
        t.with(name);
        t.set_stats(name, cpu, memory);
    }
    t.handle_tick().await.unwrap();
    t
}

#[tokio::test]
async fn top_containers_are_sorted_per_key() {
    let t = top_tracker(10).await;

    assert_eq!(t.top(DockerSortKey::Cpu, 0), ["a", "b", "c"]);
    assert_eq!(t.top(DockerSortKey::Memory, 0), ["b", "a", "c"]);
}

#[tokio::test]
async fn top_query_limit_is_capped_by_the_configured_limit() {
    let t = top_tracker(2).await;

    assert_eq!(t.top(DockerSortKey::Cpu, 0), ["a", "b"], "0 means the cap");
    assert_eq!(t.top(DockerSortKey::Cpu, 1), ["a"]);
    assert_eq!(t.top(DockerSortKey::Cpu, 99), ["a", "b"]);
}

#[tokio::test]
async fn a_stopped_container_leaves_the_top_list() {
    let mut t = top_tracker(10).await;

    t.status("a", ContainerStatus::Exited);
    t.handle_tick().await.unwrap();

    assert_eq!(t.top(DockerSortKey::Cpu, 0), ["b", "c"]);
}

// ─────────────────────────────────────────────────────────────────────────────
// queries
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn listing_is_sorted_by_name() {
    let mut t = tracker();
    t.with("b");
    t.with("a");
    t.with("c");
    t.handle_tick().await.unwrap();

    let listed = t.ask(|response| DockerTrackerQuery::ListContainers { response });

    let in_order: Vec<String> = listed.into_iter().map(|c| c.name).collect();
    assert_eq!(in_order, ["a", "b", "c"]);
}

#[tokio::test]
async fn a_container_is_found_by_name_full_id_or_id_prefix() {
    let (t, _rx) = running().await;

    assert_eq!(t.get("a").unwrap().name, "a");
    assert_eq!(t.get("a-id").unwrap().name, "a");
    assert_eq!(t.get("a-").unwrap().name, "a");
    assert!(t.get("nope").is_none());
    assert!(t.get("id").is_none(), "only a prefix of the id matches");
}

#[tokio::test]
async fn queries_before_the_first_tick_are_empty() {
    let t = tracker();

    assert!(
        t.ask(|response| DockerTrackerQuery::ListContainers { response })
            .is_empty()
    );
    assert!(t.get("a").is_none());
    assert!(t.top(DockerSortKey::Cpu, 0).is_empty());
}

#[tokio::test]
#[ignore = "finding 60: an empty id matches every container, as every id starts with it"]
async fn an_empty_id_matches_nothing() {
    let (t, _rx) = running().await;

    assert!(t.get("").is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// container commands
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn each_command_reaches_docker_with_the_resolved_id_and_its_options() {
    let (mut t, mut rx) = running().await;

    t.run(|user, response| DockerTrackerCommand::StopContainer {
        user,
        id_or_name: "a".into(),
        timeout_secs: Some(5),
        response,
    })
    .await
    .unwrap();
    t.run(|user, response| DockerTrackerCommand::KillContainer {
        user,
        id_or_name: "a".into(),
        signal: Some("SIGTERM".into()),
        response,
    })
    .await
    .unwrap();
    t.run(|user, response| DockerTrackerCommand::StartContainer {
        user,
        id_or_name: "a".into(),
        response,
    })
    .await
    .unwrap();
    t.run(|user, response| DockerTrackerCommand::RestartContainer {
        user,
        id_or_name: "a".into(),
        timeout_secs: None,
        response,
    })
    .await
    .unwrap();
    t.run(|user, response| DockerTrackerCommand::PauseContainer {
        user,
        id_or_name: "a".into(),
        response,
    })
    .await
    .unwrap();
    t.run(|user, response| DockerTrackerCommand::UnpauseContainer {
        user,
        id_or_name: "a".into(),
        response,
    })
    .await
    .unwrap();

    assert_eq!(
        t.calls()[2..],
        [
            "stop a-id Some(5)",
            r#"kill a-id Some("SIGTERM")"#,
            "start a-id",
            "restart a-id None",
            "pause a-id",
            "unpause a-id",
        ]
    );
    assert_eq!(
        drain(&mut rx),
        [
            "command stop true false",
            "command kill true false",
            "command start true false",
            "command restart true false",
            "command pause true false",
            "command unpause true false",
        ]
    );
}

#[tokio::test]
async fn a_command_event_names_the_target() {
    let (mut t, mut rx) = running().await;

    t.kill("a").await.unwrap();

    let DockerTrackerEvent::CommandExecuted { action, .. } = rx.try_recv().unwrap() else {
        panic!("wrong event");
    };
    assert_eq!(action.id_or_name(), Some("a"));
    assert_eq!(action.container_name(), Some("a"));
}

#[tokio::test]
async fn an_untracked_target_goes_to_docker_as_given_with_no_name() {
    let (mut t, mut rx) = running().await;

    t.kill("elsewhere").await.unwrap();

    assert_eq!(t.calls().last().unwrap(), "kill elsewhere None");
    let DockerTrackerEvent::CommandExecuted { action, .. } = rx.try_recv().unwrap() else {
        panic!("wrong event");
    };
    assert_eq!(action.container_name(), None);
}

#[tokio::test]
async fn a_docker_error_is_returned_and_reported() {
    let (mut t, mut rx) = running().await;
    t.source.fail_commands = true;

    let err = t.kill("a").await.unwrap_err();

    assert!(err.to_string().contains("daemon said no"), "{err}");
    assert_eq!(drain(&mut rx), ["command kill false true"]);
}

#[tokio::test]
#[ignore = "finding 60: an ambiguous id prefix acts on whichever container comes first"]
async fn an_ambiguous_prefix_is_rejected() {
    let mut t = tracker();
    t.with("web-1");
    t.with("web-2");
    t.source.containers[0].id = "abc1".into();
    t.source.containers[1].id = "abc2".into();
    t.handle_tick().await.unwrap();
    let before = t.calls().len();

    assert!(t.kill("abc").await.is_err());

    assert_eq!(t.calls().len(), before, "nothing was killed");
}

#[tokio::test]
#[ignore = "finding 60: an empty id kills a tracked container"]
async fn an_empty_id_kills_nothing() {
    let (mut t, _rx) = running().await;
    let before = t.calls().len();

    let _ = t.kill("").await;

    let calls = t.calls();
    assert!(
        !calls[before..].iter().any(|c| c.contains("-id")),
        "{calls:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// polling
// ─────────────────────────────────────────────────────────────────────────────

async fn set_interval(t: &mut DockerTracker<Fake>, interval: Duration) -> Result<()> {
    t.run(|user, response| DockerTrackerCommand::SetPollInterval {
        user,
        interval,
        response,
    })
    .await
}

#[tokio::test]
async fn poll_commands_change_the_poll_and_report_it() {
    let mut t = tracker();
    let mut rx = t.channels.event_tx.subscribe();
    assert!(t.poll_status().paused, "paused until the loop starts");

    t.run(|user, response| DockerTrackerCommand::ResumePoll { user, response })
        .await
        .unwrap();
    assert!(!t.poll_status().paused);

    set_interval(&mut t, Duration::from_millis(250))
        .await
        .unwrap();
    let status = t.poll_status();
    assert_eq!(status.interval, 250);
    assert!(!status.paused);

    t.run(|user, response| DockerTrackerCommand::PausePoll { user, response })
        .await
        .unwrap();
    assert!(t.poll_status().paused);

    assert_eq!(
        drain(&mut rx),
        [
            "command resume_poll true false",
            "command set_poll_interval true false",
            "command pause_poll true false",
        ]
    );
}

#[tokio::test]
async fn setting_the_interval_while_paused_stays_paused() {
    let mut t = tracker();

    set_interval(&mut t, Duration::from_millis(250))
        .await
        .unwrap();

    let status = t.poll_status();
    assert_eq!(status.interval, 250);
    assert!(status.paused);
}

#[tokio::test]
async fn a_zero_interval_is_rejected_while_running() {
    let mut t = tracker();
    t.poll.resume();

    assert!(set_interval(&mut t, Duration::ZERO).await.is_err());
    assert_eq!(t.poll_status().interval, 5000);
}

#[tokio::test]
async fn a_zero_interval_is_rejected_while_paused() {
    let mut t = tracker();

    assert!(set_interval(&mut t, Duration::ZERO).await.is_err());
    assert_eq!(t.poll_status().interval, 5000);
}

// ─────────────────────────────────────────────────────────────────────────────
// the running loop
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_loop_ticks_answers_queries_and_forwards_oom_events() {
    let mut t = tracker();
    t.with("a");
    t.source.oom = vec![("a-id".into(), "a".into())];
    let query_tx = t.channels.query_tx.clone();
    let mut rx = t.channels.event_tx.subscribe();
    let task = tokio::spawn(t.start_tracking_loop());
    let wait = Duration::from_secs(5);

    let mut events = Vec::new();
    while events.len() < 2 {
        let event = tokio::time::timeout(wait, rx.recv())
            .await
            .unwrap()
            .unwrap();
        events.push(format!("{event:?}"));
    }
    assert!(events.iter().any(|e| e.starts_with("InitialSnapshot")));
    assert!(events.iter().any(|e| e.starts_with("ContainerOomKilled")));

    let (response, answer) = oneshot::channel();
    query_tx
        .send(DockerTrackerQuery::ListContainers { response })
        .await
        .unwrap();
    let listed = tokio::time::timeout(wait, answer).await.unwrap().unwrap();
    assert_eq!(listed.len(), 1);

    task.abort();
}

#[tokio::test(start_paused = true)]
#[ignore = "finding 59: commands run inside the loop, so a slow one blocks queries and ticks"]
async fn queries_are_answered_while_a_command_is_running() {
    let mut t = tracker();
    t.source.hang_commands = true;
    let query_tx = t.channels.query_tx.clone();
    let command_tx = t.channels.command_tx.clone();
    let task = tokio::spawn(t.start_tracking_loop());

    let (response, _pending) = oneshot::channel();
    command_tx
        .send(DockerTrackerCommand::StartContainer {
            user: user(),
            id_or_name: "a".into(),
            response,
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let (response, answer) = oneshot::channel();
    query_tx
        .send(DockerTrackerQuery::PollStatus { response })
        .await
        .unwrap();
    let answered = tokio::time::timeout(Duration::from_mins(1), answer).await;

    task.abort();
    assert!(answered.is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// the OOM event stream
// ─────────────────────────────────────────────────────────────────────────────

fn oom_event(id: Option<&str>, name: Option<&str>) -> EventMessage {
    EventMessage {
        actor: Some(EventActor {
            id: id.map(Into::into),
            attributes: name.map(|n| HashMap::from([("name".to_string(), n.to_string())])),
        }),
        ..EventMessage::default()
    }
}

type StreamItem = std::result::Result<EventMessage, bollard::errors::Error>;

async fn forwarded(items: Vec<StreamItem>) -> Vec<(String, String)> {
    let (tx, mut rx) = mpsc::channel(8);
    forward_oom_events(futures::stream::iter(items), tx).await;
    let mut out = Vec::new();
    while let Some(item) = rx.recv().await {
        out.push(item);
    }
    out
}

#[tokio::test]
async fn oom_events_carry_the_id_and_name() {
    let events = forwarded(vec![Ok(oom_event(Some("abc"), Some("web")))]).await;

    assert_eq!(events, [("abc".to_string(), "web".to_string())]);
}

#[tokio::test]
async fn an_oom_event_without_a_name_falls_back_to_the_short_id() {
    let id = "0123456789abcdef";

    let events = forwarded(vec![Ok(oom_event(Some(id), None))]).await;

    assert_eq!(events, [(id.to_string(), "0123456789ab".to_string())]);
}

#[tokio::test]
async fn an_oom_event_without_an_actor_is_forwarded_with_empty_fields() {
    // Pinned: an event naming no container still reaches subscribers.
    let events = forwarded(vec![Ok(EventMessage::default())]).await;

    assert_eq!(events, [(String::new(), String::new())]);
}

#[tokio::test(start_paused = true)]
async fn the_oom_listener_stops_at_the_first_stream_error() {
    // Pinned (finding 61): the log says "reconnecting", but the task just ends, so OOM
    // events are lost until the server restarts. Ending the stream does the same.
    let events = forwarded(vec![
        Ok(oom_event(Some("first"), Some("first"))),
        Err(bollard::errors::Error::RequestTimeoutError),
        Ok(oom_event(Some("second"), Some("second"))),
    ])
    .await;

    assert_eq!(events, [("first".to_string(), "first".to_string())]);
}

#[tokio::test]
async fn the_oom_listener_stops_when_nobody_is_listening() {
    let (tx, rx) = mpsc::channel(8);
    drop(rx);

    let stream = futures::stream::iter(vec![Ok(oom_event(Some("a"), None))]);
    forward_oom_events(stream, tx).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// event payloads
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn events_convert_to_their_public_names_and_data() {
    let cases = [
        (
            DockerTrackerEvent::ContainerStatusChanged {
                container: container("a"),
                previous: ContainerStatus::Exited,
            },
            "docker.container_status_changed",
            serde_json::json!({
                "id": "a-id",
                "name": "a",
                "image": "image",
                "status": "running",
                "previous_status": "exited",
            }),
        ),
        (
            DockerTrackerEvent::ContainerHealthChanged {
                container: container("a"),
                previous: ContainerHealth::Healthy,
            },
            "docker.container_health_changed",
            serde_json::json!({
                "id": "a-id",
                "name": "a",
                "image": "image",
                "health": "none",
                "previous_health": "healthy",
            }),
        ),
        (
            DockerTrackerEvent::ContainerOomKilled {
                id: "a-id".into(),
                name: "a".into(),
            },
            "docker.container_oom_killed",
            serde_json::json!({ "id": "a-id", "name": "a" }),
        ),
    ];

    for (event, name, data) in cases {
        let payload = EventPayload::from(&event);
        assert_eq!(payload.event, name);
        assert_eq!(payload.data, data, "{name}");
    }
}

#[test]
fn list_events_carry_the_count_and_the_containers() {
    for (event, name) in [
        (
            DockerTrackerEvent::InitialSnapshot {
                containers: vec![container("a")],
            },
            "docker.initial_snapshot",
        ),
        (
            DockerTrackerEvent::ContainersAppeared {
                containers: vec![container("a")],
            },
            "docker.containers_appeared",
        ),
        (
            DockerTrackerEvent::ContainersDisappeared {
                containers: vec![container("a")],
            },
            "docker.containers_disappeared",
        ),
    ] {
        let payload = EventPayload::from(&event);
        assert_eq!(payload.event, name);
        assert_eq!(payload.data["container_count"], 1);
        assert_eq!(payload.data["containers"][0]["name"], "a");
    }
}

#[test]
fn command_events_carry_the_action_and_the_error() {
    let event = DockerTrackerEvent::CommandExecuted {
        user: user(),
        action: DockerCommandAction::Start {
            id_or_name: "a".into(),
            container_name: Some("web".into()),
        },
        success: false,
        error: Some("nope".into()),
    };

    let payload = EventPayload::from(&event);

    assert_eq!(payload.event, "docker.command_executed");
    assert_eq!(payload.data["action"], "start");
    assert_eq!(payload.data["id_or_name"], "a");
    assert_eq!(payload.data["container_name"], "web");
    assert_eq!(payload.data["success"], false);
    assert_eq!(payload.data["error"], "nope");
}

// ─────────────────────────────────────────────────────────────────────────────
// the real source, with a broken daemon behind it
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bollard_source_turns_a_broken_daemon_into_errors() {
    // Hangs up on every request, which fails faster than a refused port on Windows.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        while let Ok((connection, _)) = listener.accept().await {
            drop(connection);
        }
    });
    let docker = Docker::connect_with_http(&url, 5, bollard::API_DEFAULT_VERSION).unwrap();
    let source = BollardSource(docker);

    let err = source.list().await.unwrap_err();

    assert!(err.to_string().contains("Docker API error"), "{err}");
    assert!(source.stats("a").await.is_none());
    assert!(source.kill("a", None).await.is_err());
    server.abort();
}
