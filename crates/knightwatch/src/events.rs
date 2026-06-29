use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSource {
    ProcessTracker,
    SystemResources,
    Systemd,
    DockerTracker,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EventPayload {
    pub version: &'static str,
    #[serde(skip)]
    pub source: EventSource,
    pub event: &'static str,
    pub timestamp: String,
    pub data: Value,
}

impl EventPayload {
    pub fn new(source: EventSource, event: &'static str, data: Value) -> Self {
        Self {
            version: crate::utils::get_version(),
            source,
            event,
            timestamp: crate::utils::now_rfc3339(),
            data,
        }
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
}
