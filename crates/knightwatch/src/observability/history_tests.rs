#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use serde_json::{Value, json};
use std::{io, path::Path};

use kw_types::event::{EventPayload, EventSource};

use super::history::{date_part, extract_log_date, log_event, log_file_name, prune_old_event_logs};

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

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// `YYYY-MM-DD` for "now minus `days`" (negative gives a future date), in UTC.
fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

fn touch(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), "{}\n").unwrap();
}

fn log_name(date: &str) -> String {
    format!("knightwatch-events-{date}.log")
}

// ---------------------------------------------------------------------------
// date_part
// ---------------------------------------------------------------------------

#[test]
fn date_part_takes_first_ten_chars_of_rfc3339() {
    assert_eq!(date_part("2024-05-17T10:20:30Z"), "2024-05-17");
    assert_eq!(date_part("2024-05-17T10:20:30.123456+03:30"), "2024-05-17");
}

#[test]
fn date_part_of_exactly_ten_chars_is_unchanged() {
    assert_eq!(date_part("2024-05-17"), "2024-05-17");
}

#[test]
fn date_part_of_short_string_returns_whole_string() {
    assert_eq!(date_part("2024-05"), "2024-05");
    assert_eq!(date_part("x"), "x");
}

#[test]
fn date_part_of_empty_string_is_empty() {
    assert_eq!(date_part(""), "");
}

#[test]
fn date_part_does_not_panic_inside_a_multibyte_char() {
    // 9 ASCII bytes then a 3-byte char: byte 10 is not a char boundary, so
    // `get(0..10)` is None and the whole string comes back.
    let s = "aaaaaaaaa\u{20ac}";
    assert_eq!(date_part(s), s);
}

#[test]
fn date_part_does_not_validate_that_it_is_a_date() {
    // Pinned: any 10 leading chars are taken as the "date".
    assert_eq!(date_part("not-a-date-at-all"), "not-a-date-");
}

// ---------------------------------------------------------------------------
// log_file_name / extract_log_date
// ---------------------------------------------------------------------------

#[test]
fn log_file_name_has_prefix_date_and_suffix() {
    assert_eq!(
        log_file_name("2024-05-17"),
        "knightwatch-events-2024-05-17.log"
    );
}

#[test]
fn log_file_name_with_empty_date_has_no_date() {
    // Pinned (finding 34).
    assert_eq!(log_file_name(""), "knightwatch-events-.log");
}

#[test]
fn extract_log_date_round_trips_log_file_name() {
    for date in ["2024-05-17", "1999-12-31", "2100-01-01"] {
        assert_eq!(
            extract_log_date(&log_file_name(date)).as_deref(),
            Some(date)
        );
    }
}

#[test]
fn extract_log_date_rejects_wrong_prefix() {
    assert_eq!(extract_log_date("events-2024-05-17.log"), None);
    assert_eq!(extract_log_date("other-2024-05-17.log"), None);
    assert_eq!(extract_log_date("2024-05-17.log"), None);
}

#[test]
fn extract_log_date_rejects_wrong_suffix() {
    assert_eq!(extract_log_date("knightwatch-events-2024-05-17"), None);
    assert_eq!(
        extract_log_date("knightwatch-events-2024-05-17.log.bak"),
        None
    );
    assert_eq!(extract_log_date("knightwatch-events-2024-05-17.txt"), None);
}

#[test]
fn extract_log_date_is_case_sensitive() {
    assert_eq!(extract_log_date("Knightwatch-Events-2024-05-17.log"), None);
    assert_eq!(extract_log_date("knightwatch-events-2024-05-17.LOG"), None);
}

#[test]
fn extract_log_date_does_not_validate_the_date() {
    // Pinned: whatever sits between prefix and suffix is returned, even an
    // empty string or text that is not a date.
    assert_eq!(
        extract_log_date("knightwatch-events-.log").as_deref(),
        Some("")
    );
    assert_eq!(
        extract_log_date("knightwatch-events-garbage.log").as_deref(),
        Some("garbage")
    );
}

#[test]
fn extract_log_date_of_unrelated_names_is_none() {
    assert_eq!(extract_log_date(""), None);
    assert_eq!(extract_log_date("notes.txt"), None);
    assert_eq!(extract_log_date("knightwatch-events-"), None);
}

// ---------------------------------------------------------------------------
// log_event
// ---------------------------------------------------------------------------

#[tokio::test]
async fn log_event_creates_file_named_after_the_timestamp_date() {
    let dir = tempfile::tempdir().unwrap();
    let p = payload(
        "docker.start",
        "2024-05-17T10:00:00Z",
        EventSource::DockerTracker,
    );
    log_event(dir.path(), &p).await.unwrap();

    assert!(
        dir.path()
            .join("knightwatch-events-2024-05-17.log")
            .is_file()
    );
}

#[tokio::test]
async fn log_event_writes_one_json_line_ending_with_newline() {
    let dir = tempfile::tempdir().unwrap();
    let p = payload(
        "docker.start",
        "2024-05-17T10:00:00Z",
        EventSource::DockerTracker,
    );
    log_event(dir.path(), &p).await.unwrap();

    let path = dir.path().join("knightwatch-events-2024-05-17.log");
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.ends_with('\n'));
    assert_eq!(raw.matches('\n').count(), 1);
    assert!(serde_json::from_str::<Value>(raw.trim_end()).is_ok());
}

#[tokio::test]
async fn log_event_line_holds_the_stored_event_fields() {
    let dir = tempfile::tempdir().unwrap();
    let p = payload(
        "systemd.failed",
        "2024-05-17T10:00:00Z",
        EventSource::Systemd,
    );
    log_event(dir.path(), &p).await.unwrap();

    let lines = read_lines(&dir.path().join("knightwatch-events-2024-05-17.log"));
    assert_eq!(lines.len(), 1);
    let v: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v["event"], "systemd.failed");
    assert_eq!(v["timestamp"], "2024-05-17T10:00:00Z");
    assert_eq!(v["data"], json!({ "name": "systemd.failed" }));
}

#[tokio::test]
async fn log_event_stores_the_source_even_though_payload_skips_it() {
    // `EventPayload.source` is #[serde(skip)], but `StoredEvent.source` is
    // serialized, so the log file is where the source survives.
    let dir = tempfile::tempdir().unwrap();
    for (name, source, text) in [
        ("a", EventSource::ScreenCapture, "ScreenCapture"),
        ("b", EventSource::ProcessTracker, "ProcessTracker"),
        ("c", EventSource::SystemResources, "SystemResources"),
        ("d", EventSource::Systemd, "Systemd"),
        ("e", EventSource::DockerTracker, "DockerTracker"),
    ] {
        log_event(dir.path(), &payload(name, "2024-05-17T10:00:00Z", source))
            .await
            .unwrap();
        let lines = read_lines(&dir.path().join("knightwatch-events-2024-05-17.log"));
        let last: Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        assert_eq!(last["source"], text);
    }
}

#[tokio::test]
async fn log_event_does_not_store_the_payload_version() {
    // Pinned: `StoredEvent` has no `version`, so it is dropped on disk.
    let dir = tempfile::tempdir().unwrap();
    let p = payload("x", "2024-05-17T10:00:00Z", EventSource::ScreenCapture);
    log_event(dir.path(), &p).await.unwrap();

    let lines = read_lines(&dir.path().join("knightwatch-events-2024-05-17.log"));
    let v: Value = serde_json::from_str(&lines[0]).unwrap();
    assert!(v.get("version").is_none());
    assert_eq!(v.as_object().unwrap().len(), 4);
}

#[tokio::test]
async fn log_event_appends_to_the_same_day_in_order() {
    let dir = tempfile::tempdir().unwrap();
    for (i, hour) in ["09", "10", "11"].iter().enumerate() {
        let p = payload(
            &format!("e{i}"),
            &format!("2024-05-17T{hour}:00:00Z"),
            EventSource::ProcessTracker,
        );
        log_event(dir.path(), &p).await.unwrap();
    }

    let lines = read_lines(&dir.path().join("knightwatch-events-2024-05-17.log"));
    assert_eq!(lines.len(), 3);
    for (i, line) in lines.iter().enumerate() {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["event"], format!("e{i}"));
    }
}

#[tokio::test]
async fn log_event_does_not_overwrite_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knightwatch-events-2024-05-17.log");
    std::fs::write(&path, "{\"old\":true}\n").unwrap();

    let p = payload("new", "2024-05-17T10:00:00Z", EventSource::Systemd);
    log_event(dir.path(), &p).await.unwrap();

    let lines = read_lines(&path);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "{\"old\":true}");
}

#[tokio::test]
async fn log_event_puts_different_days_in_different_files() {
    let dir = tempfile::tempdir().unwrap();
    log_event(
        dir.path(),
        &payload("a", "2024-05-17T23:59:59Z", EventSource::Systemd),
    )
    .await
    .unwrap();
    log_event(
        dir.path(),
        &payload("b", "2024-05-18T00:00:00Z", EventSource::Systemd),
    )
    .await
    .unwrap();

    assert_eq!(
        read_lines(&dir.path().join(log_name("2024-05-17"))).len(),
        1
    );
    assert_eq!(
        read_lines(&dir.path().join(log_name("2024-05-18"))).len(),
        1
    );
}

#[tokio::test]
async fn log_event_names_the_file_after_the_timestamps_own_offset() {
    // Pinned (finding 33): the date is the first 10 chars as written, not the
    // UTC date. 23:30 at -05:00 is already the next day in UTC, but the event
    // goes in the file for the local date.
    let dir = tempfile::tempdir().unwrap();
    log_event(
        dir.path(),
        &payload("a", "2024-05-17T23:30:00-05:00", EventSource::Systemd),
    )
    .await
    .unwrap();

    assert!(dir.path().join(log_name("2024-05-17")).is_file());
    assert!(!dir.path().join(log_name("2024-05-18")).exists());
}

#[tokio::test]
async fn log_event_keeps_newlines_in_data_on_a_single_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = payload("x", "2024-05-17T10:00:00Z", EventSource::ScreenCapture);
    p.data = json!({ "text": "line one\nline two\r\nline three" });
    log_event(dir.path(), &p).await.unwrap();

    let lines = read_lines(&dir.path().join(log_name("2024-05-17")));
    assert_eq!(lines.len(), 1);
    let v: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(v["data"]["text"], "line one\nline two\r\nline three");
}

#[tokio::test]
async fn log_event_does_not_filter_ticks() {
    // The tracer loop skips ticks before calling `log_event`; `log_event`
    // itself writes whatever it is given.
    let dir = tempfile::tempdir().unwrap();
    let p = payload(
        "resources.tick",
        "2024-05-17T10:00:00Z",
        EventSource::SystemResources,
    );
    assert!(p.is_tick());
    log_event(dir.path(), &p).await.unwrap();

    assert_eq!(
        read_lines(&dir.path().join(log_name("2024-05-17"))).len(),
        1
    );
}

#[tokio::test]
async fn log_event_fails_when_the_directory_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    let p = payload("x", "2024-05-17T10:00:00Z", EventSource::ScreenCapture);

    let err = log_event(&missing, &p).await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

#[tokio::test]
async fn log_event_with_short_timestamp_uses_it_whole_as_the_date() {
    // Pinned (finding 34).
    let dir = tempfile::tempdir().unwrap();
    log_event(dir.path(), &payload("a", "x", EventSource::ScreenCapture))
        .await
        .unwrap();

    assert!(dir.path().join("knightwatch-events-x.log").is_file());
}

#[tokio::test]
async fn log_event_with_empty_timestamp_writes_a_file_with_no_date() {
    // Pinned (finding 34): the file is `knightwatch-events-.log`, which
    // `prune_old_event_logs` will delete (see below).
    let dir = tempfile::tempdir().unwrap();
    log_event(dir.path(), &payload("a", "", EventSource::ScreenCapture))
        .await
        .unwrap();

    assert!(dir.path().join("knightwatch-events-.log").is_file());
}

// ---------------------------------------------------------------------------
// prune_old_event_logs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn prune_deletes_files_older_than_retention() {
    let dir = tempfile::tempdir().unwrap();
    for age in [31, 45, 400] {
        touch(dir.path(), &log_name(&days_ago(age)));
    }

    prune_old_event_logs(dir.path()).await;

    for age in [31, 45, 400] {
        assert!(
            !dir.path().join(log_name(&days_ago(age))).exists(),
            "age {age}"
        );
    }
}

#[tokio::test]
async fn prune_keeps_recent_files_and_future_dates() {
    let dir = tempfile::tempdir().unwrap();
    for age in [29, 10, 1, 0, -1, -400] {
        touch(dir.path(), &log_name(&days_ago(age)));
    }

    prune_old_event_logs(dir.path()).await;

    for age in [29, 10, 1, 0, -1, -400] {
        assert!(
            dir.path().join(log_name(&days_ago(age))).exists(),
            "age {age}"
        );
    }
}

#[tokio::test]
async fn prune_keeps_the_file_exactly_on_the_cutoff_day() {
    // The comparison is a strict `<` on the date, so the file dated exactly
    // 30 days ago stays. (Would flake only if the test straddled UTC midnight.)
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &log_name(&days_ago(30)));

    prune_old_event_logs(dir.path()).await;

    assert!(dir.path().join(log_name(&days_ago(30))).exists());
}

#[tokio::test]
async fn prune_mixed_directory_only_removes_the_old_ones() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &log_name(&days_ago(60)));
    touch(dir.path(), &log_name(&days_ago(31)));
    touch(dir.path(), &log_name(&days_ago(5)));
    touch(dir.path(), &log_name(&days_ago(0)));

    prune_old_event_logs(dir.path()).await;

    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    let mut want = vec![log_name(&days_ago(5)), log_name(&days_ago(0))];
    want.sort();
    assert_eq!(left, want);
}

#[tokio::test]
async fn prune_ignores_files_that_do_not_match_the_log_name() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "notes.txt",
        "knightwatch-events-2000-01-01.log.bak",
        "knightwatch-events-2000-01-01.LOG",
        "other-2000-01-01.log",
        "events-2000-01-01.log",
    ] {
        touch(dir.path(), name);
    }

    prune_old_event_logs(dir.path()).await;

    for name in [
        "notes.txt",
        "knightwatch-events-2000-01-01.log.bak",
        "knightwatch-events-2000-01-01.LOG",
        "other-2000-01-01.log",
        "events-2000-01-01.log",
    ] {
        assert!(dir.path().join(name).exists(), "{name}");
    }
}

#[tokio::test]
async fn prune_does_not_look_at_file_contents_or_mtime() {
    // Only the date in the name counts: an old-named file with fresh content
    // is deleted, a new-named file with ancient content is kept.
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join(log_name(&days_ago(100)));
    let new = dir.path().join(log_name(&days_ago(1)));
    std::fs::write(&old, "{\"timestamp\":\"2999-01-01T00:00:00Z\"}\n").unwrap();
    std::fs::write(&new, "{\"timestamp\":\"1999-01-01T00:00:00Z\"}\n").unwrap();

    prune_old_event_logs(dir.path()).await;

    assert!(!old.exists());
    assert!(new.exists());
}

#[tokio::test]
async fn prune_on_a_missing_directory_returns_quietly() {
    let dir = tempfile::tempdir().unwrap();
    prune_old_event_logs(&dir.path().join("does-not-exist")).await;
}

#[tokio::test]
async fn prune_on_an_empty_directory_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    prune_old_event_logs(dir.path()).await;
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn prune_survives_an_old_named_directory_and_still_prunes_the_rest() {
    // `remove_file` on a directory fails; the error is logged and the loop
    // carries on. The directory is left alone.
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join(log_name("2000-01-01"));
    std::fs::create_dir(&fake).unwrap();
    touch(dir.path(), &log_name(&days_ago(90)));
    touch(dir.path(), &log_name(&days_ago(2)));

    prune_old_event_logs(dir.path()).await;

    assert!(fake.is_dir());
    assert!(!dir.path().join(log_name(&days_ago(90))).exists());
    assert!(dir.path().join(log_name(&days_ago(2))).exists());
}

#[tokio::test]
async fn prune_deletes_the_file_with_an_empty_date() {
    // Pinned (finding 34): "" sorts before any cutoff, so a log written for an
    // event with an empty timestamp is removed on the next prune.
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "knightwatch-events-.log");

    prune_old_event_logs(dir.path()).await;

    assert!(!dir.path().join("knightwatch-events-.log").exists());
}

#[tokio::test]
async fn prune_keeps_a_file_whose_date_text_sorts_after_the_cutoff() {
    // Pinned: dates are compared as plain strings with no validation, so
    // "garbage" ('g' > '2') is never pruned.
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "knightwatch-events-garbage.log");

    prune_old_event_logs(dir.path()).await;

    assert!(dir.path().join("knightwatch-events-garbage.log").exists());
}

#[tokio::test]
async fn prune_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &log_name(&days_ago(90)));
    touch(dir.path(), &log_name(&days_ago(3)));

    prune_old_event_logs(dir.path()).await;
    prune_old_event_logs(dir.path()).await;

    assert!(!dir.path().join(log_name(&days_ago(90))).exists());
    assert!(dir.path().join(log_name(&days_ago(3))).exists());
}

// ---------------------------------------------------------------------------
// log_event and prune together
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_event_logged_with_an_old_timestamp_is_pruned_right_away() {
    // Pinned: retention is by the event's own date, so back-dated events
    // vanish on the next prune.
    let dir = tempfile::tempdir().unwrap();
    let old = format!("{}T10:00:00Z", days_ago(60));
    let fresh = format!("{}T10:00:00Z", days_ago(1));
    log_event(dir.path(), &payload("old", &old, EventSource::Systemd))
        .await
        .unwrap();
    log_event(dir.path(), &payload("fresh", &fresh, EventSource::Systemd))
        .await
        .unwrap();

    prune_old_event_logs(dir.path()).await;

    assert!(!dir.path().join(log_name(&days_ago(60))).exists());
    assert!(dir.path().join(log_name(&days_ago(1))).exists());
}
