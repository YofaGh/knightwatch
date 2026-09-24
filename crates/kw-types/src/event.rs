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
    #[must_use]
    pub fn new(
        version: String,
        source: EventSource,
        event: &'static str,
        timestamp: String,
        data: Value,
    ) -> Self {
        Self {
            version,
            source,
            event: event.to_string(),
            timestamp,
            data,
        }
    }
    #[must_use]
    pub fn is_screen_capture(&self) -> bool {
        self.source == EventSource::ScreenCapture
    }
    #[must_use]
    pub fn is_process_tracker(&self) -> bool {
        self.source == EventSource::ProcessTracker
    }
    #[must_use]
    pub fn is_system_resources(&self) -> bool {
        self.source == EventSource::SystemResources
    }
    #[must_use]
    pub fn is_systemd(&self) -> bool {
        self.source == EventSource::Systemd
    }
    #[must_use]
    pub fn is_docker_tracker(&self) -> bool {
        self.source == EventSource::DockerTracker
    }
    #[must_use]
    pub fn is_tick(&self) -> bool {
        matches!(self.event.as_str(), "resources.tick" | "systemd.tick")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub event: String,
    pub timestamp: String,
    pub source: EventSource,
    pub data: Value,
}

impl From<&EventPayload> for StoredEvent {
    fn from(p: &EventPayload) -> Self {
        Self {
            event: p.event.clone(),
            timestamp: p.timestamp.clone(),
            source: p.source,
            data: p.data.clone(),
        }
    }
}
