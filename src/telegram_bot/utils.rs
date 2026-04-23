use super::models::TelegramDisplay;
use crate::process_tracker::{enums::ProcessTrackerEvent, utils::snapshot_to_response};

pub fn format_event(event: &ProcessTrackerEvent) -> String {
    match event {
        ProcessTrackerEvent::InitialSnapshot { root, children } => {
            let root_info = TelegramDisplay(&snapshot_to_response(root));
            let mut msg = format!("🟢 *Initial Snapshot*\n\n*Root:*\n{root_info}");
            if children.is_empty() {
                msg.push_str("\n\n*Children:* _none_");
            } else {
                msg.push_str(&format!("\n\n*Children* ({}):", children.len()));
                for child in children {
                    let child_info = TelegramDisplay(&snapshot_to_response(child));
                    msg.push_str(&format!("\n{child_info}\n"));
                }
            }
            msg
        }
        ProcessTrackerEvent::ChildrenAppeared(snapshots) => {
            let mut msg = format!("🆕 *New Children Appeared* ({})", snapshots.len());
            for snap in snapshots {
                let info = TelegramDisplay(&snapshot_to_response(snap));
                msg.push_str(&format!("\n{info}\n"));
            }
            msg
        }
        ProcessTrackerEvent::ChildrenExited(pids) => {
            let pid_list = pids
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("🔴 *Children Exited*\nPIDs: {pid_list}")
        }
        ProcessTrackerEvent::AllChildrenGone => {
            "✅ *All children have exited*\nThe root process may still be running\\.".to_string()
        }
        ProcessTrackerEvent::RootExited { pid } => {
            format!("💀 *Root Process Exited*\nPID: `{pid}`")
        }
    }
}
