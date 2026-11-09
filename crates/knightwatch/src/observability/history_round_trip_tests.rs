#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use serde_json::json;
use std::{path::Path, time::Duration};

use kw_types::{
    event::{EventPayload, EventSource, StoredEvent},
    history::HistoryQuery,
};

use super::history::{log_event, prune_old_event_logs, query_history_in};
use crate::utils::now_rfc3339;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn payload(event: &str, timestamp: &str, source: EventSource) -> EventPayload {
    EventPayload {
        version: "1".to_owned(),
        source,
        event: event.to_owned(),
        timestamp: timestamp.to_owned(),
        data: json!({ "name": event }),
    }
}

async fn log(dir: &Path, event: &str, timestamp: &str) {
    log_event(dir, &payload(event, timestamp, EventSource::Systemd))
        .await
        .unwrap();
}

async fn run(dir: &Path, query: HistoryQuery) -> Vec<StoredEvent> {
    query_history_in(dir, query).await.unwrap()
}

fn names(events: &[StoredEvent]) -> Vec<&str> {
    events.iter().map(|e| e.event.as_str()).collect()
}

/// `YYYY-MM-DDT12:00:00Z` for "now minus `days`", in UTC.
fn noon_days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT12:00:00Z")
        .to_string()
}

// ---------------------------------------------------------------------------
// round trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_logged_event_reads_back_with_its_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = payload(
        "docker.oom",
        "2024-05-01T10:00:00Z",
        EventSource::DockerTracker,
    );
    p.data = json!({ "text": "multi\nline ✓ \"quoted\"", "n": [1, 2.5, null] });
    log_event(dir.path(), &p).await.unwrap();

    let got = run(dir.path(), HistoryQuery::default()).await;
    assert_eq!(got.len(), 1);
    // `source` is skipped on the payload's wire form but kept on disk.
    assert_eq!(got[0].source, EventSource::DockerTracker);
    assert_eq!(
        serde_json::to_value(&got[0]).unwrap(),
        serde_json::to_value(StoredEvent::from(&p)).unwrap()
    );
}

#[tokio::test]
async fn events_over_several_days_come_back_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    // Logged out of order on purpose.
    log(dir.path(), "c", "2024-05-03T01:00:00Z").await;
    log(dir.path(), "a", "2024-05-01T01:00:00Z").await;
    log(dir.path(), "b", "2024-05-02T01:00:00Z").await;
    log(dir.path(), "a2", "2024-05-01T02:00:00Z").await;

    let got = run(dir.path(), HistoryQuery::default()).await;
    assert_eq!(names(&got), ["a", "a2", "b", "c"]);
}

#[tokio::test]
async fn the_source_filter_works_on_logged_events() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        ("s", EventSource::ScreenCapture),
        ("p", EventSource::ProcessTracker),
        ("r", EventSource::SystemResources),
        ("d", EventSource::DockerTracker),
    ] {
        log_event(dir.path(), &payload(name, "2024-05-01T01:00:00Z", source))
            .await
            .unwrap();
    }
    let query = HistoryQuery {
        source: Some(EventSource::ProcessTracker),
        ..HistoryQuery::default()
    };
    assert_eq!(names(&run(dir.path(), query).await), ["p"]);
}

#[tokio::test]
async fn limit_returns_the_newest_logged_events() {
    let dir = tempfile::tempdir().unwrap();
    for day in 1..=3 {
        for hour in 1..=2 {
            let ts = format!("2024-05-{day:02}T{hour:02}:00:00Z");
            log(dir.path(), &format!("d{day}h{hour}"), &ts).await;
        }
    }
    let query = HistoryQuery {
        limit: Some(3),
        ..HistoryQuery::default()
    };
    assert_eq!(
        names(&run(dir.path(), query).await),
        ["d2h2", "d3h1", "d3h2"]
    );
}

#[tokio::test]
async fn since_and_until_work_with_real_now_timestamps() {
    // Finding 4: string comparison is only safe for `now_rfc3339` output.
    let dir = tempfile::tempdir().unwrap();
    let start = now_rfc3339();
    tokio::time::sleep(Duration::from_millis(5)).await;
    log(dir.path(), "before", &now_rfc3339()).await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let mark = now_rfc3339();
    tokio::time::sleep(Duration::from_millis(5)).await;
    log(dir.path(), "after", &now_rfc3339()).await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let end = now_rfc3339();

    let since = HistoryQuery {
        since: Some(mark.clone()),
        ..HistoryQuery::default()
    };
    assert_eq!(names(&run(dir.path(), since).await), ["after"]);

    let until = HistoryQuery {
        until: Some(mark),
        ..HistoryQuery::default()
    };
    assert_eq!(names(&run(dir.path(), until).await), ["before"]);

    let window = HistoryQuery {
        since: Some(start),
        until: Some(end),
        ..HistoryQuery::default()
    };
    assert_eq!(names(&run(dir.path(), window).await), ["before", "after"]);
}

#[tokio::test]
async fn concurrent_writers_do_not_tear_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut tasks = Vec::new();
    for i in 0..50 {
        let dir = dir.path().to_owned();
        tasks.push(tokio::spawn(async move {
            let mut p = payload(
                &format!("e{i}"),
                "2024-05-01T01:00:00Z",
                EventSource::Systemd,
            );
            // Big enough that a torn write would show.
            p.data = json!({ "pad": "x".repeat(8 * 1024) });
            log_event(&dir, &p).await.unwrap();
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    // A torn line would be skipped as malformed, so the count would drop.
    assert_eq!(run(dir.path(), HistoryQuery::default()).await.len(), 50);
}

#[tokio::test]
async fn pruned_days_no_longer_show_up() {
    let dir = tempfile::tempdir().unwrap();
    log(dir.path(), "old", &noon_days_ago(40)).await;
    log(dir.path(), "recent", &noon_days_ago(1)).await;
    prune_old_event_logs(dir.path()).await;
    assert_eq!(
        names(&run(dir.path(), HistoryQuery::default()).await),
        ["recent"]
    );
}

#[tokio::test]
async fn a_query_while_writing_sees_whole_lines_only() {
    let dir = tempfile::tempdir().unwrap();
    let writer = {
        let dir = dir.path().to_owned();
        tokio::spawn(async move {
            for i in 0..200 {
                log(&dir, &format!("e{i}"), "2024-05-01T01:00:00Z").await;
            }
        })
    };
    let mut last = 0;
    while !writer.is_finished() {
        let got = run(dir.path(), HistoryQuery::default()).await;
        assert!(got.len() >= last, "the count never goes down");
        // Every event read is a prefix of what was written, in order.
        for (i, e) in got.iter().enumerate() {
            assert_eq!(e.event, format!("e{i}"));
        }
        last = got.len();
        tokio::task::yield_now().await;
    }
    writer.await.unwrap();
    assert_eq!(run(dir.path(), HistoryQuery::default()).await.len(), 200);
}

// ---------------------------------------------------------------------------
// a torn last line (crash mid-write)
// ---------------------------------------------------------------------------

fn tear(dir: &Path) {
    // A line cut short with no newline, as a crash mid-write would leave it.
    std::fs::write(
        dir.join("knightwatch-events-2024-05-01.log"),
        r#"{"event":"torn","timestamp":"2024-05-01T00:"#,
    )
    .unwrap();
}

#[tokio::test]
async fn after_a_torn_line_the_next_logged_event_is_lost() {
    // Pinned (finding 69): `log_event` appends straight after the torn bytes,
    // so the new event joins the bad line and is skipped with it.
    let dir = tempfile::tempdir().unwrap();
    tear(dir.path());
    log(dir.path(), "next", "2024-05-01T01:00:00Z").await;
    log(dir.path(), "later", "2024-05-01T02:00:00Z").await;
    let got = run(dir.path(), HistoryQuery::default()).await;
    assert_eq!(names(&got), ["later"]);
}

#[tokio::test]
#[ignore = "finding 69: an event logged after a torn last line is lost with it"]
async fn wanted_an_event_after_a_torn_line_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    tear(dir.path());
    log(dir.path(), "next", "2024-05-01T01:00:00Z").await;
    let got = run(dir.path(), HistoryQuery::default()).await;
    assert_eq!(names(&got), ["next"]);
}
