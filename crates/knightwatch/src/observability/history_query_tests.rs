#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use serde_json::{Value, json};
use std::path::Path;

use kw_types::{
    event::{EventSource, StoredEvent},
    history::HistoryQuery,
};

use super::history::query_history_in;
use crate::errors::Error;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn ev(day: u32, hour: u32) -> StoredEvent {
    ev_src(day, hour, EventSource::ScreenCapture)
}

fn ev_src(day: u32, hour: u32, source: EventSource) -> StoredEvent {
    StoredEvent {
        event: format!("d{day}h{hour}"),
        timestamp: format!("2024-05-{day:02}T{hour:02}:00:00Z"),
        source,
        data: json!({ "day": day, "hour": hour, "nested": { "ok": true } }),
    }
}

fn day_file(day: u32) -> String {
    format!("knightwatch-events-2024-05-{day:02}.log")
}

fn put(dir: &Path, day: u32, events: &[StoredEvent]) {
    let mut text = String::new();
    for e in events {
        text.push_str(&serde_json::to_string(e).unwrap());
        text.push('\n');
    }
    std::fs::write(dir.join(day_file(day)), text).unwrap();
}

fn three_days() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for day in 1..=3 {
        put(dir.path(), day, &[ev(day, 1), ev(day, 2)]);
    }
    dir
}

fn names(events: &[StoredEvent]) -> Vec<&str> {
    events.iter().map(|e| e.event.as_str()).collect()
}

fn json_of(events: &[StoredEvent]) -> Vec<Value> {
    events
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect()
}

async fn run(dir: &Path, query: HistoryQuery) -> Vec<StoredEvent> {
    query_history_in(dir, query).await.unwrap()
}

fn q() -> HistoryQuery {
    HistoryQuery::default()
}

// ---------------------------------------------------------------------------
// empty and missing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_directory_gives_an_empty_result() {
    let dir = tempfile::tempdir().unwrap();
    let got = run(&dir.path().join("does-not-exist"), q()).await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn empty_directory_gives_an_empty_result() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(dir.path(), q()).await.is_empty());
}

#[tokio::test]
async fn empty_log_file_gives_an_empty_result() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(day_file(1)), "").unwrap();
    assert!(run(dir.path(), q()).await.is_empty());
}

// ---------------------------------------------------------------------------
// no filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_filters_returns_everything_oldest_first() {
    let dir = three_days();
    let got = run(dir.path(), q()).await;
    assert_eq!(
        names(&got),
        ["d1h1", "d1h2", "d2h1", "d2h2", "d3h1", "d3h2"]
    );
}

#[tokio::test]
async fn files_are_ordered_by_date_not_by_creation_order() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), 3, &[ev(3, 1)]);
    put(dir.path(), 1, &[ev(1, 1)]);
    put(dir.path(), 2, &[ev(2, 1)]);

    let got = run(dir.path(), q()).await;
    assert_eq!(names(&got), ["d1h1", "d2h1", "d3h1"]);
}

#[tokio::test]
async fn events_survive_the_round_trip_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let written = vec![
        ev_src(1, 1, EventSource::ProcessTracker),
        ev_src(1, 2, EventSource::Systemd),
        ev_src(1, 3, EventSource::DockerTracker),
    ];
    put(dir.path(), 1, &written);

    let got = run(dir.path(), q()).await;
    assert_eq!(json_of(&got), json_of(&written));
}

#[tokio::test]
async fn lines_inside_a_file_keep_file_order_even_if_timestamps_disagree() {
    // Pinned: nothing sorts by timestamp; order is file name, then line order.
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), 1, &[ev(1, 5), ev(1, 2), ev(1, 9)]);

    let got = run(dir.path(), q()).await;
    assert_eq!(names(&got), ["d1h5", "d1h2", "d1h9"]);
}

#[tokio::test]
async fn files_that_are_not_event_logs_are_ignored() {
    let dir = three_days();
    let line = serde_json::to_string(&ev(9, 9)).unwrap();
    for name in [
        "notes.txt",
        "knightwatch-events-2024-05-04.log.bak",
        "other-2024-05-05.log",
        "knightwatch-events-2024-05-06.LOG",
    ] {
        std::fs::write(dir.path().join(name), format!("{line}\n")).unwrap();
    }

    let got = run(dir.path(), q()).await;
    assert_eq!(got.len(), 6);
    assert!(!names(&got).contains(&"d9h9"));
}

#[tokio::test]
async fn subdirectories_are_not_followed() {
    let dir = three_days();
    let sub = dir.path().join("archive");
    std::fs::create_dir(&sub).unwrap();
    put(&sub, 4, &[ev(4, 1)]);

    let got = run(dir.path(), q()).await;
    assert_eq!(got.len(), 6);
}

// ---------------------------------------------------------------------------
// bad lines
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_lines_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let good1 = serde_json::to_string(&ev(1, 1)).unwrap();
    let good2 = serde_json::to_string(&ev(1, 2)).unwrap();
    let text = format!("not json at all\n{good1}\n\n{{\"event\":\"cut off\n{good2}\n   \n");
    std::fs::write(dir.path().join(day_file(1)), text).unwrap();

    let got = run(dir.path(), q()).await;
    assert_eq!(names(&got), ["d1h1", "d1h2"]);
}

#[tokio::test]
async fn lines_with_the_wrong_shape_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let good = serde_json::to_string(&ev(1, 1)).unwrap();
    // Valid JSON, but missing fields / unknown source / not an object.
    let text = format!(
        "{{\"event\":\"x\"}}\n[1,2,3]\n42\n\"str\"\nnull\n\
         {{\"event\":\"x\",\"timestamp\":\"t\",\"source\":\"Nope\",\"data\":null}}\n{good}\n"
    );
    std::fs::write(dir.path().join(day_file(1)), text).unwrap();

    let got = run(dir.path(), q()).await;
    assert_eq!(names(&got), ["d1h1"]);
}

#[tokio::test]
async fn crlf_line_endings_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let a = serde_json::to_string(&ev(1, 1)).unwrap();
    let b = serde_json::to_string(&ev(1, 2)).unwrap();
    std::fs::write(dir.path().join(day_file(1)), format!("{a}\r\n{b}\r\n")).unwrap();

    let got = run(dir.path(), q()).await;
    assert_eq!(names(&got), ["d1h1", "d1h2"]);
}

#[tokio::test]
async fn a_last_line_without_a_trailing_newline_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let a = serde_json::to_string(&ev(1, 1)).unwrap();
    std::fs::write(dir.path().join(day_file(1)), a).unwrap();

    let got = run(dir.path(), q()).await;
    assert_eq!(names(&got), ["d1h1"]);
}

// ---------------------------------------------------------------------------
// since / until
// ---------------------------------------------------------------------------

#[tokio::test]
async fn since_a_later_day_skips_earlier_files() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1", "d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn since_inside_a_day_filters_events_of_that_day() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02T01:30:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn since_is_inclusive() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-03T02:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d3h2"]);
}

#[tokio::test]
async fn until_an_earlier_day_skips_later_files() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-02T23:59:59Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1", "d1h2", "d2h1", "d2h2"]);
}

#[tokio::test]
async fn until_inside_a_day_filters_events_of_that_day() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-02T01:30:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1", "d1h2", "d2h1"]);
}

#[tokio::test]
async fn until_is_inclusive() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-01T01:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1"]);
}

#[tokio::test]
async fn since_and_until_together_pick_a_window() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-01T02:00:00Z".into()),
            until: Some("2024-05-03T01:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h2", "d2h1", "d2h2", "d3h1"]);
}

#[tokio::test]
async fn since_equal_to_until_matches_exactly_that_instant() {
    let dir = three_days();
    let at = "2024-05-02T01:00:00Z".to_owned();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some(at.clone()),
            until: Some(at),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1"]);
}

#[tokio::test]
async fn since_after_everything_gives_nothing() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2030-01-01T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn until_before_everything_gives_nothing() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2000-01-01T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn since_after_until_gives_nothing() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-03T00:00:00Z".into()),
            until: Some("2024-05-01T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn date_only_bounds_work_on_the_file_level() {
    // A bare date as `since` skips earlier files; inside the kept file the
    // string compare "2024-05-02T01:00:00Z" >= "2024-05-02" holds for every
    // event of that day.
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1", "d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn date_only_until_drops_the_events_of_that_day() {
    // Pinned: "2024-05-02T01:00:00Z" <= "2024-05-02" is false (the longer
    // string sorts after), so a bare-date `until` keeps the file for that day
    // but none of its events. Callers must send a full timestamp.
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-02".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1", "d1h2"]);
}

#[tokio::test]
async fn an_event_dated_after_its_file_is_still_filtered_by_its_own_timestamp() {
    // A stray line whose timestamp belongs to another day is judged by its
    // timestamp, not by the file it sits in.
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), 1, &[ev(1, 1), ev(3, 1)]);

    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-02T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1"]);
}

#[tokio::test]
async fn bounds_in_another_utc_offset_can_drop_a_whole_file() {
    // Pinned (finding 33). The event below is 2024-05-02T04:30:00Z in UTC, so
    // it is after `since`, but its own text starts with 2024-05-01, so it is
    // in the 05-01 file, and the file selection (date part of `since` is
    // 05-02) drops that file before the event is looked at.
    let dir = tempfile::tempdir().unwrap();
    let mut e = ev(1, 1);
    e.event = "late-in-new-york".into();
    e.timestamp = "2024-05-01T23:30:00-05:00".into();
    put(dir.path(), 1, &[e]);

    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

// ---------------------------------------------------------------------------
// event / source filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn event_filter_is_an_exact_match() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            event: Some("d2h2".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h2"]);
}

#[tokio::test]
async fn event_filter_is_not_a_prefix_or_substring_match() {
    let dir = three_days();
    for needle in ["d2", "d2h", "2h2", "D2H2", ""] {
        let got = run(
            dir.path(),
            HistoryQuery {
                event: Some(needle.into()),
                ..q()
            },
        )
        .await;
        assert!(got.is_empty(), "needle {needle:?}");
    }
}

#[tokio::test]
async fn event_filter_can_match_across_several_files() {
    let dir = tempfile::tempdir().unwrap();
    for day in 1..=3 {
        let mut a = ev(day, 1);
        a.event = "docker.die".into();
        let b = ev(day, 2);
        put(dir.path(), day, &[a, b]);
    }

    let got = run(
        dir.path(),
        HistoryQuery {
            event: Some("docker.die".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(got.len(), 3);
    assert!(got.iter().all(|e| e.event == "docker.die"));
}

#[tokio::test]
async fn source_filter_keeps_only_that_source() {
    let dir = tempfile::tempdir().unwrap();
    put(
        dir.path(),
        1,
        &[
            ev_src(1, 1, EventSource::ProcessTracker),
            ev_src(1, 2, EventSource::Systemd),
        ],
    );
    put(
        dir.path(),
        2,
        &[
            ev_src(2, 1, EventSource::ProcessTracker),
            ev_src(2, 2, EventSource::DockerTracker),
        ],
    );

    let got = run(
        dir.path(),
        HistoryQuery {
            source: Some(EventSource::ProcessTracker),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1", "d2h1"]);
    assert!(got.iter().all(|e| e.source == EventSource::ProcessTracker));
}

#[tokio::test]
async fn source_with_no_events_gives_nothing() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            source: Some(EventSource::Systemd),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn all_filters_combine_with_and() {
    let dir = tempfile::tempdir().unwrap();
    put(
        dir.path(),
        2,
        &[
            ev_src(2, 1, EventSource::Systemd),
            ev_src(2, 2, EventSource::Systemd),
            ev_src(2, 3, EventSource::ProcessTracker),
        ],
    );

    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02T02:00:00Z".into()),
            until: Some("2024-05-02T03:00:00Z".into()),
            event: Some("d2h2".into()),
            source: Some(EventSource::Systemd),
            limit: None,
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h2"]);
}

// ---------------------------------------------------------------------------
// limit (no `until`: newest first, early stop)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn limit_returns_the_newest_events_in_chronological_order() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(2),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d3h1", "d3h2"]);
}

#[tokio::test]
async fn limit_one_returns_the_single_newest_event() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(1),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d3h2"]);
}

#[tokio::test]
async fn limit_can_end_in_the_middle_of_an_older_file() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(3),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn limit_spanning_several_files() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(5),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h2", "d2h1", "d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn limit_equal_to_the_total_returns_everything() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(6),
            ..q()
        },
    )
    .await;
    assert_eq!(
        names(&got),
        ["d1h1", "d1h2", "d2h1", "d2h2", "d3h1", "d3h2"]
    );
}

#[tokio::test]
async fn limit_larger_than_the_total_returns_everything_in_order() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(1000),
            ..q()
        },
    )
    .await;
    assert_eq!(
        names(&got),
        ["d1h1", "d1h2", "d2h1", "d2h2", "d3h1", "d3h2"]
    );
}

#[tokio::test]
async fn limit_zero_returns_nothing() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(0),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn limit_with_other_filters_counts_only_matches() {
    let dir = tempfile::tempdir().unwrap();
    for day in 1..=3 {
        put(
            dir.path(),
            day,
            &[
                ev_src(day, 1, EventSource::ProcessTracker),
                ev_src(day, 2, EventSource::Systemd),
            ],
        );
    }

    let got = run(
        dir.path(),
        HistoryQuery {
            source: Some(EventSource::ProcessTracker),
            limit: Some(2),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1", "d3h1"]);
}

#[tokio::test]
async fn limit_with_since_stays_inside_the_window() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02T00:00:00Z".into()),
            limit: Some(10),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1", "d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn limit_stops_reading_older_files_once_it_is_met() {
    // The oldest file is not valid UTF-8, which would make a full read fail.
    // With a limit and no `until` the newest file already fills the limit, so
    // that file is never opened.
    let dir = three_days();
    std::fs::write(dir.path().join(day_file(1)), [0xff_u8, 0xfe, b'\n']).unwrap();

    let got = run(
        dir.path(),
        HistoryQuery {
            limit: Some(2),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d3h1", "d3h2"]);
}

#[tokio::test]
async fn limit_still_reads_older_files_when_the_newest_are_short() {
    let dir = three_days();
    std::fs::write(dir.path().join(day_file(1)), [0xff_u8, 0xfe, b'\n']).unwrap();

    // 5 wanted, only 4 events are in the files before the bad one, so the bad
    // file is reached (pinned together with finding 32).
    let err = query_history_in(
        dir.path(),
        HistoryQuery {
            limit: Some(5),
            ..q()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Other(_)), "{err:?}");
}

// ---------------------------------------------------------------------------
// limit with `until` (oldest first, trim from the front)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn limit_with_until_returns_the_last_events_before_the_bound() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-02T23:59:59Z".into()),
            limit: Some(2),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1", "d2h2"]);
}

#[tokio::test]
async fn limit_with_until_can_span_files() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-03T23:59:59Z".into()),
            limit: Some(3),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn limit_with_until_larger_than_the_matches_returns_all_in_order() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-02T01:00:00Z".into()),
            limit: Some(50),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d1h1", "d1h2", "d2h1"]);
}

#[tokio::test]
async fn limit_with_until_and_since_returns_the_newest_of_the_window() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-01T02:00:00Z".into()),
            until: Some("2024-05-03T01:00:00Z".into()),
            limit: Some(2),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h2", "d3h1"]);
}

#[tokio::test]
async fn limit_zero_with_until_returns_nothing() {
    let dir = three_days();
    let got = run(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-03T23:59:59Z".into()),
            limit: Some(0),
            ..q()
        },
    )
    .await;
    assert!(got.is_empty());
}

#[tokio::test]
async fn limit_with_until_reads_every_file_in_range() {
    // No early stop on this path, so an unreadable file inside the range
    // fails the query even though the limit could be met without it
    // (pinned together with finding 32).
    let dir = three_days();
    std::fs::write(dir.path().join(day_file(1)), [0xff_u8, 0xfe, b'\n']).unwrap();

    let err = query_history_in(
        dir.path(),
        HistoryQuery {
            until: Some("2024-05-03T23:59:59Z".into()),
            limit: Some(2),
            ..q()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Other(_)), "{err:?}");
}

// ---------------------------------------------------------------------------
// unreadable files (finding 32)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_file_that_is_not_utf8_fails_the_whole_query() {
    // Pinned (finding 32).
    let dir = three_days();
    std::fs::write(dir.path().join(day_file(2)), [0xff_u8, 0xfe, b'\n']).unwrap();

    let err = query_history_in(dir.path(), q()).await.unwrap_err();
    match err {
        Error::Other(m) => assert!(
            m.contains("Failed to read knightwatch-events-2024-05-02.log"),
            "{m}"
        ),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "finding 32: one unreadable (non-UTF-8) log file fails the whole history query; it should be skipped with a log line"]
async fn wanted_a_non_utf8_file_is_skipped() {
    let dir = three_days();
    std::fs::write(dir.path().join(day_file(2)), [0xff_u8, 0xfe, b'\n']).unwrap();

    let got = query_history_in(dir.path(), q()).await.unwrap();
    assert_eq!(names(&got), ["d1h1", "d1h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn an_unreadable_file_outside_the_date_range_is_never_opened() {
    let dir = three_days();
    std::fs::write(dir.path().join(day_file(1)), [0xff_u8, 0xfe, b'\n']).unwrap();

    let got = run(
        dir.path(),
        HistoryQuery {
            since: Some("2024-05-02T00:00:00Z".into()),
            ..q()
        },
    )
    .await;
    assert_eq!(names(&got), ["d2h1", "d2h2", "d3h1", "d3h2"]);
}

#[tokio::test]
async fn a_directory_named_like_a_log_file_fails_the_query() {
    // Pinned: `read_to_string` on a directory is an error that is not
    // NotFound, so it is reported rather than skipped.
    let dir = three_days();
    let fake = dir.path().join("knightwatch-events-2024-05-04.log");
    std::fs::create_dir(&fake).unwrap();

    let err = query_history_in(dir.path(), q()).await.unwrap_err();
    assert!(matches!(err, Error::Other(_)), "{err:?}");
}
