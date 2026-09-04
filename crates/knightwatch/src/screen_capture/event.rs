use serde_json::json;

#[derive(Debug, Clone)]
pub enum ScreenCaptureEvent {
    /// A user issued a mutating command (poll-control),
    /// along with whether it succeeded.
    CommandExecuted {
        user: crate::prelude::DisplayUser,
        action: super::commands::ScreenCaptureAction,
        success: bool,
        error: Option<String>,
    },
}

impl From<&ScreenCaptureEvent> for crate::events::EventPayload {
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
        Self::new(crate::events::EventSource::ScreenCapture, event_name, data)
    }
}
