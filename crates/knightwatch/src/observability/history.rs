use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io, path::Path, time::Duration};
use tokio::{fs, io::AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::{
    config::log_dir,
    events::{EventPayload, EventSource},
    prelude::*,
    utils::recv_or_pending,
};

const RETENTION_DAYS: i64 = 30;
const PRUNE_INTERVAL: Duration = Duration::from_hours(24);

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
            event: p.event.to_string(),
            timestamp: p.timestamp.clone(),
            source: p.source,
            data: p.data.clone(),
        }
    }
}

/// First 10 chars of an RFC3339 timestamp, i.e. "YYYY-MM-DD".
/// Falls back to the whole string if it's shorter than expected.
fn date_part(timestamp: &str) -> &str {
    timestamp.get(0..10).unwrap_or(timestamp)
}

fn log_file_name(date: &str) -> String {
    format!("knightwatch-events-{date}.log")
}

pub async fn log_event(dir: &Path, payload: &EventPayload) -> io::Result<()> {
    let stored: StoredEvent = payload.into();

    let mut line = serde_json::to_string(&stored)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    line.push('\n');

    let path = dir.join(log_file_name(date_part(&stored.timestamp)));

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;

    file.write_all(line.as_bytes()).await
}

/// Deletes daily log files older than `RETENTION_DAYS`.
pub async fn prune_old_event_logs(dir: &Path) {
    let Some(cutoff_dt) =
        chrono::Utc::now().checked_sub_signed(chrono::Duration::days(RETENTION_DAYS))
    else {
        error!("webhook: failed to compute log retention cutoff (overflow)");
        return;
    };
    let cutoff = cutoff_dt.format("%Y-%m-%d").to_string();

    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(e) => {
            error!("webhook: failed to read event log directory: {}", e);
            return;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(date) = extract_log_date(&name) else {
            continue;
        };
        if date.as_str() < cutoff.as_str()
            && let Err(e) = fs::remove_file(entry.path()).await
        {
            error!("webhook: failed to prune {}: {}", name, e);
        }
    }
}

/// Parses "events-YYYY-MM-DD.log" -> "YYYY-MM-DD"
fn extract_log_date(filename: &str) -> Option<String> {
    filename
        .strip_prefix("knightwatch-events-")?
        .strip_suffix(".log")
        .map(str::to_owned)
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    pub event: Option<String>,
    pub source: Option<EventSource>,
    pub limit: Option<usize>,
}

impl HistoryQuery {
    pub fn filter(&self, event: &StoredEvent) -> bool {
        self.since
            .as_ref()
            .is_none_or(|since| &event.timestamp >= since)
            && self
                .until
                .as_ref()
                .is_none_or(|until| &event.timestamp <= until)
            && self.event.as_ref().is_none_or(|ev| &event.event == ev)
            && self.source.is_none_or(|src| event.source == src)
    }
}

pub async fn query_history(query: HistoryQuery) -> Result<Vec<StoredEvent>> {
    let dir = log_dir().ok_or_else(|| Error::Other("Failed to get logs directory".into()))?;
    let mut files = match fs::read_dir(&dir).await {
        Ok(mut entries) => {
            let mut names = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str()
                    && let Some(date) = extract_log_date(name)
                {
                    names.push((name.to_owned(), date));
                }
            }
            names
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(Error::Other(format!(
                "Failed to read event log directory: {e}"
            )));
        }
    };
    let since_date = query.since.as_deref().map(date_part);
    let until_date = query.until.as_deref().map(date_part);
    // Keep only files whose date could overlap [since_date, until_date].
    files.retain(|(_, date)| {
        since_date.is_none_or(|s| date.as_str() >= s)
            && until_date.is_none_or(|u| date.as_str() <= u)
    });
    // Newest-first if we have a limit and no upper bound narrowing us to
    // an older window — lets us stop reading files early.
    let newest_first = query.limit.is_some() && query.until.is_none();
    files.sort();
    if newest_first {
        files.reverse();
    }
    let mut collected = Vec::new();
    for (name, _) in &files {
        let content = match fs::read_to_string(dir.join(name)).await {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::Other(format!("Failed to read {name}: {e}"))),
        };
        let mut matches: Vec<StoredEvent> = content
            .lines()
            .filter_map(|line| serde_json::from_str::<StoredEvent>(line).ok())
            .filter(|r| query.filter(r))
            .collect();
        if newest_first {
            matches.reverse();
        }
        collected.extend(matches);
        if let Some(limit) = query.limit
            && newest_first
            && collected.len() >= limit
        {
            collected.truncate(limit);
            break;
        }
    }
    if newest_first {
        collected.reverse();
    } else if let Some(limit) = query.limit {
        let len = collected.len();
        if len > limit {
            collected.drain(0..len.saturating_sub(limit));
        }
    }
    Ok(collected)
}

async fn event_tracer(cancel_token: CancellationToken) {
    let mut process_tracker_rx = crate::process_tracker::subscribe_events();
    let mut system_resources_rx = crate::system_resources::subscribe_events();
    let mut systemd_rx = crate::systemd::subscribe_events();
    let mut docker_tracker_rx = crate::docker_tracker::subscribe_events();
    if crate::all_none!(
        process_tracker_rx,
        system_resources_rx,
        systemd_rx,
        docker_tracker_rx
    ) {
        return;
    }
    let Some(log_path) = log_dir() else {
        error!("Failed to get logs directory");
        return;
    };

    if let Err(e) = fs::create_dir_all(&log_path).await {
        error!("event tracer: failed to create event log directory: {}", e);
        return;
    }

    prune_old_event_logs(&log_path).await;
    let mut prune_interval = tokio::time::interval(PRUNE_INTERVAL);
    prune_interval.tick().await; // consume the immediate first tick

    loop {
        let payload = tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
                info!("event tracer: dispatcher shutting down");
                return;
            }
            _ = prune_interval.tick() => {
                prune_old_event_logs(&log_path).await;
                continue;
            }
            e = recv_or_pending(&mut process_tracker_rx, "event tracer: process tracker") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut system_resources_rx, "event tracer: system resources") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut systemd_rx, "event tracer: systemd") => {
                EventPayload::from(&e)
            }
            e = recv_or_pending(&mut docker_tracker_rx, "event tracer: docker tracker") => {
                EventPayload::from(&e)
            }
        };

        if payload.is_tick() {
            continue;
        }
        if let Err(e) = log_event(&log_path, &payload).await {
            error!("event tracer: failed to write event to log file: {}", e);
        }
    }
}

pub fn init_event_tracer(cancel_token: CancellationToken) {
    tokio::spawn(event_tracer(cancel_token));
}
