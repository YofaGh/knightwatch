use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum EventSource {
    #[default]
    ScreenCapture,
    ProcessTracker,
    SystemResources,
    Systemd,
    DockerTracker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub version: String,
    #[serde(skip)]
    pub source: EventSource,
    pub event: String,
    pub timestamp: String,
    pub data: Value,
}

impl EventPayload {
    pub fn new(source: EventSource, event: &'static str, data: Value) -> Self {
        Self {
            version: crate::utils::get_version().to_string(),
            source,
            event: event.to_string(),
            timestamp: crate::utils::now_rfc3339(),
            data,
        }
    }
    pub fn is_screen_capture(&self) -> bool {
        self.source == EventSource::ScreenCapture
    }
    pub fn is_process_tracker(&self) -> bool {
        self.source == EventSource::ProcessTracker
    }
    pub fn is_system_resources(&self) -> bool {
        self.source == EventSource::SystemResources
    }
    pub fn is_systemd(&self) -> bool {
        self.source == EventSource::Systemd
    }
    pub fn is_docker_tracker(&self) -> bool {
        self.source == EventSource::DockerTracker
    }
    pub fn is_tick(&self) -> bool {
        matches!(self.event.as_str(), "resources.tick" | "systemd.tick")
    }
}
