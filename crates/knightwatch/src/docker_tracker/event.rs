use serde_json::json;

use kw_types::docker::ContainerSnapshot;

#[derive(Debug, Clone)]
pub enum DockerTrackerEvent {
    /// Emitted once on the very first tick with every container currently running.
    InitialSnapshot { containers: Vec<ContainerSnapshot> },

    /// One or more containers that weren't present last tick are now running.
    ContainersAppeared { containers: Vec<ContainerSnapshot> },

    /// One or more containers that were present last tick are no longer listed.
    /// Carries the last known snapshot so callers have name/image context.
    ContainersDisappeared { containers: Vec<ContainerSnapshot> },

    /// A container's lifecycle status changed (e.g. running → exited).
    ContainerStatusChanged {
        container: ContainerSnapshot,
        previous: kw_types::docker::ContainerStatus,
    },

    /// A container's health check result changed (healthy ↔ unhealthy ↔ starting).
    ContainerHealthChanged {
        container: ContainerSnapshot,
        previous: kw_types::docker::ContainerHealth,
    },

    /// A container was OOM-killed by the kernel. Derived from the Docker events
    /// stream (`oom` action) rather than poll diffing.
    ContainerOomKilled { id: String, name: String },

    /// A container action was performed via a `DockerTrackerCommand`.
    ContainerActionResult {
        id: String,
        name: String,
        action: super::container::ContainerAction,
        success: bool,
    },
}

impl From<&DockerTrackerEvent> for crate::events::EventPayload {
    fn from(event: &DockerTrackerEvent) -> Self {
        let (event_name, data) = match event {
            DockerTrackerEvent::InitialSnapshot { containers } => (
                "docker.initial_snapshot",
                json!({
                    "container_count": containers.len(),
                    "containers": containers,
                }),
            ),
            DockerTrackerEvent::ContainersAppeared { containers } => (
                "docker.containers_appeared",
                json!({
                    "container_count": containers.len(),
                    "containers": containers,
                }),
            ),
            DockerTrackerEvent::ContainersDisappeared { containers } => (
                "docker.containers_disappeared",
                json!({
                    "container_count": containers.len(),
                    "containers": containers,
                }),
            ),
            DockerTrackerEvent::ContainerStatusChanged {
                container,
                previous,
            } => (
                "docker.container_status_changed",
                json!({
                    "id": container.id,
                    "name": container.name,
                    "image": container.image,
                    "status": container.status,
                    "previous_status": previous,
                }),
            ),
            DockerTrackerEvent::ContainerHealthChanged {
                container,
                previous,
            } => (
                "docker.container_health_changed",
                json!({
                    "id": container.id,
                    "name": container.name,
                    "image": container.image,
                    "health": container.health,
                    "previous_health": previous,
                }),
            ),
            DockerTrackerEvent::ContainerOomKilled { id, name } => (
                "docker.container_oom_killed",
                json!({
                    "id": id,
                    "name": name,
                }),
            ),
            DockerTrackerEvent::ContainerActionResult {
                id,
                name,
                action,
                success,
            } => (
                "docker.container_action_result",
                json!({
                    "id": id,
                    "name": name,
                    "action": action,
                    "success": success,
                }),
            ),
        };
        Self::new(crate::events::EventSource::DockerTracker, event_name, data)
    }
}
