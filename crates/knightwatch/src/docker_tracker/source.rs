use bollard::Docker;
use futures::{Stream, StreamExt, future::BoxFuture};
use std::{collections::HashMap, future::Future, pin::pin};
use tokio::sync::mpsc;

use kw_types::docker::{ContainerSnapshot, ContainerStats};

use crate::prelude::*;

/// What the tracker needs from Docker, so its diff logic can run on a fake in tests.
pub trait DockerSource: Send + Sync {
    /// Every container in any state, without stats.
    fn list(&self) -> impl Future<Output = Result<Vec<ContainerSnapshot>>> + Send;
    /// `None` if the stats could not be read.
    fn stats(&self, id: &str) -> impl Future<Output = Option<ContainerStats>> + Send;
    fn stop(&self, id: &str, timeout_secs: Option<i32>) -> impl Future<Output = Result<()>> + Send;
    fn kill(&self, id: &str, signal: Option<&str>) -> impl Future<Output = Result<()>> + Send;
    fn start(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    fn restart(
        &self,
        id: &str,
        timeout_secs: Option<i32>,
    ) -> impl Future<Output = Result<()>> + Send;
    fn pause(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    fn unpause(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    /// A task that forwards `(id, name)` of each OOM-killed container to `oom_tx`.
    fn oom_listener(&self, oom_tx: mpsc::Sender<(String, String)>) -> BoxFuture<'static, ()>;
}

pub struct BollardSource(pub Docker);

impl DockerSource for BollardSource {
    async fn list(&self) -> Result<Vec<ContainerSnapshot>> {
        let mut filters = HashMap::new();
        filters.insert(
            "status".to_string(),
            vec![
                "created".to_string(),
                "removing".to_string(),
                "running".to_string(),
                "paused".to_string(),
                "restarting".to_string(),
                "exited".to_string(),
                "dead".to_string(),
            ],
        );

        let options = bollard::query_parameters::ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();

        let summaries = self
            .0
            .list_containers(Some(options))
            .await
            .map_err(|err| Error::bollard_error(&err))?;

        Ok(summaries
            .iter()
            .filter_map(ContainerSnapshot::from_summary)
            .collect())
    }

    async fn stats(&self, id: &str) -> Option<ContainerStats> {
        let options = bollard::query_parameters::StatsOptionsBuilder::default()
            .stream(false)
            .one_shot(true)
            .build();
        let stats = self.0.stats(id, Some(options)).next().await?.ok()?;
        Some(ContainerStats::from_bollard(&stats))
    }

    async fn stop(&self, id: &str, timeout_secs: Option<i32>) -> Result<()> {
        let opts = timeout_secs.map(|t| {
            bollard::query_parameters::StopContainerOptionsBuilder::default()
                .t(t)
                .build()
        });
        self.0
            .stop_container(id, opts)
            .await
            .map_err(|err| Error::bollard_error(&err))
    }

    async fn kill(&self, id: &str, signal: Option<&str>) -> Result<()> {
        let opts = signal.map(|s| {
            bollard::query_parameters::KillContainerOptionsBuilder::default()
                .signal(s)
                .build()
        });
        self.0
            .kill_container(id, opts)
            .await
            .map_err(|err| Error::bollard_error(&err))
    }

    async fn start(&self, id: &str) -> Result<()> {
        self.0
            .start_container(id, None)
            .await
            .map_err(|err| Error::bollard_error(&err))
    }

    async fn restart(&self, id: &str, timeout_secs: Option<i32>) -> Result<()> {
        let opts = timeout_secs.map(|t| {
            bollard::query_parameters::RestartContainerOptionsBuilder::default()
                .t(t)
                .build()
        });
        self.0
            .restart_container(id, opts)
            .await
            .map_err(|err| Error::bollard_error(&err))
    }

    async fn pause(&self, id: &str) -> Result<()> {
        self.0
            .pause_container(id)
            .await
            .map_err(|err| Error::bollard_error(&err))
    }

    async fn unpause(&self, id: &str) -> Result<()> {
        self.0
            .unpause_container(id)
            .await
            .map_err(|err| Error::bollard_error(&err))
    }

    fn oom_listener(&self, oom_tx: mpsc::Sender<(String, String)>) -> BoxFuture<'static, ()> {
        Box::pin(docker_event_listener(self.0.clone(), oom_tx))
    }
}

// ============================================================================
// Docker event stream listener (OOM events only)
// ============================================================================

async fn docker_event_listener(docker: Docker, oom_tx: mpsc::Sender<(String, String)>) {
    let mut filters = HashMap::new();
    filters.insert("type".to_string(), vec!["container".to_string()]);
    filters.insert("event".to_string(), vec!["oom".to_string()]);

    let opts = bollard::query_parameters::EventsOptionsBuilder::default()
        .filters(&filters)
        .build();

    forward_oom_events(docker.events(Some(opts)), oom_tx).await;
}

pub async fn forward_oom_events(
    stream: impl Stream<
        Item = std::result::Result<bollard::models::EventMessage, bollard::errors::Error>,
    >,
    oom_tx: mpsc::Sender<(String, String)>,
) {
    let mut stream = pin!(stream);

    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(event) => {
                let id = event
                    .actor
                    .as_ref()
                    .and_then(|a| a.id.clone())
                    .unwrap_or_default();
                let name = event
                    .actor
                    .as_ref()
                    .and_then(|a| a.attributes.as_ref())
                    .and_then(|attrs| attrs.get("name"))
                    .cloned()
                    .unwrap_or_else(|| id.chars().take(12).collect());

                if oom_tx.send((id, name)).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                error!(?e, "docker event stream error; reconnecting in 5 s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                break;
            }
        }
    }
}
