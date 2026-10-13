use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, from_value, json};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Serialize -> deserialize -> serialize again; both JSON values must match.
fn assert_roundtrip<T: Serialize + DeserializeOwned>(v: &T) {
    let first = serde_json::to_value(v).unwrap();
    let back: T = serde_json::from_value(first.clone()).unwrap();
    let second = serde_json::to_value(&back).unwrap();
    assert_eq!(first, second);
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .expect("expected a JSON object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

// ---------------------------------------------------------------------------
// event.rs
// ---------------------------------------------------------------------------

mod event {
    use super::*;
    use kw_types::event::{EventPayload, EventSource, StoredEvent};

    const ALL_SOURCES: [EventSource; 5] = [
        EventSource::ScreenCapture,
        EventSource::ProcessTracker,
        EventSource::SystemResources,
        EventSource::Systemd,
        EventSource::DockerTracker,
    ];

    fn payload(source: EventSource, event: &'static str) -> EventPayload {
        EventPayload::new(
            "1".to_string(),
            source,
            event,
            "2026-01-01T00:00:00Z".to_string(),
            json!({ "k": 1 }),
        )
    }

    // ---- EventSource -------------------------------------------------------

    #[test]
    fn source_default_is_screen_capture() {
        assert_eq!(EventSource::default(), EventSource::ScreenCapture);
    }

    #[test]
    fn source_wire_names_and_roundtrip() {
        let names = [
            (EventSource::ScreenCapture, "ScreenCapture"),
            (EventSource::ProcessTracker, "ProcessTracker"),
            (EventSource::SystemResources, "SystemResources"),
            (EventSource::Systemd, "Systemd"),
            (EventSource::DockerTracker, "DockerTracker"),
        ];
        for (src, name) in names {
            assert_eq!(serde_json::to_value(src).unwrap(), json!(name));
            let back: EventSource = from_value(json!(name)).unwrap();
            assert_eq!(back, src);
        }
    }

    #[test]
    fn source_rejects_unknown_name() {
        assert!(from_value::<EventSource>(json!("Nope")).is_err());
        assert!(from_value::<EventSource>(json!("systemd")).is_err());
    }

    // ---- EventPayload ------------------------------------------------------

    #[test]
    fn new_sets_every_field() {
        let p = payload(EventSource::Systemd, "systemd.unit_failed");
        assert_eq!(p.version, "1");
        assert_eq!(p.source, EventSource::Systemd);
        assert_eq!(p.event, "systemd.unit_failed");
        assert_eq!(p.timestamp, "2026-01-01T00:00:00Z");
        assert_eq!(p.data, json!({ "k": 1 }));
    }

    #[test]
    fn predicates_are_true_for_exactly_one_source() {
        for (i, src) in ALL_SOURCES.into_iter().enumerate() {
            let p = payload(src, "x");
            let flags = [
                p.is_screen_capture(),
                p.is_process_tracker(),
                p.is_system_resources(),
                p.is_systemd(),
                p.is_docker_tracker(),
            ];
            for (j, flag) in flags.into_iter().enumerate() {
                assert_eq!(flag, i == j, "source {src:?}, predicate index {j}");
            }
        }
    }

    #[test]
    fn is_tick_matches_only_the_known_tick_events() {
        assert!(payload(EventSource::SystemResources, "resources.tick").is_tick());
        assert!(payload(EventSource::Systemd, "systemd.tick").is_tick());

        for not_tick in [
            "",
            "tick",
            "resources.started",
            "resources.tick.extra",
            "RESOURCES.TICK",
        ] {
            assert!(
                !payload(EventSource::SystemResources, not_tick).is_tick(),
                "'{not_tick}'"
            );
        }
    }

    #[test]
    fn payload_json_has_exactly_four_keys_and_no_source() {
        let v = serde_json::to_value(payload(EventSource::Systemd, "systemd.tick")).unwrap();
        assert_eq!(sorted_keys(&v), ["data", "event", "timestamp", "version"]);
    }

    #[test]
    fn payload_source_is_not_serialized_and_resets_to_default_after_roundtrip() {
        // `source` is #[serde(skip)], so a payload that crosses the wire (socket,
        // SSE, webhook) always comes back as `ScreenCapture`, and `is_systemd()`
        // etc. return the wrong answer on the receiving side.
        // If you change this (serialize `source`, or derive it from the event
        // prefix), this test should flip.
        let sent = payload(EventSource::Systemd, "systemd.tick");
        let received: EventPayload = from_value(serde_json::to_value(&sent).unwrap()).unwrap();

        assert_eq!(received.source, EventSource::ScreenCapture);
        assert!(received.is_screen_capture());
        assert!(!received.is_systemd());

        // everything else survives
        assert_eq!(received.version, sent.version);
        assert_eq!(received.event, sent.event);
        assert_eq!(received.timestamp, sent.timestamp);
        assert_eq!(received.data, sent.data);
    }

    #[test]
    fn payload_data_survives_roundtrip_for_any_json_shape() {
        let datas = [
            Value::Null,
            json!(1),
            json!("text"),
            json!([1, 2, 3]),
            json!({ "a": { "b": [true, null, 1.5] } }),
        ];
        for data in datas {
            let p = EventPayload::new(
                "1".into(),
                EventSource::ProcessTracker,
                "process.x",
                "t".into(),
                data.clone(),
            );
            let back: EventPayload = from_value(serde_json::to_value(&p).unwrap()).unwrap();
            assert_eq!(back.data, data);
        }
    }

    #[test]
    fn payload_deserialize_requires_all_four_fields() {
        let full = json!({ "version": "1", "event": "e", "timestamp": "t", "data": null });
        assert!(from_value::<EventPayload>(full.clone()).is_ok());
        for key in ["version", "event", "timestamp", "data"] {
            let mut v = full.clone();
            v.as_object_mut().unwrap().remove(key);
            // `data: Value` accepts a missing key as null? Either way the other
            // three must be required.
            if key != "data" {
                assert!(
                    from_value::<EventPayload>(v).is_err(),
                    "missing '{key}' should fail"
                );
            }
        }
    }

    // ---- StoredEvent -------------------------------------------------------

    #[test]
    fn stored_event_copies_fields_from_payload() {
        let p = payload(EventSource::DockerTracker, "docker.container_died");
        let s = StoredEvent::from(&p);
        assert_eq!(s.event, p.event);
        assert_eq!(s.timestamp, p.timestamp);
        assert_eq!(s.source, EventSource::DockerTracker);
        assert_eq!(s.data, p.data);
    }

    #[test]
    fn stored_event_json_shape_includes_source_and_drops_version() {
        let s = StoredEvent::from(&payload(EventSource::Systemd, "systemd.tick"));
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(sorted_keys(&v), ["data", "event", "source", "timestamp"]);
        assert_eq!(v["source"], json!("Systemd"));
    }

    #[test]
    fn stored_event_keeps_source_through_roundtrip() {
        for src in ALL_SOURCES {
            let s = StoredEvent::from(&payload(src, "x"));
            let back: StoredEvent = from_value(serde_json::to_value(&s).unwrap()).unwrap();
            assert_eq!(back.source, src);
            assert_eq!(back.event, s.event);
            assert_eq!(back.timestamp, s.timestamp);
            assert_eq!(back.data, s.data);
        }
    }
}

// ---------------------------------------------------------------------------
// history.rs
// ---------------------------------------------------------------------------

mod history {
    use super::*;
    use kw_types::event::{EventSource, StoredEvent};
    use kw_types::history::HistoryQuery;

    fn ev(event: &str, ts: &str, source: EventSource) -> StoredEvent {
        StoredEvent {
            event: event.to_string(),
            timestamp: ts.to_string(),
            source,
            data: Value::Null,
        }
    }

    fn base() -> StoredEvent {
        ev(
            "resources.tick",
            "2026-01-15T12:00:00Z",
            EventSource::SystemResources,
        )
    }

    #[test]
    fn default_query_matches_everything() {
        let q = HistoryQuery::default();
        assert!(q.filter(&base()));
        assert!(q.filter(&ev("", "", EventSource::ScreenCapture)));
    }

    #[test]
    fn since_is_inclusive() {
        let q = HistoryQuery {
            since: Some("2026-01-15T12:00:00Z".into()),
            ..Default::default()
        };
        assert!(
            q.filter(&ev("e", "2026-01-15T12:00:00Z", EventSource::Systemd)),
            "equal"
        );
        assert!(
            q.filter(&ev("e", "2026-01-15T12:00:01Z", EventSource::Systemd)),
            "after"
        );
        assert!(
            !q.filter(&ev("e", "2026-01-15T11:59:59Z", EventSource::Systemd)),
            "before"
        );
    }

    #[test]
    fn until_is_inclusive() {
        let q = HistoryQuery {
            until: Some("2026-01-15T12:00:00Z".into()),
            ..Default::default()
        };
        assert!(
            q.filter(&ev("e", "2026-01-15T12:00:00Z", EventSource::Systemd)),
            "equal"
        );
        assert!(
            q.filter(&ev("e", "2026-01-15T11:59:59Z", EventSource::Systemd)),
            "before"
        );
        assert!(
            !q.filter(&ev("e", "2026-01-15T12:00:01Z", EventSource::Systemd)),
            "after"
        );
    }

    #[test]
    fn since_and_until_form_a_window() {
        let q = HistoryQuery {
            since: Some("2026-01-10T00:00:00Z".into()),
            until: Some("2026-01-20T00:00:00Z".into()),
            ..Default::default()
        };
        assert!(q.filter(&ev("e", "2026-01-15T00:00:00Z", EventSource::Systemd)));
        assert!(!q.filter(&ev("e", "2026-01-09T23:59:59Z", EventSource::Systemd)));
        assert!(!q.filter(&ev("e", "2026-01-20T00:00:01Z", EventSource::Systemd)));
    }

    #[test]
    fn inverted_window_matches_nothing() {
        let q = HistoryQuery {
            since: Some("2026-02-01T00:00:00Z".into()),
            until: Some("2026-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        for ts in [
            "2025-12-31T00:00:00Z",
            "2026-01-15T00:00:00Z",
            "2026-03-01T00:00:00Z",
        ] {
            assert!(!q.filter(&ev("e", ts, EventSource::Systemd)), "{ts}");
        }
    }

    #[test]
    fn same_format_utc_timestamps_compare_correctly_across_boundaries() {
        // Works because the fields are zero-padded and fixed width.
        let q = HistoryQuery {
            since: Some("2026-01-09T23:59:59Z".into()),
            ..Default::default()
        };
        assert!(q.filter(&ev("e", "2026-01-10T00:00:00Z", EventSource::Systemd)));
        assert!(q.filter(&ev("e", "2026-02-01T00:00:00Z", EventSource::Systemd)));
        assert!(q.filter(&ev("e", "2027-01-01T00:00:00Z", EventSource::Systemd)));
        assert!(!q.filter(&ev("e", "2026-01-09T23:59:58Z", EventSource::Systemd)));
    }

    #[test]
    fn event_filter_is_exact_and_case_sensitive() {
        let q = HistoryQuery {
            event: Some("resources.tick".into()),
            ..Default::default()
        };
        assert!(q.filter(&base()));
        assert!(!q.filter(&ev("resources.tick2", "t", EventSource::SystemResources)));
        assert!(!q.filter(&ev("resources", "t", EventSource::SystemResources)));
        assert!(!q.filter(&ev("Resources.Tick", "t", EventSource::SystemResources)));
    }

    #[test]
    fn source_filter_matches_only_that_source() {
        let q = HistoryQuery {
            source: Some(EventSource::Systemd),
            ..Default::default()
        };
        assert!(q.filter(&ev("e", "t", EventSource::Systemd)));
        assert!(!q.filter(&ev("e", "t", EventSource::DockerTracker)));
        assert!(!q.filter(&ev("e", "t", EventSource::ScreenCapture)));
    }

    #[test]
    fn all_criteria_are_anded_together() {
        let q = HistoryQuery {
            since: Some("2026-01-01T00:00:00Z".into()),
            until: Some("2026-12-31T23:59:59Z".into()),
            event: Some("resources.tick".into()),
            source: Some(EventSource::SystemResources),
            limit: None,
        };
        assert!(q.filter(&base()));

        // break each criterion in turn
        let mut e = base();
        e.timestamp = "2025-01-01T00:00:00Z".into();
        assert!(!q.filter(&e), "too early");

        let mut e = base();
        e.timestamp = "2027-01-01T00:00:00Z".into();
        assert!(!q.filter(&e), "too late");

        let mut e = base();
        e.event = "other".into();
        assert!(!q.filter(&e), "wrong event");

        let mut e = base();
        e.source = EventSource::Systemd;
        assert!(!q.filter(&e), "wrong source");
    }

    #[test]
    fn filter_ignores_limit() {
        // `limit` is for the caller to apply after filtering.
        let q = HistoryQuery {
            limit: Some(0),
            ..Default::default()
        };
        assert!(q.filter(&base()));
    }

    #[test]
    fn query_deserializes_from_empty_object_and_partial_objects() {
        let empty: HistoryQuery = from_value(json!({})).unwrap();
        assert!(empty.since.is_none());
        assert!(empty.until.is_none());
        assert!(empty.event.is_none());
        assert!(empty.source.is_none());
        assert!(empty.limit.is_none());

        let q: HistoryQuery =
            from_value(json!({ "source": "Systemd", "limit": 50, "event": "systemd.tick" }))
                .unwrap();
        assert_eq!(q.source, Some(EventSource::Systemd));
        assert_eq!(q.limit, Some(50));
        assert_eq!(q.event.as_deref(), Some("systemd.tick"));
    }

    #[test]
    fn query_rejects_bad_types() {
        assert!(from_value::<HistoryQuery>(json!({ "limit": -1 })).is_err());
        assert!(from_value::<HistoryQuery>(json!({ "limit": "ten" })).is_err());
        assert!(from_value::<HistoryQuery>(json!({ "source": "nope" })).is_err());
    }

    #[test]
    fn query_serde_roundtrip() {
        assert_roundtrip(&HistoryQuery::default());
        assert_roundtrip(&HistoryQuery {
            since: Some("a".into()),
            until: Some("b".into()),
            event: Some("c".into()),
            source: Some(EventSource::DockerTracker),
            limit: Some(10),
        });
    }

    // ---- known limitation: timestamps are compared as strings ---------------
    // These describe the behaviour you'd probably want. They fail today, so they
    // are #[ignore]d. Run them with `-- --ignored`; remove the attribute once
    // you either parse timestamps or guarantee a single format.

    #[test]
    #[ignore = "known limitation: filter compares timestamps lexicographically"]
    fn mixed_utc_offsets_compare_chronologically() {
        // 10:00+03:30 is 06:30Z, which is BEFORE 07:00Z.
        let q = HistoryQuery {
            since: Some("2026-01-01T07:00:00Z".into()),
            ..Default::default()
        };
        assert!(!q.filter(&ev("e", "2026-01-01T10:00:00+03:30", EventSource::Systemd)));
    }

    #[test]
    #[ignore = "known limitation: filter compares timestamps lexicographically"]
    fn fractional_seconds_compare_chronologically() {
        // 00:00:00.500Z is AFTER 00:00:00Z.
        let q = HistoryQuery {
            since: Some("2026-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        assert!(q.filter(&ev("e", "2026-01-01T00:00:00.500Z", EventSource::Systemd)));
    }
}

// ---------------------------------------------------------------------------
// api.rs
// ---------------------------------------------------------------------------

mod api {
    use super::*;
    use kw_types::api::{
        ContainerRequest, ContainerTimeoutRequest, ControlUnitParams, HealthResponse,
        KillContainerRequest, KillProcessRequest, LoginRequest, LoginResponse, ScreenshotImage,
        ScreenshotResponse, SetPollIntervalRequest, SetRefreshMaskRequest, SetThresholdsRequest,
        TopContainersParams, TopProcessesParams,
    };
    use kw_types::docker::DockerSortKey;
    use kw_types::process::{ProcessSignal, ProcessesSortKey};
    use kw_types::systemd::ServiceAction;

    // ---- responses ---------------------------------------------------------

    #[test]
    fn health_display_and_roundtrip() {
        let h = HealthResponse {
            status: "ok".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            version: "0.3.1".into(),
            uptime: "1h 2m".into(),
        };
        assert_eq!(
            h.to_string(),
            "status: ok  version: 0.3.1  uptime: 1h 2m  (2026-01-01T00:00:00Z)"
        );
        assert_roundtrip(&h);
    }

    #[test]
    fn login_request_roundtrip_and_required_fields() {
        let r = LoginRequest {
            username: "admin".into(),
            password: "pw".into(),
        };
        assert_roundtrip(&r);
        assert!(from_value::<LoginRequest>(json!({ "username": "admin" })).is_err());
        assert!(from_value::<LoginRequest>(json!({ "password": "pw" })).is_err());
    }

    #[test]
    #[ignore = "known issue: derive(Debug) on LoginRequest prints the password"]
    fn login_request_debug_does_not_leak_password() {
        let r = LoginRequest {
            username: "admin".into(),
            password: "hunter2".into(),
        };
        assert!(!format!("{r:?}").contains("hunter2"));
    }

    #[test]
    fn login_response_display_and_roundtrip() {
        let r = LoginResponse {
            token: "abc123".into(),
        };
        assert_eq!(r.to_string(), "token: abc123");
        assert_roundtrip(&r);
    }

    fn screenshot(id: u32) -> ScreenshotImage {
        ScreenshotImage {
            data: "aGVsbG8=".into(),
            mime: "image/png".into(),
            monitor_name: "DP-1".into(),
            monitor_id: id,
            width: 1920,
            height: 1080,
            timestamp: "t".into(),
        }
    }

    #[test]
    fn screenshot_image_display_and_roundtrip() {
        let s = screenshot(1);
        assert_eq!(
            s.to_string(),
            "Screenshot[monitor=DP-1 (1), 1920x1080, mime=image/png, taken_at=t]"
        );
        assert_roundtrip(&s);
    }

    #[test]
    fn screenshot_display_never_includes_image_data() {
        // Display is used in logs/CLI; it must not dump the base64 payload.
        assert!(!screenshot(1).to_string().contains("aGVsbG8="));
    }

    #[test]
    fn screenshot_response_roundtrip() {
        let r = ScreenshotResponse {
            screens: vec![screenshot(1), screenshot(2)],
            count: 2,
        };
        assert_roundtrip(&r);
        let empty = ScreenshotResponse {
            screens: vec![],
            count: 0,
        };
        assert_roundtrip(&empty);
    }

    // ---- request / params: process ----------------------------------------

    #[test]
    fn top_processes_params_limit_is_optional_sort_is_required() {
        let p: TopProcessesParams = from_value(json!({ "sort": "cpu" })).unwrap();
        assert_eq!(p.limit, None);
        assert_eq!(p.sort, ProcessesSortKey::Cpu);

        let p: TopProcessesParams = from_value(json!({ "sort": "memory", "limit": 5 })).unwrap();
        assert_eq!(p.limit, Some(5));
        assert_eq!(p.sort, ProcessesSortKey::Memory);

        assert!(
            from_value::<TopProcessesParams>(json!({ "limit": 5 })).is_err(),
            "missing sort"
        );
        assert!(
            from_value::<TopProcessesParams>(json!({ "sort": "gpu" })).is_err(),
            "bad sort"
        );
        assert!(from_value::<TopProcessesParams>(json!({ "sort": "cpu", "limit": -1 })).is_err());
        assert!(from_value::<TopProcessesParams>(json!({ "sort": "cpu", "limit": 1.5 })).is_err());
    }

    #[test]
    fn top_processes_params_roundtrip() {
        assert_roundtrip(&TopProcessesParams {
            limit: Some(3),
            sort: ProcessesSortKey::Disk,
        });
        assert_roundtrip(&TopProcessesParams {
            limit: None,
            sort: ProcessesSortKey::Cpu,
        });
    }

    #[test]
    fn kill_process_request_uses_wire_signal_names() {
        let r: KillProcessRequest = from_value(json!({ "signal": "int" })).unwrap();
        assert_eq!(r.signal, ProcessSignal::Interrupt);
        let r: KillProcessRequest = from_value(json!({ "signal": "cont" })).unwrap();
        assert_eq!(r.signal, ProcessSignal::Continue);

        assert!(from_value::<KillProcessRequest>(json!({})).is_err());
        assert!(from_value::<KillProcessRequest>(json!({ "signal": "interrupt" })).is_err());
        assert!(from_value::<KillProcessRequest>(json!({ "signal": "KILL" })).is_err());
        assert_roundtrip(&KillProcessRequest {
            signal: ProcessSignal::Term,
        });
    }

    // ---- request / params: polling & system resources ----------------------

    #[test]
    fn set_poll_interval_request_parsing() {
        let r: SetPollIntervalRequest = from_value(json!({ "interval_ms": 1500 })).unwrap();
        assert_eq!(r.interval_ms, 1500);

        assert!(from_value::<SetPollIntervalRequest>(json!({})).is_err());
        assert!(from_value::<SetPollIntervalRequest>(json!({ "interval_ms": -1 })).is_err());
        assert!(from_value::<SetPollIntervalRequest>(json!({ "interval_ms": 1.5 })).is_err());
        assert!(from_value::<SetPollIntervalRequest>(json!({ "interval_ms": "1000" })).is_err());
        assert_roundtrip(&r);
    }

    #[test]
    fn set_poll_interval_request_accepts_zero_so_the_handler_must_reject_it() {
        // The type does not validate. `tokio::time::interval` panics on a zero
        // period, so the API handler is responsible for refusing 0.
        let r: SetPollIntervalRequest = from_value(json!({ "interval_ms": 0 })).unwrap();
        assert_eq!(r.interval_ms, 0);
    }

    #[test]
    fn set_thresholds_request_requires_all_four_fields() {
        let full = json!({
            "cpu_warn": 80.0, "memory_warn": 85.5, "disk_warn": 90.0, "battery_low": 10.0
        });
        let r: SetThresholdsRequest = from_value(full.clone()).unwrap();
        assert_eq!(r.cpu_warn, 80.0);
        assert_eq!(r.memory_warn, 85.5);
        assert_eq!(r.disk_warn, 90.0);
        assert_eq!(r.battery_low, 10.0);
        assert_roundtrip(&r);

        for key in ["cpu_warn", "memory_warn", "disk_warn", "battery_low"] {
            let mut v = full.clone();
            v.as_object_mut().unwrap().remove(key);
            assert!(
                from_value::<SetThresholdsRequest>(v).is_err(),
                "missing '{key}'"
            );
        }
    }

    #[test]
    fn set_thresholds_request_accepts_integers_and_does_not_range_check() {
        // JSON integers are fine for f32 fields.
        let r: SetThresholdsRequest = from_value(json!({
            "cpu_warn": 90, "memory_warn": 90, "disk_warn": 90, "battery_low": 15
        }))
        .unwrap();
        assert_eq!(r.cpu_warn, 90.0);

        // No range validation at the type level (validate in the handler).
        let r: SetThresholdsRequest = from_value(json!({
            "cpu_warn": 150.0, "memory_warn": -5.0, "disk_warn": 0.0, "battery_low": 1000.0
        }))
        .unwrap();
        assert_eq!(r.cpu_warn, 150.0);
        assert_eq!(r.memory_warn, -5.0);
    }

    #[test]
    fn set_refresh_mask_request_requires_all_six_flags() {
        let full = json!({
            "cpu": true, "memory": false, "disks": true,
            "networks": false, "temperatures": true, "gpus": false
        });
        let r: SetRefreshMaskRequest = from_value(full.clone()).unwrap();
        assert!(r.cpu && r.disks && r.temperatures);
        assert!(!r.memory && !r.networks && !r.gpus);
        assert_roundtrip(&r);

        for key in ["cpu", "memory", "disks", "networks", "temperatures", "gpus"] {
            let mut v = full.clone();
            v.as_object_mut().unwrap().remove(key);
            assert!(
                from_value::<SetRefreshMaskRequest>(v).is_err(),
                "missing '{key}'"
            );
        }
        assert!(
            from_value::<SetRefreshMaskRequest>(json!({
                "cpu": "yes", "memory": true, "disks": true,
                "networks": true, "temperatures": true, "gpus": true
            }))
            .is_err()
        );
    }

    // ---- request / params: systemd -----------------------------------------

    #[test]
    fn control_unit_params_parsing() {
        let p: ControlUnitParams =
            from_value(json!({ "unit_name": "nginx.service", "action": "restart" })).unwrap();
        assert_eq!(p.unit_name, "nginx.service");
        assert_eq!(p.action, ServiceAction::Restart);
        assert_roundtrip(&p);

        assert!(from_value::<ControlUnitParams>(json!({ "unit_name": "x" })).is_err());
        assert!(from_value::<ControlUnitParams>(json!({ "action": "stop" })).is_err());
        assert!(
            from_value::<ControlUnitParams>(json!({ "unit_name": "x", "action": "Restart" }))
                .is_err()
        );
    }

    // ---- request / params: docker ------------------------------------------

    #[test]
    fn top_containers_params_parsing() {
        let p: TopContainersParams = from_value(json!({ "sort": "memory" })).unwrap();
        assert_eq!(p.sort, DockerSortKey::Memory);
        assert_eq!(p.limit, None);

        let p: TopContainersParams = from_value(json!({ "sort": "cpu", "limit": 3 })).unwrap();
        assert_eq!(p.sort, DockerSortKey::Cpu);
        assert_eq!(p.limit, Some(3));
        assert_roundtrip(&p);

        // "disk" is a valid *process* sort key but not a container one
        assert!(from_value::<TopContainersParams>(json!({ "sort": "disk" })).is_err());
        assert!(from_value::<TopContainersParams>(json!({ "limit": 3 })).is_err());
    }

    #[test]
    fn container_request_requires_id_or_name() {
        let r: ContainerRequest = from_value(json!({ "id_or_name": "web" })).unwrap();
        assert_eq!(r.id_or_name, "web");
        assert_roundtrip(&r);
        assert!(from_value::<ContainerRequest>(json!({})).is_err());
        assert!(from_value::<ContainerRequest>(json!({ "id_or_name": 5 })).is_err());
    }

    #[test]
    fn kill_container_request_signal_is_optional_free_text() {
        let r: KillContainerRequest = from_value(json!({ "id_or_name": "web" })).unwrap();
        assert_eq!(r.signal, None);

        // The signal is a free-form string here (unlike process kill), so the
        // server side has to validate it.
        let r: KillContainerRequest =
            from_value(json!({ "id_or_name": "web", "signal": "SIGUSR1" })).unwrap();
        assert_eq!(r.signal.as_deref(), Some("SIGUSR1"));
        assert_roundtrip(&r);

        let r: KillContainerRequest =
            from_value(json!({ "id_or_name": "web", "signal": null })).unwrap();
        assert_eq!(r.signal, None);
    }

    #[test]
    fn container_timeout_request_timeout_is_optional_and_signed() {
        let r: ContainerTimeoutRequest = from_value(json!({ "id_or_name": "web" })).unwrap();
        assert_eq!(r.timeout_secs, None);

        let r: ContainerTimeoutRequest =
            from_value(json!({ "id_or_name": "web", "timeout_secs": 30 })).unwrap();
        assert_eq!(r.timeout_secs, Some(30));
        assert_roundtrip(&r);

        // i32 accepts negatives; Docker treats -1 as "wait indefinitely".
        let r: ContainerTimeoutRequest =
            from_value(json!({ "id_or_name": "web", "timeout_secs": -1 })).unwrap();
        assert_eq!(r.timeout_secs, Some(-1));

        assert!(
            from_value::<ContainerTimeoutRequest>(
                json!({ "id_or_name": "web", "timeout_secs": 3_000_000_000_u64 })
            )
            .is_err()
        );
    }
}

// ---------------------------------------------------------------------------
// lib.rs :: Info
// ---------------------------------------------------------------------------

mod info {
    use super::*;
    use kw_types::Info;

    fn info() -> Info {
        Info {
            auth_enabled: true,
            shutdown_enabled: false,
            blind: false,
            pid: vec![],
            top_processes: true,
            limit_processes: 10,
            telegram_bot: false,
            system_resources: true,
            systemd: false,
            docker: true,
            allow_process_commands: true,
            allow_screen_commands: false,
            allow_system_resources_commands: true,
            allow_systemd_commands: false,
            allow_docker_commands: true,
        }
    }

    /// The text after `label` on the line that starts with it, trimmed.
    /// (Looks values up by label so these tests don't depend on column padding.)
    fn value_of(out: &str, label: &str) -> String {
        out.lines()
            .find(|l| l.starts_with(label))
            .unwrap_or_else(|| panic!("no '{label}' line in:\n{out}"))
            .trim_start_matches(label)
            .trim()
            .to_string()
    }

    #[test]
    fn display_has_one_line_per_group() {
        let out = info().to_string();
        assert_eq!(out.lines().count(), 8, "got:\n{out}");
    }

    #[test]
    fn display_values() {
        let out = info().to_string();
        assert_eq!(value_of(&out, "auth enabled:"), "true");
        assert_eq!(value_of(&out, "shutdown enabled:"), "false");
        assert_eq!(value_of(&out, "blind:"), "false");
        assert_eq!(value_of(&out, "tracked PIDs:"), "none");
        assert_eq!(value_of(&out, "top processes:"), "true  (limit: 10)");
        assert_eq!(value_of(&out, "telegram bot:"), "false");
        assert_eq!(
            value_of(&out, "features:"),
            "system_resources=true systemd=false docker=true"
        );
        assert_eq!(
            value_of(&out, "commands:"),
            "process=true screen=false resources=true systemd=false docker=true"
        );
    }

    #[test]
    fn display_lists_tracked_pids() {
        let mut i = info();
        i.pid = vec![1];
        assert_eq!(value_of(&i.to_string(), "tracked PIDs:"), "1");
        i.pid = vec![1, 22, 333];
        assert_eq!(value_of(&i.to_string(), "tracked PIDs:"), "1, 22, 333");
    }

    #[test]
    fn json_has_the_fifteen_keys_clients_rely_on() {
        let v = serde_json::to_value(info()).unwrap();
        assert_eq!(
            sorted_keys(&v),
            [
                "allow_docker_commands",
                "allow_process_commands",
                "allow_screen_commands",
                "allow_system_resources_commands",
                "allow_systemd_commands",
                "auth_enabled",
                "blind",
                "docker",
                "limit_processes",
                "pid",
                "shutdown_enabled",
                "system_resources",
                "systemd",
                "telegram_bot",
                "top_processes",
            ]
        );
        assert_eq!(v["pid"], json!([]));
        assert_eq!(v["limit_processes"], json!(10));
    }

    #[test]
    fn roundtrip_with_and_without_pids() {
        assert_roundtrip(&info());
        let mut i = info();
        i.pid = vec![4, 5, 6];
        assert_roundtrip(&i);
    }

    #[test]
    fn deserialize_requires_every_field() {
        let full = serde_json::to_value(info()).unwrap();
        assert!(from_value::<Info>(full.clone()).is_ok());
        for key in full.as_object().unwrap().keys() {
            let mut v = full.clone();
            v.as_object_mut().unwrap().remove(key);
            assert!(
                from_value::<Info>(v).is_err(),
                "missing '{key}' should fail"
            );
        }
    }
}
