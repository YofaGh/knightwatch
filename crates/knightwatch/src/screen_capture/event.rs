use serde_json::json;

use kw_types::event;

#[derive(Debug, Clone)]
pub enum ScreenCaptureEvent {
    /// A user issued a mutating command (poll-control),
    /// along with whether it succeeded.
    #[allow(dead_code)]
    CommandExecuted {
        user: crate::prelude::DisplayUser,
        action: super::commands::ScreenCaptureAction,
        success: bool,
        error: Option<String>,
    },
}

impl From<&ScreenCaptureEvent> for event::EventPayload {
    fn from(event: &ScreenCaptureEvent) -> Self {
        let (event_name, data) = match event {
            ScreenCaptureEvent::CommandExecuted {
                user,
                action,
                success,
                error,
            } => (
                "screen.command_executed",
                json!({
                    "user": format!("{user:?}"),
                    "action": action.name(),
                    "action_detail": format!("{action:?}"),
                    "success": success,
                    "error": error,
                }),
            ),
        };
        Self::new(
            crate::utils::get_version().to_string(),
            event::EventSource::ScreenCapture,
            event_name,
            crate::utils::now_rfc3339(),
            data,
        )
    }
}
