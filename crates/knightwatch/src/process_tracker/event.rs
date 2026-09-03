use serde_json::json;

use kw_types::process::ProcessSnapshot;

#[derive(Debug, Clone)]
pub enum ProcessTrackerEvent {
    /// Emitted on the very first tick; contains everything we found.
    InitialSnapshot {
        root: Option<ProcessSnapshot>,
        children: Vec<ProcessSnapshot>,
    },
    /// One or more new child processes appeared.
    ChildrenAppeared {
        pid: u32,
        children: Vec<ProcessSnapshot>,
    },
    /// One or more child PIDs exited.
    ChildrenExited {
        pid: u32,
        children: Vec<u32>,
    },
    /// All descendants have exited (root may still be alive).
    AllChildrenGone {
        pid: u32,
    },
    /// The root process itself has exited.
    RootExited {
        pid: u32,
    },
    WorkComplete {
        pid: u32,
    },
    /// A process was killed via a `KillProcess` or `KillTree` command.
    ProcessKilled {
        pid: u32,
        /// `false` if the signal was sent but the OS reported failure,
        /// or if the process was not found.
        success: bool,
    },
    /// A user issued a mutating command (process action or poll-control),
    /// along with whether it succeeded.
    CommandExecuted {
        user: crate::prelude::DisplayUser,
        action: super::commands::ProcessCommandAction,
        success: bool,
        error: Option<String>,
    },
}

impl From<&ProcessTrackerEvent> for crate::events::EventPayload {
    fn from(event: &ProcessTrackerEvent) -> Self {
        let (event_name, data) = match event {
            ProcessTrackerEvent::RootExited { pid } => {
                ("process.root_exited", json!({ "pid": pid }))
            }
            ProcessTrackerEvent::ChildrenExited { pid, children } => (
                "process.children_exited",
                json!({ "pid": pid, "children": children }),
            ),
            ProcessTrackerEvent::ChildrenAppeared { pid, children } => (
                "process.children_appeared",
                json!({ "pid": pid, "children": children }),
            ),
            ProcessTrackerEvent::AllChildrenGone { pid } => {
                ("process.all_children_gone", json!({ "pid": pid }))
            }
            ProcessTrackerEvent::InitialSnapshot { root, children } => (
                "process.initial_snapshot",
                json!({
                    "root_pid": root.as_ref().map_or(0, |root| root.pid),
                    "child_count": children.len()
                }),
            ),
            ProcessTrackerEvent::WorkComplete { pid } => {
                ("process.work_complete", json!({ "pid": pid }))
            }
            ProcessTrackerEvent::ProcessKilled { pid, success } => (
                "process.process_killed",
                json!({ "pid": pid, "success": success }),
            ),
            ProcessTrackerEvent::CommandExecuted {
                user,
                action,
                success,
                error,
            } => (
                "process.command_executed",
                json!({
                    "user": format!("{user:?}"),
                    "action": action.name(),
                    "action_detail": format!("{action:?}"),
                    "success": success,
                    "error": error,
                }),
            ),
        };
        Self::new(crate::events::EventSource::ProcessTracker, event_name, data)
    }
}
