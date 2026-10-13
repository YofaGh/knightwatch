use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Serialize -> deserialize -> serialize again; both JSON values must match.
/// (Several types don't derive `PartialEq`, so we compare the JSON instead.)
fn assert_roundtrip<T: Serialize + DeserializeOwned>(v: &T) {
    let first = serde_json::to_value(v).unwrap();
    let back: T = serde_json::from_value(first.clone()).unwrap();
    let second = serde_json::to_value(&back).unwrap();
    assert_eq!(first, second);
}

/// The serde string of a unit-variant enum, without the JSON quotes.
fn serde_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v).unwrap() {
        Value::String(s) => s,
        other => panic!("expected a JSON string, got {other}"),
    }
}

// ---------------------------------------------------------------------------
// process.rs
// ---------------------------------------------------------------------------

mod process {
    use super::*;
    use kw_types::process::{
        FDType, FileDescriptorInfo, IOStats, ProcessSignal, ProcessSnapshot, ProcessState,
        ProcessStatus, ProcessTree, ProcessesSortKey,
    };

    const ALL_SIGNALS: [ProcessSignal; 5] = [
        ProcessSignal::Kill,
        ProcessSignal::Interrupt,
        ProcessSignal::Stop,
        ProcessSignal::Continue,
        ProcessSignal::Term,
    ];

    // ---- ProcessSignal -----------------------------------------------------

    #[test]
    fn signal_display_matches_serde_for_every_variant() {
        for s in ALL_SIGNALS {
            assert_eq!(s.to_string(), serde_str(&s), "variant {s:?}");
        }
    }

    #[test]
    fn signal_wire_names_are_stable() {
        // These strings are part of the public API / socket protocol.
        assert_eq!(serde_str(&ProcessSignal::Kill), "kill");
        assert_eq!(serde_str(&ProcessSignal::Interrupt), "int");
        assert_eq!(serde_str(&ProcessSignal::Stop), "stop");
        assert_eq!(serde_str(&ProcessSignal::Continue), "cont");
        assert_eq!(serde_str(&ProcessSignal::Term), "term");
    }

    #[test]
    fn signal_try_from_roundtrips_display() {
        for s in ALL_SIGNALS {
            assert_eq!(ProcessSignal::try_from(s.to_string().as_str()), Ok(s));
        }
    }

    #[test]
    fn signal_serde_roundtrip() {
        for s in ALL_SIGNALS {
            let json = serde_json::to_string(&s).unwrap();
            let back: ProcessSignal = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn signal_try_from_rejects_unknown_and_is_case_sensitive() {
        // NOTE: unlike the sort keys, signal parsing is case-sensitive.
        for bad in [
            "",
            "KILL",
            "Kill",
            "interrupt",
            "continue",
            " kill",
            "sigkill",
        ] {
            assert!(ProcessSignal::try_from(bad).is_err(), "'{bad}' should fail");
        }
    }

    #[test]
    fn signal_try_from_error_message() {
        assert_eq!(
            ProcessSignal::try_from("nope"),
            Err("Invalid signal: 'nope'.".to_string())
        );
    }

    #[test]
    fn signal_serde_rejects_unknown() {
        assert!(serde_json::from_str::<ProcessSignal>("\"interrupt\"").is_err());
        assert!(serde_json::from_str::<ProcessSignal>("\"Kill\"").is_err());
    }

    #[test]
    fn supported_signals_are_consistent_with_is_supported() {
        let supported = ProcessSignal::get_supported_signals();
        assert!(supported.contains(&ProcessSignal::Kill));
        for s in ALL_SIGNALS {
            assert_eq!(s.is_supported(), supported.contains(&s), "variant {s:?}");
        }
        if cfg!(windows) {
            assert_eq!(supported, vec![ProcessSignal::Kill]);
        } else {
            assert_eq!(supported.len(), 5);
        }
    }

    // ---- ProcessState ------------------------------------------------------

    #[test]
    fn state_display() {
        assert_eq!(ProcessState::Running.to_string(), "running");
        assert_eq!(ProcessState::Sleeping.to_string(), "sleeping");
        assert_eq!(ProcessState::Gone.to_string(), "gone");
        assert_eq!(
            ProcessState::Other("zombie".into()).to_string(),
            "other(zombie)"
        );
    }

    #[test]
    fn state_serde_shape_and_roundtrip() {
        assert_eq!(
            serde_json::to_value(ProcessState::Running).unwrap(),
            json!("Running")
        );
        assert_eq!(
            serde_json::to_value(ProcessState::Other("zombie".into())).unwrap(),
            json!({ "Other": "zombie" })
        );
        for s in [
            ProcessState::Running,
            ProcessState::Sleeping,
            ProcessState::Gone,
            ProcessState::Other("stopped".into()),
        ] {
            let back: ProcessState =
                serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
            assert_eq!(back, s);
        }
    }

    // ---- FDType ------------------------------------------------------------

    #[test]
    fn fd_type_display_and_serde() {
        // FDType has no PartialEq, so compare strings.
        let cases = [
            (FDType::File, "file", "File"),
            (FDType::Socket, "socket", "Socket"),
            (FDType::Pipe, "pipe", "Pipe"),
            (FDType::Other, "other", "Other"),
        ];
        for (t, display, wire) in cases {
            assert_eq!(t.to_string(), display);
            assert_eq!(serde_str(&t), wire);
            assert_roundtrip(&t);
        }
    }

    // ---- ProcessesSortKey --------------------------------------------------

    #[test]
    fn sort_key_try_from_is_case_insensitive() {
        assert_eq!(
            ProcessesSortKey::try_from("memory"),
            Ok(ProcessesSortKey::Memory)
        );
        assert_eq!(
            ProcessesSortKey::try_from("MEMORY"),
            Ok(ProcessesSortKey::Memory)
        );
        assert_eq!(ProcessesSortKey::try_from("Cpu"), Ok(ProcessesSortKey::Cpu));
        assert_eq!(
            ProcessesSortKey::try_from("dIsK"),
            Ok(ProcessesSortKey::Disk)
        );
    }

    #[test]
    fn sort_key_display_roundtrips_through_try_from() {
        for k in [
            ProcessesSortKey::Memory,
            ProcessesSortKey::Cpu,
            ProcessesSortKey::Disk,
        ] {
            assert_eq!(ProcessesSortKey::try_from(k.to_string().as_str()), Ok(k));
        }
    }

    #[test]
    fn sort_key_serde_is_lowercase() {
        assert_eq!(serde_str(&ProcessesSortKey::Memory), "memory");
        assert_eq!(serde_str(&ProcessesSortKey::Cpu), "cpu");
        assert_eq!(serde_str(&ProcessesSortKey::Disk), "disk");
        for k in [
            ProcessesSortKey::Memory,
            ProcessesSortKey::Cpu,
            ProcessesSortKey::Disk,
        ] {
            let back: ProcessesSortKey =
                serde_json::from_value(serde_json::to_value(k).unwrap()).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn sort_key_display_only_differs_from_serde_by_case() {
        // Display is capitalised ("Memory"), serde is lowercase ("memory").
        // If Display is ever used to build a URL/query string this matters.
        for k in [
            ProcessesSortKey::Memory,
            ProcessesSortKey::Cpu,
            ProcessesSortKey::Disk,
        ] {
            assert_eq!(k.to_string().to_lowercase(), serde_str(&k));
        }
    }

    #[test]
    fn sort_key_error_message_lowercases_input() {
        assert_eq!(
            ProcessesSortKey::try_from("GPU"),
            Err("invalid sort key 'gpu', expected: cpu, memory, disk".to_string())
        );
        assert!(ProcessesSortKey::try_from("").is_err());
    }

    // ---- ProcessSnapshot ---------------------------------------------------

    fn snapshot() -> ProcessSnapshot {
        ProcessSnapshot {
            pid: 42,
            name: "bash".into(),
            state: ProcessState::Running,
            cpu_usage: 1.5,
            memory_bytes: 1024,
            disk_usage: 0,
            cwd: None,
            cmdline: vec![],
            open_files: vec![],
            io_stats: None,
        }
    }

    #[test]
    fn snapshot_omits_empty_linux_only_fields() {
        let v = serde_json::to_value(snapshot()).unwrap();
        let obj = v.as_object().unwrap();
        for key in ["cwd", "cmdline", "open_files", "io_stats"] {
            assert!(
                !obj.contains_key(key),
                "'{key}' should be skipped when empty"
            );
        }
        for key in [
            "pid",
            "name",
            "state",
            "cpu_usage",
            "memory_bytes",
            "disk_usage",
        ] {
            assert!(obj.contains_key(key), "'{key}' must always be present");
        }
    }

    #[test]
    fn snapshot_includes_populated_linux_only_fields_and_roundtrips() {
        let mut s = snapshot();
        s.cwd = Some("/tmp".into());
        s.cmdline = vec!["bash".into(), "-l".into()];
        s.open_files = vec![FileDescriptorInfo {
            fd: 3,
            target: "/tmp/x".into(),
            fd_type: FDType::File,
        }];
        s.io_stats = Some(IOStats {
            read_bytes: 1,
            write_bytes: 2,
            read_chars: 3,
            write_chars: 4,
        });

        let v = serde_json::to_value(&s).unwrap();
        let obj = v.as_object().unwrap();
        for key in ["cwd", "cmdline", "open_files", "io_stats"] {
            assert!(obj.contains_key(key), "'{key}' should be present");
        }
        assert_roundtrip(&s);
    }

    #[test]
    fn snapshot_deserializes_when_linux_only_fields_are_missing() {
        // e.g. a payload coming from a Windows host
        let v = json!({
            "pid": 1,
            "name": "init",
            "state": "Sleeping",
            "cpu_usage": 0.0,
            "memory_bytes": 0,
            "disk_usage": 0
        });
        let s: ProcessSnapshot = serde_json::from_value(v).unwrap();
        assert_eq!(s.pid, 1);
        assert_eq!(s.state, ProcessState::Sleeping);
        assert!(s.cwd.is_none());
        assert!(s.cmdline.is_empty());
        assert!(s.open_files.is_empty());
        assert!(s.io_stats.is_none());
    }

    #[test]
    fn snapshot_display() {
        let s = snapshot().to_string();
        assert!(s.starts_with("[42] bash"), "got: {s}");
        assert!(s.contains("state=running"), "got: {s}");
        assert!(s.contains("cpu=1.5%"), "got: {s}");
        assert!(s.contains("mem="), "got: {s}");
        assert!(s.contains("disk="), "got: {s}");
    }

    // ---- ProcessStatus / ProcessTree ---------------------------------------

    #[test]
    fn status_display_with_unknowns() {
        assert_eq!(
            ProcessStatus::default().to_string(),
            "[?] ?  alive=false children=0 work_done=false"
        );
    }

    #[test]
    fn status_display_with_values() {
        let s = ProcessStatus {
            root_alive: true,
            root_pid: Some(7),
            root_name: Some("sh".into()),
            child_count: 2,
            work_done: false,
            timestamp: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(
            s.to_string(),
            "[7] sh  alive=true children=2 work_done=false"
        );
        assert_roundtrip(&s);
    }

    #[test]
    fn tree_display_when_root_exited() {
        let t = ProcessTree {
            root_pid: 1,
            root: None,
            children: vec![],
            child_count: 0,
            work_done: false,
            timestamp: String::new(),
        };
        assert_eq!(
            t.to_string(),
            "root_pid: 1  children: 0  work_done: false\n  root: (exited)\n"
        );
    }

    #[test]
    fn tree_display_with_root_and_child() {
        let root = snapshot();
        let mut child = snapshot();
        child.pid = 43;
        child.name = "sleep".into();

        let t = ProcessTree {
            root_pid: 42,
            root: Some(root.clone()),
            children: vec![child.clone()],
            child_count: 1,
            work_done: true,
            timestamp: "t".into(),
        };
        let expected =
            format!("root_pid: 42  children: 1  work_done: true\n  root: {root}\n    {child}\n");
        assert_eq!(t.to_string(), expected);
        assert_roundtrip(&t);
    }
}

// ---------------------------------------------------------------------------
// systemd.rs
// ---------------------------------------------------------------------------

mod systemd {
    use super::*;
    use kw_types::systemd::{
        ServiceAction, SystemdSnapshot, UnitActiveState, UnitLoadState, UnitSnapshot, UnitType,
    };

    // ---- UnitLoadState -----------------------------------------------------

    // (variant, Display / systemd spelling, serde spelling)
    fn load_states() -> [(UnitLoadState, &'static str, &'static str); 5] {
        [
            (UnitLoadState::Loaded, "loaded", "loaded"),
            (UnitLoadState::NotFound, "not-found", "not_found"),
            (UnitLoadState::BadSetting, "bad-setting", "bad_setting"),
            (UnitLoadState::Error, "error", "error"),
            (UnitLoadState::Masked, "masked", "masked"),
        ]
    }

    #[test]
    fn load_state_display_uses_systemd_spelling() {
        for (v, display, _) in load_states() {
            assert_eq!(v.to_string(), display);
        }
    }

    #[test]
    fn load_state_serde_uses_snake_case() {
        for (v, _, wire) in load_states() {
            assert_eq!(serde_str(&v), wire);
            let back: UnitLoadState =
                serde_json::from_value(serde_json::to_value(&v).unwrap()).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn load_state_from_str_roundtrips_display() {
        for (v, display, _) in load_states() {
            assert_eq!(UnitLoadState::from(display), v);
        }
    }

    #[test]
    fn load_state_from_str_falls_back_to_not_found() {
        assert_eq!(UnitLoadState::from("garbage"), UnitLoadState::NotFound);
        assert_eq!(UnitLoadState::from(""), UnitLoadState::NotFound);
        assert_eq!(UnitLoadState::from("LOADED"), UnitLoadState::NotFound);
    }

    // ---- UnitActiveState ---------------------------------------------------

    fn active_states() -> [(UnitActiveState, &'static str); 6] {
        [
            (UnitActiveState::Active, "active"),
            (UnitActiveState::Reloading, "reloading"),
            (UnitActiveState::Inactive, "inactive"),
            (UnitActiveState::Failed, "failed"),
            (UnitActiveState::Activating, "activating"),
            (UnitActiveState::Deactivating, "deactivating"),
        ]
    }

    #[test]
    fn active_state_as_str_matches_serde() {
        for (v, s) in active_states() {
            assert_eq!(v.as_str(), s);
            assert_eq!(serde_str(&v), s);
        }
    }

    #[test]
    fn active_state_from_str_roundtrips() {
        for (v, s) in active_states() {
            assert_eq!(UnitActiveState::from(s), v);
        }
    }

    #[test]
    fn active_state_from_str_unknown_becomes_inactive() {
        // systemd can also report e.g. "maintenance"; it collapses to Inactive today.
        assert_eq!(
            UnitActiveState::from("maintenance"),
            UnitActiveState::Inactive
        );
        assert_eq!(UnitActiveState::from(""), UnitActiveState::Inactive);
        assert_eq!(UnitActiveState::from("ACTIVE"), UnitActiveState::Inactive);
    }

    // ---- UnitType ----------------------------------------------------------

    #[test]
    fn unit_type_from_name_known_suffixes() {
        let cases = [
            ("nginx.service", UnitType::Service),
            ("dbus.socket", UnitType::Socket),
            ("multi-user.target", UnitType::Target),
            ("apt-daily.timer", UnitType::Timer),
            ("home.mount", UnitType::Mount),
            ("home.automount", UnitType::Mount),
            ("sys-devices-foo.device", UnitType::Device),
        ];
        for (name, expected) in cases {
            assert_eq!(UnitType::from_name(name), expected, "name: {name}");
        }
    }

    #[test]
    fn unit_type_from_name_uses_last_dot() {
        assert_eq!(UnitType::from_name("a.b.service"), UnitType::Service);
        assert_eq!(UnitType::from_name("user@1000.service"), UnitType::Service);
    }

    #[test]
    fn unit_type_from_name_unknown_and_malformed() {
        assert_eq!(
            UnitType::from_name("user.slice"),
            UnitType::Other("slice".into())
        );
        assert_eq!(UnitType::from_name("nginx"), UnitType::Other(String::new()));
        assert_eq!(UnitType::from_name(""), UnitType::Other(String::new()));
        assert_eq!(
            UnitType::from_name("trailing."),
            UnitType::Other(String::new())
        );
    }

    #[test]
    fn unit_type_display_roundtrips_through_from_name() {
        let kinds = [
            UnitType::Service,
            UnitType::Socket,
            UnitType::Target,
            UnitType::Timer,
            UnitType::Mount,
            UnitType::Device,
            UnitType::Other("slice".into()),
        ];
        for k in kinds {
            let name = format!("x.{k}");
            assert_eq!(UnitType::from_name(&name), k, "name: {name}");
        }
    }

    #[test]
    fn unit_type_serde_shape_and_roundtrip() {
        assert_eq!(serde_str(&UnitType::Service), "service");
        assert_eq!(
            serde_json::to_value(UnitType::Other("slice".into())).unwrap(),
            json!({ "other": "slice" })
        );
        for k in [
            UnitType::Service,
            UnitType::Timer,
            UnitType::Other("scope".into()),
        ] {
            let back: UnitType = serde_json::from_value(serde_json::to_value(&k).unwrap()).unwrap();
            assert_eq!(back, k);
        }
    }

    // ---- ServiceAction -----------------------------------------------------

    fn actions() -> [(ServiceAction, &'static str); 4] {
        [
            (ServiceAction::Start, "start"),
            (ServiceAction::Stop, "stop"),
            (ServiceAction::Restart, "restart"),
            (ServiceAction::Reload, "reload"),
        ]
    }

    #[test]
    fn service_action_as_str_matches_serde_and_try_from() {
        for (a, s) in actions() {
            assert_eq!(a.as_str(), s);
            assert_eq!(serde_str(&a), s);
            assert_eq!(ServiceAction::try_from(s), Ok(a));
            let back: ServiceAction =
                serde_json::from_value(serde_json::to_value(a).unwrap()).unwrap();
            assert_eq!(back, a);
        }
    }

    #[test]
    fn service_action_try_from_rejects_unknown_and_wrong_case() {
        assert_eq!(
            ServiceAction::try_from("bogus"),
            Err("Invalid action: 'bogus'.".to_string())
        );
        for bad in ["Start", "STOP", "", " restart", "enable", "kill"] {
            assert!(ServiceAction::try_from(bad).is_err(), "'{bad}' should fail");
        }
    }

    // ---- UnitSnapshot / SystemdSnapshot ------------------------------------

    fn unit() -> UnitSnapshot {
        UnitSnapshot {
            unit_name: "nginx.service".into(),
            unit_type: UnitType::Service,
            load_state: UnitLoadState::Loaded,
            active_state: UnitActiveState::Active,
            sub_state: "running".into(),
            description: "A web server".into(),
            main_pid: None,
            memory_bytes: None,
            cpu_usage_ns: None,
            restart_count: None,
            since: None,
            fragment_path: None,
        }
    }

    fn unit_base_line() -> String {
        format!("{:<40}  {:>12}  {}", "nginx.service", "active", "running")
    }

    #[test]
    fn unit_display_minimal() {
        assert_eq!(unit().to_string(), unit_base_line());
    }

    #[test]
    fn unit_display_with_pid() {
        let mut u = unit();
        u.main_pid = Some(123);
        assert_eq!(u.to_string(), format!("{}  pid=123", unit_base_line()));
    }

    #[test]
    fn unit_display_with_pid_and_memory() {
        let mut u = unit();
        u.main_pid = Some(123);
        u.memory_bytes = Some(2048);
        let s = u.to_string();
        assert!(
            s.starts_with(&format!("{}  pid=123", unit_base_line())),
            "got: {s}"
        );
        assert!(s.contains("  mem="), "got: {s}");
    }

    #[test]
    fn unit_display_pads_name_column() {
        let s = unit().to_string();
        // name column is 40 wide + 2 spaces separator before the state column
        assert_eq!(&s[..13], "nginx.service");
        assert_eq!(&s[13..42], " ".repeat(29));
    }

    #[test]
    fn unit_serde_roundtrip_minimal_and_full() {
        assert_roundtrip(&unit());

        let mut u = unit();
        u.main_pid = Some(123);
        u.memory_bytes = Some(2048);
        u.cpu_usage_ns = Some(999);
        u.restart_count = Some(3);
        u.since = Some("2026-01-01T00:00:00Z".into());
        u.fragment_path = Some("/lib/systemd/system/nginx.service".into());
        assert_roundtrip(&u);
    }

    #[test]
    fn systemd_snapshot_display_empty() {
        let s = SystemdSnapshot {
            timestamp: "2026-01-01T00:00:00Z".into(),
            units: vec![],
            failed_count: 3,
            active_count: 1,
            inactive_count: 2,
        };
        assert_eq!(
            s.to_string(),
            "Systemd  2026-01-01T00:00:00Z  active=1  inactive=2  failed=3\n"
        );
    }

    #[test]
    fn systemd_snapshot_display_lists_units_indented() {
        let s = SystemdSnapshot {
            timestamp: "t".into(),
            units: vec![unit()],
            failed_count: 0,
            active_count: 1,
            inactive_count: 0,
        };
        let expected = format!(
            "Systemd  t  active=1  inactive=0  failed=0\n  {}\n",
            unit_base_line()
        );
        assert_eq!(s.to_string(), expected);
        assert_roundtrip(&s);
    }
}

// ---------------------------------------------------------------------------
// docker.rs (everything that doesn't need the `server` feature)
// ---------------------------------------------------------------------------

mod docker {
    use super::*;
    use kw_types::docker::{
        ContainerHealth, ContainerSnapshot, ContainerStats, ContainerStatus, DockerSortKey,
    };

    // ---- ContainerStatus ---------------------------------------------------

    fn statuses() -> [(ContainerStatus, &'static str); 8] {
        [
            (ContainerStatus::Created, "created"),
            (ContainerStatus::Running, "running"),
            (ContainerStatus::Paused, "paused"),
            (ContainerStatus::Restarting, "restarting"),
            (ContainerStatus::Removing, "removing"),
            (ContainerStatus::Exited, "exited"),
            (ContainerStatus::Dead, "dead"),
            (ContainerStatus::Stopping, "stopping"),
        ]
    }

    #[test]
    fn status_display_matches_serde_for_unit_variants() {
        for (v, s) in statuses() {
            assert_eq!(v.to_string(), s);
            assert_eq!(serde_str(&v), s);
            let back: ContainerStatus =
                serde_json::from_value(serde_json::to_value(&v).unwrap()).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn status_unknown_variant() {
        let u = ContainerStatus::Unknown("weird".into());
        assert_eq!(u.to_string(), "unknown(weird)");
        assert_eq!(
            serde_json::to_value(&u).unwrap(),
            json!({ "unknown": "weird" })
        );
        let back: ContainerStatus = serde_json::from_value(json!({ "unknown": "weird" })).unwrap();
        assert_eq!(back, u);
    }

    // ---- ContainerHealth ---------------------------------------------------

    #[test]
    fn health_from_status_str_real_docker_strings() {
        let cases = [
            ("Up 3 hours (healthy)", ContainerHealth::Healthy),
            ("Up 5 minutes (unhealthy)", ContainerHealth::Unhealthy),
            (
                "Up 10 seconds (health: starting)",
                ContainerHealth::Starting,
            ),
            ("Up 2 days", ContainerHealth::None),
            ("Exited (0) 2 hours ago", ContainerHealth::None),
            ("Created", ContainerHealth::None),
            ("", ContainerHealth::None),
        ];
        for (input, expected) in cases {
            assert_eq!(
                ContainerHealth::from_status_str(input),
                expected,
                "input: '{input}'"
            );
        }
    }

    #[test]
    fn health_from_status_str_is_case_insensitive() {
        assert_eq!(
            ContainerHealth::from_status_str("UP 1 HOUR (HEALTHY)"),
            ContainerHealth::Healthy
        );
        assert_eq!(
            ContainerHealth::from_status_str("Up 1 hour (Unhealthy)"),
            ContainerHealth::Unhealthy
        );
    }

    #[test]
    fn health_unhealthy_is_not_mistaken_for_healthy() {
        // "(unhealthy)" must not match the "(healthy)" check.
        assert_eq!(
            ContainerHealth::from_status_str("Up 1 minute (unhealthy)"),
            ContainerHealth::Unhealthy
        );
    }

    #[test]
    fn health_display_matches_serde() {
        let all = [
            (ContainerHealth::None, "none"),
            (ContainerHealth::Starting, "starting"),
            (ContainerHealth::Healthy, "healthy"),
            (ContainerHealth::Unhealthy, "unhealthy"),
            (ContainerHealth::Unknown, "unknown"),
        ];
        for (v, s) in all {
            assert_eq!(v.to_string(), s);
            assert_eq!(serde_str(&v), s);
            let back: ContainerHealth =
                serde_json::from_value(serde_json::to_value(&v).unwrap()).unwrap();
            assert_eq!(back, v);
        }
    }

    // ---- DockerSortKey -----------------------------------------------------

    #[test]
    fn sort_key_try_from_is_case_insensitive() {
        assert_eq!(DockerSortKey::try_from("cpu"), Ok(DockerSortKey::Cpu));
        assert_eq!(DockerSortKey::try_from("CPU"), Ok(DockerSortKey::Cpu));
        assert_eq!(DockerSortKey::try_from("Memory"), Ok(DockerSortKey::Memory));
    }

    #[test]
    fn sort_key_error_message() {
        assert_eq!(
            DockerSortKey::try_from("FOO"),
            Err("invalid sort key 'foo', expected: cpu, memory".to_string())
        );
        // "disk" is valid for processes but not for containers
        assert!(DockerSortKey::try_from("disk").is_err());
    }

    #[test]
    fn sort_key_display_matches_serde_and_roundtrips() {
        for k in [DockerSortKey::Cpu, DockerSortKey::Memory] {
            assert_eq!(k.to_string(), serde_str(&k));
            assert_eq!(DockerSortKey::try_from(k.to_string().as_str()), Ok(k));
            let back: DockerSortKey =
                serde_json::from_value(serde_json::to_value(k).unwrap()).unwrap();
            assert_eq!(back, k);
        }
    }

    // ---- ContainerSnapshot / ContainerStats --------------------------------

    fn stats() -> ContainerStats {
        ContainerStats {
            cpu_percent: 12.5,
            memory_bytes: 512 * 1024 * 1024,
            memory_limit_bytes: 1024 * 1024 * 1024,
            memory_percent: Some(0.5),
            net_rx_bytes: 100,
            net_tx_bytes: 200,
            block_read_bytes: 300,
            block_write_bytes: 400,
            pid_count: 7,
        }
    }

    fn container(stats: Option<ContainerStats>) -> ContainerSnapshot {
        ContainerSnapshot {
            id: "abc123def4567890".into(),
            short_id: "abc123def456".into(),
            name: "web".into(),
            image: "nginx:latest".into(),
            status: ContainerStatus::Running,
            health: ContainerHealth::Healthy,
            stats,
        }
    }

    #[test]
    fn container_display_without_stats() {
        assert_eq!(
            container(None).to_string(),
            "web (abc123def456)  running  image=nginx:latest  health=healthy"
        );
    }

    #[test]
    fn container_display_with_stats_and_memory_percent() {
        let s = container(Some(stats())).to_string();
        assert!(
            s.starts_with("web (abc123def456)  running  image=nginx:latest  health=healthy"),
            "got: {s}"
        );
        assert!(s.contains("  cpu=12.5%"), "got: {s}");
        assert!(s.contains("  mem="), "got: {s}");
        // memory_percent is a 0..1 fraction and is printed as a percentage
        assert!(s.ends_with(" (50.0%)"), "got: {s}");
    }

    #[test]
    fn container_display_without_memory_percent() {
        let mut st = stats();
        st.memory_limit_bytes = 0;
        st.memory_percent = None;
        let s = container(Some(st)).to_string();
        assert!(s.contains("  cpu=12.5%"), "got: {s}");
        assert!(s.contains("  mem="), "got: {s}");
        assert!(!s.contains("50.0%"), "got: {s}");
    }

    #[test]
    fn container_serde_roundtrip() {
        assert_roundtrip(&container(None));
        assert_roundtrip(&container(Some(stats())));
        let mut st = stats();
        st.memory_percent = None;
        assert_roundtrip(&st);
    }

    #[test]
    fn container_stats_none_fields_serialize_as_null() {
        // memory_percent has no skip_serializing_if, so clients (e.g. the Svelte
        // dashboard) can rely on the key being present.
        let mut st = stats();
        st.memory_percent = None;
        let v = serde_json::to_value(&st).unwrap();
        assert_eq!(v.get("memory_percent"), Some(&Value::Null));

        let c = serde_json::to_value(container(None)).unwrap();
        assert_eq!(c.get("stats"), Some(&Value::Null));
    }
}
