#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::{cell::RefCell, collections::HashSet, time::Duration};
use sysinfo::Signal;
use tokio::sync::oneshot;

use kw_types::{
    event::EventPayload,
    polling::PollStatus,
    process::{ProcessSignal, ProcessStatus, ProcessTree},
};

use super::{super::commands::ProcessCommandAction, *};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct FakeProc {
    parent: Option<u32>,
    cpu: f32,
    memory: u64,
    disk: u64,
}

/// `live` is the machine; `seen` is what the last refresh saw. Reads use `seen`,
/// so a code path that forgets to refresh fails its test.
#[derive(Default)]
struct Fake {
    live: HashMap<u32, FakeProc>,
    seen: HashMap<u32, FakeProc>,
    refuse_kill: HashSet<u32>,
    kills: RefCell<Vec<u32>>,
}

impl Fake {
    fn try_kill(&self, pid: u32) -> Option<bool> {
        self.seen.get(&pid)?;
        if self.refuse_kill.contains(&pid) {
            return Some(false);
        }
        self.kills.borrow_mut().push(pid);
        Some(true)
    }
}

impl ProcessSource for Fake {
    fn refresh_usage(&mut self) {
        self.seen.clone_from(&self.live);
    }

    fn refresh_all(&mut self) {
        self.seen.clone_from(&self.live);
    }

    fn refresh_pid(&mut self, pid: u32) {
        match self.live.get(&pid) {
            Some(proc) => self.seen.insert(pid, proc.clone()),
            None => self.seen.remove(&pid),
        };
    }

    fn snapshot(&self, pid: u32) -> Option<ProcessSnapshot> {
        let proc = self.seen.get(&pid)?;
        Some(ProcessSnapshot {
            pid,
            name: format!("proc{pid}"),
            state: ProcessState::Running,
            cpu_usage: proc.cpu,
            memory_bytes: proc.memory,
            disk_usage: proc.disk,
            cwd: None,
            cmdline: Vec::new(),
            open_files: Vec::new(),
            io_stats: None,
        })
    }

    fn parents(&self) -> Vec<(u32, u32)> {
        self.seen
            .iter()
            .filter_map(|(pid, proc)| Some((*pid, proc.parent?)))
            .collect()
    }

    fn usage(&self) -> Vec<(u32, f32, u64, u64)> {
        self.seen
            .iter()
            .map(|(pid, p)| (*pid, p.cpu, p.memory, p.disk))
            .collect()
    }

    fn kill(&self, pid: u32) -> bool {
        self.try_kill(pid).unwrap_or(false)
    }

    fn kill_with(&self, pid: u32, _signal: Signal) -> Option<bool> {
        self.try_kill(pid)
    }
}

fn tracker(roots: &[u32]) -> ProcessTracker<Fake> {
    ProcessTracker {
        state: ProcessTrackerState::new(roots.to_vec()),
        channels: ProcessTrackerChannels::new(),
        source: Fake::default(),
        poll: Poll::new(2),
        track_top_processes: false,
        limit_processes: 10,
    }
}

impl ProcessTracker<Fake> {
    fn spawn(&mut self, pid: u32, parent: Option<u32>) {
        self.source.live.insert(
            pid,
            FakeProc {
                parent,
                ..FakeProc::default()
            },
        );
    }

    fn exit(&mut self, pid: u32) {
        self.source.live.remove(&pid);
    }

    fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> ProcessTrackerQuery) -> T {
        let (response, mut rx) = oneshot::channel();
        self.handle_query(make(response));
        rx.try_recv().unwrap()
    }

    fn run<T>(&mut self, make: impl FnOnce(oneshot::Sender<T>) -> ProcessTrackerCommand) -> T {
        let (response, mut rx) = oneshot::channel();
        self.handle_command(make(response));
        rx.try_recv().unwrap()
    }
}

fn user() -> DisplayUser {
    DisplayUser {
        username: "alice".into(),
        telegram_chat_id: None,
    }
}

fn pids(snaps: &[ProcessSnapshot]) -> Vec<u32> {
    let mut out: Vec<u32> = snaps.iter().map(|s| s.pid).collect();
    out.sort_unstable();
    out
}

fn sorted(mut pids: Vec<u32>) -> Vec<u32> {
    pids.sort_unstable();
    pids
}

/// Events as short strings, because `ProcessTrackerEvent` has no `PartialEq`.
fn drain(rx: &mut broadcast::Receiver<ProcessTrackerEvent>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(match event {
            ProcessTrackerEvent::InitialSnapshot { root, children } => format!(
                "initial root={:?} children={:?}",
                root.map(|r| r.pid),
                pids(&children)
            ),
            ProcessTrackerEvent::ChildrenAppeared { pid, children } => {
                format!("appeared {pid} {:?}", pids(&children))
            }
            ProcessTrackerEvent::ChildrenExited { pid, children } => {
                format!("exited {pid} {:?}", sorted(children))
            }
            ProcessTrackerEvent::AllChildrenGone { pid } => format!("all_gone {pid}"),
            ProcessTrackerEvent::RootExited { pid } => format!("root_exited {pid}"),
            ProcessTrackerEvent::WorkComplete { pid } => format!("complete {pid}"),
            ProcessTrackerEvent::ProcessKilled { pid, success } => {
                format!("killed {pid} {success}")
            }
            ProcessTrackerEvent::CommandExecuted {
                action,
                success,
                error,
                ..
            } => format!("command {} {success} {}", action.name(), error.is_some()),
        });
    }
    out
}

/// A root (pid 1) with no children yet, already past its first tick.
fn running_root() -> (
    ProcessTracker<Fake>,
    broadcast::Receiver<ProcessTrackerEvent>,
) {
    let mut t = tracker(&[1]);
    t.spawn(1, None);
    t.handle_tick();
    let rx = t.channels.event_tx.subscribe();
    (t, rx)
}

// ─────────────────────────────────────────────────────────────────────────────
// diff to events
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn first_tick_reports_the_whole_subtree_as_one_snapshot() {
    let mut t = tracker(&[1]);
    let mut rx = t.channels.event_tx.subscribe();
    t.spawn(1, None);
    t.spawn(2, Some(1));
    t.spawn(3, Some(2));
    t.spawn(50, None); // unrelated
    t.spawn(51, Some(50)); // child of something else

    t.handle_tick();

    assert_eq!(drain(&mut rx), ["initial root=Some(1) children=[2, 3]"]);
}

#[test]
fn first_tick_without_the_root_is_not_work_done() {
    let mut t = tracker(&[1]);
    let mut rx = t.channels.event_tx.subscribe();

    t.handle_tick();

    assert_eq!(
        drain(&mut rx),
        ["root_exited 1", "initial root=None children=[]"]
    );
    let done = t.ask(|response| ProcessTrackerQuery::IsWorkDone {
        root_pid: 1,
        response,
    });
    assert_eq!(done, Some(false));
}

#[test]
fn a_root_that_appears_late_is_picked_up_without_a_second_snapshot() {
    let mut t = tracker(&[1]);
    t.handle_tick();
    let mut rx = t.channels.event_tx.subscribe();
    t.spawn(1, None);
    t.spawn(2, Some(1));

    t.handle_tick();

    assert_eq!(drain(&mut rx), ["appeared 1 [2]"]);
}

#[test]
fn an_unchanged_tick_emits_nothing() {
    let (mut t, mut rx) = running_root();
    t.spawn(2, Some(1));
    t.handle_tick();
    drain(&mut rx);

    t.handle_tick();

    assert!(drain(&mut rx).is_empty());
}

#[test]
fn new_children_are_reported_once_and_only_the_new_ones() {
    let (mut t, mut rx) = running_root();
    t.spawn(2, Some(1));
    t.handle_tick();
    assert_eq!(drain(&mut rx), ["appeared 1 [2]"]);

    t.spawn(3, Some(1));
    t.spawn(4, Some(3));
    t.handle_tick();
    assert_eq!(drain(&mut rx), ["appeared 1 [3, 4]"]);
}

#[test]
fn a_partial_exit_does_not_claim_all_children_gone() {
    let (mut t, mut rx) = running_root();
    t.spawn(2, Some(1));
    t.spawn(3, Some(1));
    t.handle_tick();
    drain(&mut rx);

    t.exit(2);
    t.handle_tick();

    assert_eq!(drain(&mut rx), ["exited 1 [2]"]);
}

#[test]
fn all_children_gone_fires_once_on_the_transition_and_the_root_stays_tracked() {
    let (mut t, mut rx) = running_root();
    t.spawn(2, Some(1));
    t.handle_tick();
    drain(&mut rx);

    t.exit(2);
    t.handle_tick();
    assert_eq!(drain(&mut rx), ["exited 1 [2]", "all_gone 1"]);

    t.handle_tick();
    assert!(drain(&mut rx).is_empty());

    // Not done while the root lives, so a new child is still seen.
    t.spawn(3, Some(1));
    t.handle_tick();
    assert_eq!(drain(&mut rx), ["appeared 1 [3]"]);
}

#[test]
fn root_exit_without_children_completes_and_then_goes_silent() {
    let (mut t, mut rx) = running_root();

    t.exit(1);
    t.handle_tick();
    assert_eq!(drain(&mut rx), ["root_exited 1", "complete 1"]);

    t.handle_tick();
    assert!(drain(&mut rx).is_empty());

    // A reused pid is not picked up again once the work is done.
    t.spawn(1, None);
    t.handle_tick();
    assert!(drain(&mut rx).is_empty());
}

#[test]
fn work_completes_only_after_the_last_orphan_exits() {
    let (mut t, mut rx) = running_root();
    t.spawn(2, Some(1));
    t.handle_tick();
    drain(&mut rx);

    t.exit(1);
    t.handle_tick();
    assert_eq!(drain(&mut rx), ["root_exited 1"]);

    t.exit(2);
    t.handle_tick();
    let events = drain(&mut rx);
    assert_eq!(
        events[events.len() - 3..],
        ["exited 1 [2]", "all_gone 1", "complete 1"]
    );
}

#[test]
#[ignore = "finding 51: RootExited repeats on every tick until the work is done"]
fn root_exited_is_emitted_once_while_orphans_live() {
    let (mut t, mut rx) = running_root();
    t.spawn(2, Some(1));
    t.handle_tick();
    drain(&mut rx);

    t.exit(1);
    t.handle_tick();
    t.handle_tick();

    assert_eq!(drain(&mut rx), ["root_exited 1"]);
}

#[test]
#[ignore = "finding 51: a bad pid emits RootExited on every tick, forever"]
fn root_exited_is_emitted_once_for_a_root_that_never_appeared() {
    let mut t = tracker(&[1]);
    let mut rx = t.channels.event_tx.subscribe();

    t.handle_tick();
    t.handle_tick();

    assert_eq!(
        drain(&mut rx),
        ["root_exited 1", "initial root=None children=[]"]
    );
}

#[test]
fn roots_are_tracked_independently() {
    let mut t = tracker(&[1, 10]);
    t.spawn(1, None);
    t.spawn(10, None);
    t.handle_tick();
    let mut rx = t.channels.event_tx.subscribe();

    t.exit(1);
    t.spawn(11, Some(10));
    t.handle_tick();

    let mut events = drain(&mut rx);
    events.sort();
    assert_eq!(events, ["appeared 10 [11]", "complete 1", "root_exited 1"]);
}

#[test]
fn a_child_shared_by_nested_roots_is_reported_for_both() {
    let mut t = tracker(&[1, 2]);
    t.spawn(1, None);
    t.spawn(2, Some(1));
    t.handle_tick();
    let mut rx = t.channels.event_tx.subscribe();

    t.spawn(3, Some(2));
    t.handle_tick();

    let mut events = drain(&mut rx);
    events.sort();
    assert_eq!(events, ["appeared 1 [3]", "appeared 2 [3]"]);
}

// ─────────────────────────────────────────────────────────────────────────────
// queries
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn queries_report_the_last_tick() {
    let (mut t, _rx) = running_root();
    t.spawn(2, Some(1));
    t.handle_tick();

    let root = t.ask(|response| ProcessTrackerQuery::GetRoot {
        root_pid: 1,
        response,
    });
    assert_eq!(root.unwrap().pid, 1);

    let children = t.ask(|response| ProcessTrackerQuery::GetChildren {
        root_pid: 1,
        response,
    });
    assert_eq!(pids(&children), [2]);

    let tree: Option<ProcessTree> = t.ask(|response| ProcessTrackerQuery::GetProcessTree {
        root_pid: 1,
        response,
    });
    let tree = tree.unwrap();
    assert_eq!(tree.root_pid, 1);
    assert_eq!(tree.child_count, 1);
    assert!(!tree.work_done);

    let status: Option<ProcessStatus> = t.ask(|response| ProcessTrackerQuery::GetProcessStatus {
        root_pid: 1,
        response,
    });
    let status = status.unwrap();
    assert!(status.root_alive);
    assert_eq!(status.root_pid, Some(1));
    assert_eq!(status.root_name.as_deref(), Some("proc1"));
    assert_eq!(status.child_count, 1);

    let trees = t.ask(|response| ProcessTrackerQuery::GetAllProcessTrees { response });
    assert_eq!(trees.len(), 1);
    assert_eq!(
        t.ask(|response| ProcessTrackerQuery::GetTrackedPids { response }),
        [1]
    );
}

#[test]
fn queries_about_an_untracked_pid_are_empty() {
    let t = tracker(&[]);

    assert!(
        t.ask(|response| ProcessTrackerQuery::GetRoot {
            root_pid: 9,
            response
        })
        .is_none()
    );
    assert!(
        t.ask(|response| ProcessTrackerQuery::GetChildren {
            root_pid: 9,
            response
        })
        .is_empty()
    );
    assert_eq!(
        t.ask(|response| ProcessTrackerQuery::IsWorkDone {
            root_pid: 9,
            response
        }),
        None
    );
    assert!(
        t.ask(|response| ProcessTrackerQuery::GetProcessTree {
            root_pid: 9,
            response
        })
        .is_none()
    );
    assert!(
        t.ask(|response| ProcessTrackerQuery::GetProcessStatus {
            root_pid: 9,
            response
        })
        .is_none()
    );
    assert!(
        t.ask(|response| ProcessTrackerQuery::GetAllProcessTrees { response })
            .is_empty()
    );
}

#[test]
fn an_exited_root_reads_as_none_not_as_gone() {
    // Pinned: the `ProcessState::Gone` write in `update_root_pid_state` is overwritten
    // a few lines later, so `Gone` is never visible (finding 52).
    let (mut t, _rx) = running_root();
    t.exit(1);
    t.handle_tick();

    assert!(
        t.ask(|response| ProcessTrackerQuery::GetRoot {
            root_pid: 1,
            response
        })
        .is_none()
    );
    let status = t
        .ask(|response| ProcessTrackerQuery::GetProcessStatus {
            root_pid: 1,
            response,
        })
        .unwrap();
    assert!(!status.root_alive);
    assert_eq!(status.root_pid, None);
    assert_eq!(status.root_name, None);
    assert!(status.work_done);
}

// ─────────────────────────────────────────────────────────────────────────────
// top processes
// ─────────────────────────────────────────────────────────────────────────────

/// Pids 1..=3: cpu is highest on 1, memory on 2, disk on 3.
fn top_tracker(limit_processes: usize) -> ProcessTracker<Fake> {
    let mut t = tracker(&[]);
    t.track_top_processes = true;
    t.limit_processes = limit_processes;
    for (pid, cpu, memory, disk) in [(1, 90.0, 20, 10), (2, 50.0, 30, 20), (3, 10.0, 10, 30)] {
        t.source.live.insert(
            pid,
            FakeProc {
                parent: None,
                cpu,
                memory,
                disk,
            },
        );
    }
    t
}

fn top(t: &ProcessTracker<Fake>, by: ProcessesSortKey, limit: usize) -> Vec<u32> {
    let snaps = t.ask(|response| ProcessTrackerQuery::GetTopProcesses {
        by,
        limit,
        response,
    });
    snaps.iter().map(|s| s.pid).collect()
}

#[test]
fn top_processes_are_sorted_per_key() {
    let mut t = top_tracker(10);
    t.handle_tick();

    assert_eq!(top(&t, ProcessesSortKey::Cpu, 0), [1, 2, 3]);
    assert_eq!(top(&t, ProcessesSortKey::Memory, 0), [2, 1, 3]);
    assert_eq!(top(&t, ProcessesSortKey::Disk, 0), [3, 2, 1]);
}

#[test]
fn top_query_limit_is_capped_by_the_configured_limit() {
    let mut t = top_tracker(2);
    t.handle_tick();

    assert_eq!(top(&t, ProcessesSortKey::Cpu, 0), [1, 2], "0 means the cap");
    assert_eq!(top(&t, ProcessesSortKey::Cpu, 1), [1]);
    assert_eq!(top(&t, ProcessesSortKey::Cpu, 99), [1, 2]);
}

#[test]
fn top_processes_drop_exited_pids_on_the_next_tick() {
    let mut t = top_tracker(10);
    t.handle_tick();

    t.exit(1);
    t.handle_tick();

    assert_eq!(top(&t, ProcessesSortKey::Cpu, 0), [2, 3]);
    assert!(!t.state.process_cache.contains_key(&1));
}

#[test]
fn top_processes_are_empty_when_not_enabled() {
    let mut t = top_tracker(10);
    t.track_top_processes = false;
    t.handle_tick();

    assert!(top(&t, ProcessesSortKey::Cpu, 0).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// kill commands
// ─────────────────────────────────────────────────────────────────────────────

fn kill(t: &mut ProcessTracker<Fake>, pid: u32) -> Result<bool> {
    t.run(|response| ProcessTrackerCommand::KillProcess {
        user: user(),
        pid,
        signal: ProcessSignal::Kill,
        response,
    })
}

fn kill_tree(t: &mut ProcessTracker<Fake>, root_pid: u32) -> Vec<u32> {
    t.run(|response| ProcessTrackerCommand::KillTree {
        user: user(),
        root_pid,
        response,
    })
    .unwrap()
}

#[test]
fn killing_a_process_reports_it() {
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();
    t.spawn(5, None); // Not ticked yet: the command must refresh it itself.

    assert!(kill(&mut t, 5).unwrap());

    assert_eq!(*t.source.kills.borrow(), [5]);
    assert_eq!(
        drain(&mut rx),
        ["killed 5 true", "command kill_process true false"]
    );
}

#[test]
fn killing_a_missing_process_is_an_error() {
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();

    let err = kill(&mut t, 5).unwrap_err();

    assert!(err.to_string().contains("Process 5 not found"), "{err}");
    assert_eq!(drain(&mut rx), ["command kill_process false true"]);
}

#[test]
fn killing_a_process_that_exited_since_the_last_tick_is_an_error() {
    let (mut t, _rx) = running_root();
    t.exit(1);

    assert!(kill(&mut t, 1).is_err());
}

#[test]
fn a_refused_kill_is_ok_false_but_the_command_event_says_success() {
    // Pinned: `CommandExecuted.success` means "ran", so a refused kill still reads true.
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();
    t.spawn(5, None);
    t.source.refuse_kill.insert(5);

    assert!(!kill(&mut t, 5).unwrap());

    assert_eq!(
        drain(&mut rx),
        ["killed 5 false", "command kill_process true false"]
    );
}

#[cfg(windows)]
#[test]
fn an_unsupported_signal_is_an_error_and_kills_nothing() {
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();
    t.spawn(5, None);

    let result = t.run(|response| ProcessTrackerCommand::KillProcess {
        user: user(),
        pid: 5,
        signal: ProcessSignal::Term,
        response,
    });

    assert!(result.is_err());
    assert!(t.source.kills.borrow().is_empty());
    assert_eq!(drain(&mut rx), ["command kill_process false true"]);
}

#[test]
fn kill_tree_kills_descendants_and_the_root_but_nothing_else() {
    let mut t = tracker(&[1]);
    t.spawn(1, None);
    t.spawn(2, Some(1));
    t.spawn(3, Some(2));
    t.spawn(50, None);
    let mut rx = t.channels.event_tx.subscribe();

    let killed = kill_tree(&mut t, 1);

    assert_eq!(sorted(killed), [1, 2, 3]);
    assert_eq!(sorted(t.source.kills.borrow().clone()), [1, 2, 3]);
    let events = drain(&mut rx);
    assert_eq!(events.len(), 4);
    assert_eq!(events[3], "command kill_tree true false");
}

#[test]
fn kill_tree_leaves_out_pids_that_refused() {
    let mut t = tracker(&[]);
    t.spawn(1, None);
    t.spawn(2, Some(1));
    t.source.refuse_kill.insert(2);
    let mut rx = t.channels.event_tx.subscribe();

    assert_eq!(kill_tree(&mut t, 1), [1]);

    assert_eq!(
        drain(&mut rx),
        ["killed 1 true", "command kill_tree true false"]
    );
}

#[test]
fn kill_tree_of_an_unknown_root_succeeds_with_nothing_killed() {
    // Pinned: unlike `KillProcess`, a missing root is `Ok`, and the event says success.
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();

    assert!(kill_tree(&mut t, 9).is_empty());

    assert_eq!(drain(&mut rx), ["command kill_tree true false"]);
}

#[test]
#[ignore = "finding 53: the code says leaves first, but parents are killed before children"]
fn kill_tree_kills_leaves_before_their_parents() {
    let mut t = tracker(&[]);
    t.spawn(1, None);
    t.spawn(2, Some(1));
    t.spawn(3, Some(2));

    assert_eq!(kill_tree(&mut t, 1), [3, 2, 1]);
}

// ─────────────────────────────────────────────────────────────────────────────
// track and untrack
// ─────────────────────────────────────────────────────────────────────────────

fn track(t: &mut ProcessTracker<Fake>, pid: u32) {
    t.run(|response| ProcessTrackerCommand::TrackPid {
        user: user(),
        pid,
        response,
    })
    .unwrap();
}

#[test]
fn a_newly_tracked_pid_gets_its_own_first_tick() {
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();
    t.spawn(5, None);

    track(&mut t, 5);
    t.handle_tick();

    assert_eq!(
        drain(&mut rx),
        [
            "command track_pid true false",
            "initial root=Some(5) children=[]"
        ]
    );
}

#[test]
fn tracking_a_pid_twice_keeps_its_state() {
    let (mut t, mut rx) = running_root();

    track(&mut t, 1);
    t.handle_tick();

    assert_eq!(drain(&mut rx), ["command track_pid true false"]);
}

#[test]
fn tracking_a_finished_pid_again_does_not_restart_it() {
    // Pinned: only untracking first gives a finished pid a fresh start.
    let (mut t, mut rx) = running_root();
    t.exit(1);
    t.handle_tick();
    drain(&mut rx);
    t.spawn(1, None);

    track(&mut t, 1);
    t.handle_tick();

    assert_eq!(drain(&mut rx), ["command track_pid true false"]);
}

#[test]
fn untracking_discards_state_and_stops_events() {
    let (mut t, mut rx) = running_root();

    t.run(|response| ProcessTrackerCommand::UntrackPid {
        user: user(),
        pid: 1,
        response,
    })
    .unwrap();
    t.exit(1);
    t.handle_tick();

    assert_eq!(drain(&mut rx), ["command untrack_pid true false"]);
    assert!(
        t.ask(|response| ProcessTrackerQuery::GetTrackedPids { response })
            .is_empty()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// polling
// ─────────────────────────────────────────────────────────────────────────────

fn poll_status(t: &ProcessTracker<Fake>) -> PollStatus {
    t.ask(|response| ProcessTrackerQuery::PollStatus { response })
        .unwrap()
}

fn set_interval(t: &mut ProcessTracker<Fake>, interval: Duration) -> Result<()> {
    t.run(|response| ProcessTrackerCommand::SetPollInterval {
        user: user(),
        interval,
        response,
    })
}

#[tokio::test]
async fn poll_commands_change_the_poll_and_report_it() {
    let mut t = tracker(&[]);
    let mut rx = t.channels.event_tx.subscribe();
    assert!(poll_status(&t).paused, "paused until the loop starts");

    t.run(|response| ProcessTrackerCommand::ResumePoll {
        user: user(),
        response,
    })
    .unwrap();
    assert!(!poll_status(&t).paused);

    set_interval(&mut t, Duration::from_millis(250)).unwrap();
    let status = poll_status(&t);
    assert_eq!(status.interval, 250);
    assert!(!status.paused);

    t.run(|response| ProcessTrackerCommand::PausePoll {
        user: user(),
        response,
    })
    .unwrap();
    assert!(poll_status(&t).paused);

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
    let mut t = tracker(&[]);

    set_interval(&mut t, Duration::from_millis(250)).unwrap();

    let status = poll_status(&t);
    assert_eq!(status.interval, 250);
    assert!(status.paused);
}

#[tokio::test]
#[ignore = "finding 3: a zero interval panics and kills the tracker task"]
async fn a_zero_interval_is_rejected_while_running() {
    let mut t = tracker(&[]);
    t.poll.resume();

    assert!(set_interval(&mut t, Duration::ZERO).is_err());
    assert_eq!(poll_status(&t).interval, 2000);
}

#[tokio::test]
#[ignore = "finding 3: while paused, a zero interval is accepted and panics on resume"]
async fn a_zero_interval_is_rejected_while_paused() {
    let mut t = tracker(&[]);

    assert!(set_interval(&mut t, Duration::ZERO).is_err());
    assert_eq!(poll_status(&t).interval, 2000);
}

// ─────────────────────────────────────────────────────────────────────────────
// the running loop
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_loop_ticks_and_answers_queries_and_commands() {
    let mut t = tracker(&[1]);
    t.spawn(1, None);
    let query_tx = t.channels.query_tx.clone();
    let command_tx = t.channels.command_tx.clone();
    let mut rx = t.channels.event_tx.subscribe();
    let task = tokio::spawn(t.start_tracking_loop());
    let wait = Duration::from_secs(5);

    let first = tokio::time::timeout(wait, rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        first,
        ProcessTrackerEvent::InitialSnapshot { root: Some(_), .. }
    ));

    let (response, answer) = oneshot::channel();
    query_tx
        .send(ProcessTrackerQuery::GetTrackedPids { response })
        .await
        .unwrap();
    let tracked = tokio::time::timeout(wait, answer).await.unwrap().unwrap();
    assert_eq!(tracked, [1]);

    let (response, answer) = oneshot::channel();
    command_tx
        .send(ProcessTrackerCommand::PausePoll {
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

#[test]
fn events_convert_to_their_public_names_and_data() {
    let snap = Fake {
        seen: HashMap::from([(7, FakeProc::default())]),
        ..Fake::default()
    }
    .snapshot(7)
    .unwrap();
    let cases = [
        (
            ProcessTrackerEvent::InitialSnapshot {
                root: Some(snap.clone()),
                children: vec![snap.clone(), snap],
            },
            "process.initial_snapshot",
            serde_json::json!({ "root_pid": 7, "child_count": 2 }),
        ),
        (
            ProcessTrackerEvent::InitialSnapshot {
                root: None,
                children: Vec::new(),
            },
            "process.initial_snapshot",
            serde_json::json!({ "root_pid": 0, "child_count": 0 }),
        ),
        (
            ProcessTrackerEvent::ChildrenExited {
                pid: 1,
                children: vec![2],
            },
            "process.children_exited",
            serde_json::json!({ "pid": 1, "children": [2] }),
        ),
        (
            ProcessTrackerEvent::AllChildrenGone { pid: 1 },
            "process.all_children_gone",
            serde_json::json!({ "pid": 1 }),
        ),
        (
            ProcessTrackerEvent::RootExited { pid: 1 },
            "process.root_exited",
            serde_json::json!({ "pid": 1 }),
        ),
        (
            ProcessTrackerEvent::WorkComplete { pid: 1 },
            "process.work_complete",
            serde_json::json!({ "pid": 1 }),
        ),
        (
            ProcessTrackerEvent::ProcessKilled {
                pid: 1,
                success: false,
            },
            "process.process_killed",
            serde_json::json!({ "pid": 1, "success": false }),
        ),
    ];

    for (event, name, data) in cases {
        let payload = EventPayload::from(&event);
        assert_eq!(payload.event, name);
        assert_eq!(payload.data, data, "{name}");
    }
}

#[test]
fn command_events_carry_the_action_and_the_error() {
    let event = ProcessTrackerEvent::CommandExecuted {
        user: user(),
        action: ProcessCommandAction::TrackPid { pid: 4 },
        success: false,
        error: Some("nope".into()),
    };

    let payload = EventPayload::from(&event);

    assert_eq!(payload.event, "process.command_executed");
    assert_eq!(payload.data["action"], "track_pid");
    assert_eq!(payload.data["success"], false);
    assert_eq!(payload.data["error"], "nope");
}

// ─────────────────────────────────────────────────────────────────────────────
// the real source
// ─────────────────────────────────────────────────────────────────────────────

fn long_running_child() -> std::process::Child {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("ping");
        c.args(["-n", "30", "127.0.0.1"]);
        c
    } else {
        let mut c = std::process::Command::new("sleep");
        c.arg("30");
        c
    };
    command
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn child")
}

#[test]
fn sysinfo_source_sees_and_kills_a_real_child() {
    let mut child = long_running_child();
    let (me, pid) = (std::process::id(), child.id());
    let mut source = SysinfoSource(System::new());

    source.refresh_usage();

    assert_eq!(source.snapshot(pid).unwrap().pid, pid);
    assert!(source.parents().contains(&(pid, me)));
    assert!(source.usage().iter().any(|entry| entry.0 == pid));

    assert!(source.kill(pid));
    assert!(!child.wait().unwrap().success());
}

#[test]
fn sysinfo_source_treats_a_missing_pid_as_missing() {
    // Too large to be a real pid on any supported platform.
    let missing = u32::MAX - 1;
    let mut source = SysinfoSource(System::new());

    source.refresh_pid(missing);

    assert!(source.snapshot(missing).is_none());
    assert!(!source.kill(missing));
    assert_eq!(source.kill_with(missing, Signal::Kill), None);
}
