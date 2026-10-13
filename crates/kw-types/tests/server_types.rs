#[allow(unused_imports)]
use serde_json::{Value, json};

#[allow(dead_code)]
fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

// ---------------------------------------------------------------------------
// PollStatus (not feature-gated)
// ---------------------------------------------------------------------------

mod poll_status {
    use super::*;
    use kw_types::polling::PollStatus;

    #[test]
    fn display_paused_and_resumed() {
        assert_eq!(
            PollStatus {
                interval: 5000,
                paused: true
            }
            .to_string(),
            "interval: 5000(ms) paused"
        );
        assert_eq!(
            PollStatus {
                interval: 250,
                paused: false
            }
            .to_string(),
            "interval: 250(ms) resumed"
        );
        assert_eq!(
            PollStatus {
                interval: 0,
                paused: false
            }
            .to_string(),
            "interval: 0(ms) resumed"
        );
    }

    #[test]
    fn json_shape_and_roundtrip() {
        let s = PollStatus {
            interval: 5000,
            paused: true,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v, json!({ "interval": 5000, "paused": true }));

        let back: PollStatus = serde_json::from_value(v).unwrap();
        assert_eq!(back.interval, 5000);
        assert!(back.paused);
    }

    #[test]
    fn interval_up_to_u64_max_roundtrips() {
        let s = PollStatus {
            interval: u128::from(u64::MAX),
            paused: false,
        };
        let back: PollStatus = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(back.interval, u128::from(u64::MAX));
    }

    #[test]
    fn rejects_bad_json() {
        assert!(serde_json::from_value::<PollStatus>(json!({ "interval": 1 })).is_err());
        assert!(serde_json::from_value::<PollStatus>(json!({ "paused": true })).is_err());
        assert!(
            serde_json::from_value::<PollStatus>(json!({ "interval": -1, "paused": true }))
                .is_err()
        );
        assert!(
            serde_json::from_value::<PollStatus>(json!({ "interval": 1, "paused": "yes" }))
                .is_err()
        );
    }
}

// ---------------------------------------------------------------------------
// Poll (tokio timers, paused clock)
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
mod poll {
    use kw_types::polling::{Poll, PollStatus};
    use std::time::Duration;
    use tokio::time::Instant;

    #[test]
    fn new_starts_paused_without_a_timer() {
        let p = Poll::new(5);
        assert!(p.is_paused());
        assert!(p.interval_timer.is_none());
        assert_eq!(p.interval, Duration::from_secs(5));
    }

    #[test]
    fn status_reflects_interval_and_paused_flag() {
        let p = Poll::new(5);
        let s = PollStatus::from(&p);
        assert_eq!(s.interval, 5000);
        assert!(s.paused);
    }

    #[tokio::test(start_paused = true)]
    async fn resume_creates_a_timer_and_pause_drops_it() {
        let mut p = Poll::new(5);
        p.resume();
        assert!(!p.is_paused());
        assert!(p.interval_timer.is_some());
        assert!(!PollStatus::from(&p).paused);

        p.pause();
        assert!(p.is_paused());
        assert!(p.interval_timer.is_none());
        assert!(PollStatus::from(&p).paused);
        // the interval itself is kept
        assert_eq!(p.interval, Duration::from_secs(5));
    }

    #[tokio::test(start_paused = true)]
    async fn first_tick_is_immediate_then_one_per_interval() {
        let mut p = Poll::new(5);
        p.resume();
        let timer = p.interval_timer.as_mut().unwrap();

        let start = Instant::now();
        timer.tick().await;
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "first tick should be immediate"
        );

        timer.tick().await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_secs(5), "elapsed {elapsed:?}");
        assert!(elapsed < Duration::from_secs(6), "elapsed {elapsed:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn pause_then_resume_keeps_the_interval() {
        let mut p = Poll::new(7);
        p.resume();
        p.pause();
        p.resume();
        assert!(!p.is_paused());
        assert_eq!(p.interval, Duration::from_secs(7));
        assert_eq!(PollStatus::from(&p).interval, 7000);
    }

    #[tokio::test(start_paused = true)]
    async fn set_interval_while_running_restarts_the_timer_with_the_new_period() {
        let mut p = Poll::new(5);
        p.resume();
        p.set_interval(Duration::from_secs(2));
        assert!(!p.is_paused());
        assert_eq!(p.interval, Duration::from_secs(2));
        assert_eq!(PollStatus::from(&p).interval, 2000);

        let timer = p.interval_timer.as_mut().unwrap();
        let start = Instant::now();
        timer.tick().await; // fresh interval: immediate
        timer.tick().await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_secs(2), "elapsed {elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "elapsed {elapsed:?}");
    }

    #[test]
    fn set_interval_while_paused_stays_paused() {
        // plain #[test]: no timer is created, so no runtime is needed
        let mut p = Poll::new(5);
        p.set_interval(Duration::from_millis(1500));
        assert!(p.is_paused());
        assert!(p.interval_timer.is_none());
        assert_eq!(p.interval, Duration::from_millis(1500));
        assert_eq!(PollStatus::from(&p).interval, 1500);
    }

    #[test]
    fn status_reports_sub_second_intervals_in_ms() {
        let mut p = Poll::new(1);
        p.set_interval(Duration::from_micros(2_500));
        assert_eq!(PollStatus::from(&p).interval, 2); // as_millis truncates
    }

    // ---- zero period: documented panics ------------------------------------
    // `tokio::time::interval` panics on a zero period. These pin that behaviour
    // so the API/socket handlers know they must reject 0 BEFORE it reaches Poll.
    // If you make Poll reject or clamp zero itself, replace these tests.

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "non-zero")]
    async fn resume_with_zero_seconds_panics() {
        let mut p = Poll::new(0);
        p.resume();
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "non-zero")]
    async fn set_interval_zero_while_running_panics() {
        let mut p = Poll::new(5);
        p.resume();
        p.set_interval(Duration::ZERO);
    }

    #[test]
    fn set_interval_zero_while_paused_does_not_panic_until_resume() {
        let mut p = Poll::new(5);
        p.set_interval(Duration::ZERO);
        assert!(p.is_paused());
        assert_eq!(p.interval, Duration::ZERO);
    }
}

// ---------------------------------------------------------------------------
// Docker: compute_cpu_percent
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
mod docker_cpu {
    use super::*;
    use bollard::config::ContainerStatsResponse;
    use kw_types::docker::compute_cpu_percent;

    fn stats(v: Value) -> ContainerStatsResponse {
        serde_json::from_value(v).expect("valid docker stats payload")
    }

    /// cpu_stats / precpu_stats in the shape Docker's stats endpoint returns.
    fn cpu_payload(total: u64, pre_total: u64, sys: u64, pre_sys: u64, cpus: Option<u32>) -> Value {
        let mut cpu = json!({
            "cpu_usage": { "total_usage": total },
            "system_cpu_usage": sys
        });
        if let Some(n) = cpus {
            cpu["online_cpus"] = json!(n);
        }
        json!({
            "cpu_stats": cpu,
            "precpu_stats": {
                "cpu_usage": { "total_usage": pre_total },
                "system_cpu_usage": pre_sys
            }
        })
    }

    #[test]
    fn follows_dockers_formula() {
        // cpu_delta 50, system_delta 1000, 4 cpus => 50/1000 * 4 * 100 = 20
        approx(
            compute_cpu_percent(&stats(cpu_payload(1_050, 1_000, 11_000, 10_000, Some(4)))),
            20.0,
        );
    }

    #[test]
    fn one_cpu_fully_used_is_100_percent() {
        approx(
            compute_cpu_percent(&stats(cpu_payload(2_000, 1_000, 2_000, 1_000, Some(1)))),
            100.0,
        );
    }

    #[test]
    fn percent_is_scaled_by_cpu_count_so_it_can_exceed_100() {
        approx(
            compute_cpu_percent(&stats(cpu_payload(2_000, 1_000, 2_000, 1_000, Some(8)))),
            800.0,
        );
    }

    #[test]
    fn zero_system_delta_gives_zero() {
        approx(
            compute_cpu_percent(&stats(cpu_payload(1_050, 1_000, 10_000, 10_000, Some(4)))),
            0.0,
        );
    }

    #[test]
    fn zero_cpu_delta_gives_zero() {
        approx(
            compute_cpu_percent(&stats(cpu_payload(1_000, 1_000, 11_000, 10_000, Some(4)))),
            0.0,
        );
    }

    #[test]
    fn counters_going_backwards_saturate_instead_of_underflowing() {
        // container restarted between frames, or counters wrapped
        approx(
            compute_cpu_percent(&stats(cpu_payload(900, 1_000, 11_000, 10_000, Some(4)))),
            0.0,
        );
        approx(
            compute_cpu_percent(&stats(cpu_payload(1_050, 1_000, 9_000, 10_000, Some(4)))),
            0.0,
        );
    }

    #[test]
    fn missing_data_gives_zero_not_a_panic() {
        // nothing at all
        approx(compute_cpu_percent(&stats(json!({}))), 0.0);
        // no previous frame (first sample after start)
        approx(
            compute_cpu_percent(&stats(json!({
                "cpu_stats": {
                    "cpu_usage": { "total_usage": 1_050 },
                    "system_cpu_usage": 11_000,
                    "online_cpus": 4
                }
            }))),
            0.0,
        );
        // no container cpu usage, only system counters
        approx(
            compute_cpu_percent(&stats(json!({
                "cpu_stats": { "system_cpu_usage": 11_000 },
                "precpu_stats": { "system_cpu_usage": 10_000 }
            }))),
            0.0,
        );
        // no system counters
        approx(
            compute_cpu_percent(&stats(json!({
                "cpu_stats": { "cpu_usage": { "total_usage": 1_050 } },
                "precpu_stats": { "cpu_usage": { "total_usage": 1_000 } }
            }))),
            0.0,
        );
    }

    #[test]
    fn missing_online_cpus_falls_back_to_percpu_usage_length() {
        // 50/1000*100 = 5 per cpu, 2 entries in percpu_usage => 10
        let s = stats(json!({
            "cpu_stats": {
                "cpu_usage": { "total_usage": 1_050, "percpu_usage": [600, 450] },
                "system_cpu_usage": 11_000
            },
            "precpu_stats": {
                "cpu_usage": { "total_usage": 1_000 },
                "system_cpu_usage": 10_000
            }
        }));
        approx(compute_cpu_percent(&s), 10.0);
    }

    #[test]
    fn missing_online_cpus_and_percpu_assumes_one_cpu() {
        approx(
            compute_cpu_percent(&stats(cpu_payload(1_050, 1_000, 11_000, 10_000, None))),
            5.0,
        );
    }

    #[test]
    fn online_cpus_of_zero_is_used_as_is_and_gives_zero_percent() {
        // Some(0) is NOT replaced by the percpu fallback. If a Docker host ever
        // reports online_cpus = 0, CPU % will read 0 for that container.
        approx(
            compute_cpu_percent(&stats(cpu_payload(1_050, 1_000, 11_000, 10_000, Some(0)))),
            0.0,
        );
    }
}

// ---------------------------------------------------------------------------
// Docker: ContainerStats::from_bollard
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
mod docker_stats {
    use super::*;
    use bollard::config::ContainerStatsResponse;
    use kw_types::docker::{ContainerStats, compute_cpu_percent};

    const MIB: u64 = 1024 * 1024;

    fn stats(v: Value) -> ContainerStatsResponse {
        serde_json::from_value(v).expect("valid docker stats payload")
    }

    fn full_payload() -> Value {
        json!({
            "cpu_stats": {
                "cpu_usage": { "total_usage": 1_050 },
                "system_cpu_usage": 11_000,
                "online_cpus": 4
            },
            "precpu_stats": {
                "cpu_usage": { "total_usage": 1_000 },
                "system_cpu_usage": 10_000
            },
            "memory_stats": { "usage": 512 * MIB, "limit": 1024 * MIB },
            "networks": {
                "eth0": { "rx_bytes": 100, "tx_bytes": 200 },
                "eth1": { "rx_bytes": 1, "tx_bytes": 2 }
            },
            "blkio_stats": {
                "io_service_bytes_recursive": [
                    { "major": 8, "minor": 0, "op": "read", "value": 300 },
                    { "major": 8, "minor": 0, "op": "write", "value": 400 },
                    { "major": 8, "minor": 0, "op": "total", "value": 700 }
                ]
            },
            "pids_stats": { "current": 7 }
        })
    }

    #[test]
    fn full_payload_is_mapped_field_by_field() {
        let s = ContainerStats::from_bollard(&stats(full_payload()));
        approx(s.cpu_percent, 20.0);
        assert_eq!(s.memory_bytes, 512 * MIB);
        assert_eq!(s.memory_limit_bytes, 1024 * MIB);
        approx(s.memory_percent.expect("limit is set"), 0.5);
        assert_eq!(s.net_rx_bytes, 101);
        assert_eq!(s.net_tx_bytes, 202);
        assert_eq!(s.block_read_bytes, 300);
        assert_eq!(s.block_write_bytes, 400);
        assert_eq!(s.pid_count, 7);
    }

    #[test]
    fn cpu_percent_is_the_same_as_compute_cpu_percent() {
        let raw = stats(full_payload());
        approx(
            ContainerStats::from_bollard(&raw).cpu_percent,
            compute_cpu_percent(&raw),
        );
    }

    #[test]
    fn empty_payload_gives_all_zeros_and_no_memory_percent() {
        let s = ContainerStats::from_bollard(&stats(json!({})));
        approx(s.cpu_percent, 0.0);
        assert_eq!(s.memory_bytes, 0);
        assert_eq!(s.memory_limit_bytes, 0);
        assert_eq!(s.memory_percent, None);
        assert_eq!(s.net_rx_bytes, 0);
        assert_eq!(s.net_tx_bytes, 0);
        assert_eq!(s.block_read_bytes, 0);
        assert_eq!(s.block_write_bytes, 0);
        assert_eq!(s.pid_count, 0);
    }

    #[test]
    fn memory_percent_is_none_when_limit_is_zero_or_missing() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "memory_stats": { "usage": 100, "limit": 0 }
        })));
        assert_eq!(s.memory_bytes, 100);
        assert_eq!(s.memory_percent, None);

        let s = ContainerStats::from_bollard(&stats(json!({
            "memory_stats": { "usage": 100 }
        })));
        assert_eq!(s.memory_limit_bytes, 0);
        assert_eq!(s.memory_percent, None);
    }

    #[test]
    fn memory_is_the_raw_usage_figure() {
        // `usage` from Docker includes page cache. `docker stats` subtracts it
        // (cgroup v1: cache, v2: inactive_file), so numbers shown here will be
        // higher than `docker stats` for the same container. This pins the
        // current behaviour: extra memory_stats fields are ignored.
        let s = ContainerStats::from_bollard(&stats(json!({
            "memory_stats": {
                "usage": 300 * MIB,
                "limit": 1024 * MIB,
                "stats": { "inactive_file": 200 * MIB, "cache": 200 * MIB }
            }
        })));
        assert_eq!(s.memory_bytes, 300 * MIB);
    }

    #[test]
    fn network_counters_are_summed_across_interfaces() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "networks": {
                "eth0": { "rx_bytes": 10, "tx_bytes": 20 },
                "eth1": { "rx_bytes": 30, "tx_bytes": 40 },
                "lo":   { "rx_bytes": 50, "tx_bytes": 60 }
            }
        })));
        assert_eq!(s.net_rx_bytes, 90);
        assert_eq!(s.net_tx_bytes, 120);
    }

    #[test]
    fn network_interface_with_missing_counters_counts_as_zero() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "networks": { "eth0": { "rx_bytes": 10 }, "eth1": {} }
        })));
        assert_eq!(s.net_rx_bytes, 10);
        assert_eq!(s.net_tx_bytes, 0);
    }

    #[test]
    fn network_sums_saturate_instead_of_overflowing() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "networks": {
                "eth0": { "rx_bytes": u64::MAX, "tx_bytes": u64::MAX },
                "eth1": { "rx_bytes": 1, "tx_bytes": 1 }
            }
        })));
        assert_eq!(s.net_rx_bytes, u64::MAX);
        assert_eq!(s.net_tx_bytes, u64::MAX);
    }

    #[test]
    fn block_io_matches_read_and_write_case_insensitively_and_ignores_the_rest() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "blkio_stats": { "io_service_bytes_recursive": [
                { "op": "Read",  "value": 1 },
                { "op": "READ",  "value": 2 },
                { "op": "write", "value": 10 },
                { "op": "Write", "value": 20 },
                { "op": "Total", "value": 1000 },
                { "op": "Sync",  "value": 1000 },
                { "op": "Async", "value": 1000 },
                { "op": "Discard", "value": 1000 }
            ] }
        })));
        assert_eq!(s.block_read_bytes, 3);
        assert_eq!(s.block_write_bytes, 30);
    }

    #[test]
    fn block_io_entries_with_missing_op_or_value_are_harmless() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "blkio_stats": { "io_service_bytes_recursive": [
                { "value": 999 },
                { "op": "read" },
                { "op": null, "value": 5 },
                { "op": "write", "value": 7 }
            ] }
        })));
        assert_eq!(s.block_read_bytes, 0);
        assert_eq!(s.block_write_bytes, 7);
    }

    #[test]
    fn block_io_missing_or_null_list_gives_zero() {
        // cgroup v2 hosts often report null here
        for payload in [
            json!({ "blkio_stats": {} }),
            json!({ "blkio_stats": { "io_service_bytes_recursive": null } }),
            json!({ "blkio_stats": { "io_service_bytes_recursive": [] } }),
        ] {
            let s = ContainerStats::from_bollard(&stats(payload));
            assert_eq!(s.block_read_bytes, 0);
            assert_eq!(s.block_write_bytes, 0);
        }
    }

    #[test]
    fn block_io_sums_saturate() {
        let s = ContainerStats::from_bollard(&stats(json!({
            "blkio_stats": { "io_service_bytes_recursive": [
                { "op": "read", "value": u64::MAX },
                { "op": "read", "value": 1 }
            ] }
        })));
        assert_eq!(s.block_read_bytes, u64::MAX);
    }
}

// ---------------------------------------------------------------------------
// Docker: ContainerStatus::from_state_enum and ContainerSnapshot::from_summary
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
mod docker_summary {
    use super::*;
    use bollard::config::ContainerSummaryStateEnum as S;
    use bollard::models::ContainerSummary;
    use kw_types::docker::{ContainerHealth, ContainerSnapshot, ContainerStatus};

    fn summary(v: Value) -> ContainerSummary {
        serde_json::from_value(v).expect("valid container summary")
    }

    fn base() -> Value {
        json!({
            "Id": "abc123def4567890abcdef",
            "Names": ["/web"],
            "Image": "nginx:latest",
            "State": "running",
            "Status": "Up 3 hours (healthy)"
        })
    }

    // ---- from_state_enum ---------------------------------------------------

    #[test]
    fn state_enum_maps_every_variant() {
        let cases = [
            (S::CREATED, ContainerStatus::Created),
            (S::RUNNING, ContainerStatus::Running),
            (S::PAUSED, ContainerStatus::Paused),
            (S::RESTARTING, ContainerStatus::Restarting),
            (S::REMOVING, ContainerStatus::Removing),
            (S::EXITED, ContainerStatus::Exited),
            (S::DEAD, ContainerStatus::Dead),
            (S::STOPPING, ContainerStatus::Stopping),
        ];
        for (state, expected) in cases {
            assert_eq!(
                ContainerStatus::from_state_enum(Some(&state)),
                expected,
                "{state:?}"
            );
        }
    }

    #[test]
    fn state_enum_none_and_empty_are_unknown_empty() {
        let unknown = ContainerStatus::Unknown("empty".to_string());
        assert_eq!(ContainerStatus::from_state_enum(None), unknown);
        assert_eq!(ContainerStatus::from_state_enum(Some(&S::EMPTY)), unknown);
    }

    // ---- from_summary ------------------------------------------------------

    #[test]
    fn full_summary_is_mapped() {
        let c = ContainerSnapshot::from_summary(&summary(base())).expect("has an id");
        assert_eq!(c.id, "abc123def4567890abcdef");
        assert_eq!(c.short_id, "abc123def456");
        assert_eq!(c.name, "web");
        assert_eq!(c.image, "nginx:latest");
        assert_eq!(c.status, ContainerStatus::Running);
        assert_eq!(c.health, ContainerHealth::Healthy);
        assert!(c.stats.is_none());
    }

    #[test]
    fn summary_without_an_id_is_skipped() {
        let mut v = base();
        v.as_object_mut().unwrap().remove("Id");
        assert!(ContainerSnapshot::from_summary(&summary(v)).is_none());
    }

    #[test]
    fn name_comes_from_the_first_entry_without_leading_slashes() {
        let mut v = base();
        v["Names"] = json!(["/web", "/other"]);
        assert_eq!(
            ContainerSnapshot::from_summary(&summary(v)).unwrap().name,
            "web"
        );

        let mut v = base();
        v["Names"] = json!(["//web"]);
        assert_eq!(
            ContainerSnapshot::from_summary(&summary(v)).unwrap().name,
            "web"
        );

        let mut v = base();
        v["Names"] = json!(["noslash"]);
        assert_eq!(
            ContainerSnapshot::from_summary(&summary(v)).unwrap().name,
            "noslash"
        );
    }

    #[test]
    fn name_falls_back_to_the_short_id() {
        let mut v = base();
        v["Names"] = json!([]);
        let c = ContainerSnapshot::from_summary(&summary(v)).unwrap();
        assert_eq!(c.name, c.short_id);

        let mut v = base();
        v.as_object_mut().unwrap().remove("Names");
        let c = ContainerSnapshot::from_summary(&summary(v)).unwrap();
        assert_eq!(c.name, "abc123def456");
    }

    #[test]
    fn short_id_handles_short_and_non_ascii_ids() {
        let mut v = base();
        v["Id"] = json!("abc");
        let c = ContainerSnapshot::from_summary(&summary(v)).unwrap();
        assert_eq!(c.short_id, "abc");
        assert_eq!(c.id, "abc");

        // truncates by characters, so it can't split a UTF-8 sequence
        let mut v = base();
        v["Id"] = json!("é".repeat(20));
        let c = ContainerSnapshot::from_summary(&summary(v)).unwrap();
        assert_eq!(c.short_id, "é".repeat(12));
    }

    #[test]
    fn missing_image_state_and_status_get_defaults() {
        let c =
            ContainerSnapshot::from_summary(&summary(json!({ "Id": "abc123def4567890" }))).unwrap();
        assert_eq!(c.image, "");
        assert_eq!(c.status, ContainerStatus::Unknown("empty".to_string()));
        assert_eq!(c.health, ContainerHealth::None);
        assert_eq!(c.name, c.short_id);
    }

    #[test]
    fn docker_state_strings_map_to_statuses() {
        let cases = [
            ("created", ContainerStatus::Created),
            ("running", ContainerStatus::Running),
            ("paused", ContainerStatus::Paused),
            ("restarting", ContainerStatus::Restarting),
            ("removing", ContainerStatus::Removing),
            ("exited", ContainerStatus::Exited),
            ("dead", ContainerStatus::Dead),
        ];
        for (state, expected) in cases {
            let mut v = base();
            v["State"] = json!(state);
            assert_eq!(
                ContainerSnapshot::from_summary(&summary(v)).unwrap().status,
                expected,
                "state '{state}'"
            );
        }
    }

    #[test]
    fn health_is_derived_from_the_status_text() {
        let cases = [
            ("Up 3 hours (healthy)", ContainerHealth::Healthy),
            ("Up 5 minutes (unhealthy)", ContainerHealth::Unhealthy),
            (
                "Up 10 seconds (health: starting)",
                ContainerHealth::Starting,
            ),
            ("Up 2 days", ContainerHealth::None),
            ("Exited (1) 3 minutes ago", ContainerHealth::None),
        ];
        for (text, expected) in cases {
            let mut v = base();
            v["Status"] = json!(text);
            assert_eq!(
                ContainerSnapshot::from_summary(&summary(v)).unwrap().health,
                expected,
                "status '{text}'"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// other `From` conversions that don't need a live OS object
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
mod conversions {
    use kw_types::process::{ProcessSignal, ProcessState, disk_usage_total};
    use kw_types::resources::{BatteryState, DiskKind};

    // ---- disk_usage_total --------------------------------------------------

    #[test]
    fn disk_usage_total_is_read_plus_written() {
        let du = sysinfo::DiskUsage {
            read_bytes: 20,
            written_bytes: 10,
            ..Default::default()
        };
        assert_eq!(disk_usage_total(du), 30);
        assert_eq!(disk_usage_total(sysinfo::DiskUsage::default()), 0);
    }

    #[test]
    fn disk_usage_total_ignores_the_cumulative_counters() {
        let du = sysinfo::DiskUsage {
            total_read_bytes: 1_000,
            total_written_bytes: 2_000,
            read_bytes: 1,
            written_bytes: 2,
        };
        assert_eq!(disk_usage_total(du), 3);
    }

    #[test]
    fn disk_usage_total_saturates() {
        let du = sysinfo::DiskUsage {
            read_bytes: u64::MAX,
            written_bytes: 5,
            ..Default::default()
        };
        assert_eq!(disk_usage_total(du), u64::MAX);
    }

    // ---- ProcessSignal -> sysinfo ------------------------------------------

    #[test]
    fn sysinfo_signal_is_some_exactly_when_the_signal_is_supported() {
        for s in [
            ProcessSignal::Kill,
            ProcessSignal::Interrupt,
            ProcessSignal::Stop,
            ProcessSignal::Continue,
            ProcessSignal::Term,
        ] {
            assert_eq!(s.sysinfo_signal().is_some(), s.is_supported(), "{s:?}");
        }
    }

    #[test]
    fn sysinfo_signal_mapping() {
        assert_eq!(
            ProcessSignal::Kill.sysinfo_signal(),
            Some(sysinfo::Signal::Kill)
        );
        if cfg!(windows) {
            // only Kill is deliverable on Windows
            for s in [
                ProcessSignal::Interrupt,
                ProcessSignal::Stop,
                ProcessSignal::Continue,
                ProcessSignal::Term,
            ] {
                assert_eq!(s.sysinfo_signal(), None, "{s:?}");
            }
        } else {
            assert_eq!(
                ProcessSignal::Interrupt.sysinfo_signal(),
                Some(sysinfo::Signal::Interrupt)
            );
            assert_eq!(
                ProcessSignal::Stop.sysinfo_signal(),
                Some(sysinfo::Signal::Stop)
            );
            assert_eq!(
                ProcessSignal::Continue.sysinfo_signal(),
                Some(sysinfo::Signal::Continue)
            );
            assert_eq!(
                ProcessSignal::Term.sysinfo_signal(),
                Some(sysinfo::Signal::Term)
            );
        }
    }

    // ---- ProcessState from sysinfo -----------------------------------------

    #[test]
    fn process_state_from_sysinfo_status() {
        assert_eq!(
            ProcessState::from(sysinfo::ProcessStatus::Run),
            ProcessState::Running
        );
        assert_eq!(
            ProcessState::from(sysinfo::ProcessStatus::Sleep),
            ProcessState::Sleeping
        );
        assert_eq!(
            ProcessState::from(sysinfo::ProcessStatus::Idle),
            ProcessState::Sleeping
        );
        assert_eq!(
            ProcessState::from(sysinfo::ProcessStatus::Zombie),
            ProcessState::Other("Zombie".to_string())
        );
        assert_eq!(
            ProcessState::from(sysinfo::ProcessStatus::Stop),
            ProcessState::Other("Stop".to_string())
        );
    }

    // ---- DiskKind / BatteryState -------------------------------------------

    #[test]
    fn disk_kind_from_sysinfo() {
        assert_eq!(DiskKind::from(sysinfo::DiskKind::HDD), DiskKind::Hdd);
        assert_eq!(DiskKind::from(sysinfo::DiskKind::SSD), DiskKind::Ssd);
        assert_eq!(
            DiskKind::from(sysinfo::DiskKind::Unknown(-1)),
            DiskKind::Unknown
        );
        assert_eq!(
            DiskKind::from(sysinfo::DiskKind::Unknown(3)),
            DiskKind::Unknown
        );
    }

    #[test]
    fn battery_state_from_starship_battery() {
        use starship_battery::State;
        assert_eq!(BatteryState::from(State::Charging), BatteryState::Charging);
        assert_eq!(
            BatteryState::from(State::Discharging),
            BatteryState::Discharging
        );
        assert_eq!(BatteryState::from(State::Full), BatteryState::Full);
        assert_eq!(BatteryState::from(State::Unknown), BatteryState::Unknown);
        // "empty" has no counterpart in our enum
        assert_eq!(BatteryState::from(State::Empty), BatteryState::Unknown);
    }

    // ---- platform-specific (compiled and run on Linux / macOS only) --------

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn load_average_from_sysinfo() {
        use kw_types::resources::LoadAverage;
        let la = LoadAverage::from(sysinfo::LoadAvg {
            one: 1.0,
            five: 2.0,
            fifteen: 3.0,
        });
        assert_eq!((la.one, la.five, la.fifteen), (1.0, 2.0, 3.0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn fd_type_from_procfs_target() {
        use kw_types::process::FDType;
        use procfs::process::FDTarget;
        use std::path::PathBuf;

        assert_eq!(
            FDType::from(FDTarget::Path(PathBuf::from("/tmp/x"))).to_string(),
            "file"
        );
        assert_eq!(FDType::from(FDTarget::Socket(123)).to_string(), "socket");
        assert_eq!(FDType::from(FDTarget::Pipe(5)).to_string(), "pipe");
        assert_eq!(
            FDType::from(FDTarget::AnonInode("eventfd".to_string())).to_string(),
            "other"
        );
    }
}
