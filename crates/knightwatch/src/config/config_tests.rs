#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashSet, path::PathBuf, time::Duration};

use super::{
    AppConfig,
    args::{CliArgs, Command, ConfigAction, ConfigField, UsersAction},
    paths,
    persistent::PersistentConfig,
    store::JsonStore,
    users::{DisplayUser, User, Users, generate_telegram_token, get_users, hash_password},
};
use crate::errors::Error;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn config_err<T: std::fmt::Debug>(result: Result<T, Error>) -> String {
    match result {
        Err(Error::Config(text)) => text,
        other => panic!("expected Error::Config, got {other:?}"),
    }
}

fn try_parse(args: &[&str]) -> Result<CliArgs, clap::Error> {
    CliArgs::try_parse_from(std::iter::once("knightwatch").chain(args.iter().copied()))
}

fn parse(args: &[&str]) -> CliArgs {
    try_parse(args).unwrap()
}

fn user(name: &str) -> User {
    User {
        username: name.into(),
        password_hash: "not-a-real-hash".into(),
        telegram_token: format!("token-{name}"),
        telegram_chat_id: None,
    }
}

/// A user with a real bcrypt hash (minimum cost, so tests stay fast).
fn user_with_password(name: &str, password: &str) -> User {
    User {
        password_hash: bcrypt::hash(password, 4).unwrap(),
        ..user(name)
    }
}

fn users(list: Vec<User>) -> Users {
    Users { users: list }
}

// ---------------------------------------------------------------------------
// Users: lookup
// ---------------------------------------------------------------------------

#[test]
fn find_returns_the_matching_user() {
    let list = users(vec![user("alice"), user("bob")]);
    assert_eq!(list.find("bob").unwrap().username, "bob");
    assert!(list.find("carol").is_none());
    assert!(list.find("").is_none());
}

// Pinned: usernames are case-sensitive, so "Bob" and "bob" are different
// accounts.
#[test]
fn find_is_case_sensitive() {
    let list = users(vec![user("bob")]);
    assert!(list.find("Bob").is_none());
    assert!(list.find("BOB").is_none());
}

#[test]
fn find_by_telegram_token_mut_finds_and_allows_changes() {
    let mut list = users(vec![user("alice"), user("bob")]);
    list.find_by_telegram_token_mut("token-bob")
        .unwrap()
        .telegram_chat_id = Some(42);
    assert_eq!(list.find("bob").unwrap().telegram_chat_id, Some(42));
    assert_eq!(list.find("alice").unwrap().telegram_chat_id, None);
    assert!(list.find_by_telegram_token_mut("nope").is_none());
}

#[test]
fn find_by_telegram_chat_id_matches_only_linked_users() {
    let mut linked = user("bob");
    linked.telegram_chat_id = Some(7);
    let list = users(vec![user("alice"), linked]);
    assert_eq!(list.find_by_telegram_chat_id(7).unwrap().username, "bob");
    assert!(list.find_by_telegram_chat_id(8).is_none());
}

#[test]
fn get_telegram_chat_ids_collects_only_linked_users_in_order() {
    let mut a = user("a");
    a.telegram_chat_id = Some(3);
    let b = user("b");
    let mut c = user("c");
    c.telegram_chat_id = Some(1);
    assert_eq!(users(vec![a, b, c]).get_telegram_chat_ids(), vec![3, 1]);
    assert!(users(vec![]).get_telegram_chat_ids().is_empty());
}

// ---------------------------------------------------------------------------
// Users: add / remove
// ---------------------------------------------------------------------------

#[test]
fn add_appends_a_new_user() {
    let mut list = users(vec![]);
    list.add(user("bob")).unwrap();
    assert_eq!(list.users.len(), 1);
    assert!(list.find("bob").is_some());
}

#[test]
fn add_rejects_a_duplicate_username() {
    let mut list = users(vec![user("bob")]);
    let text = config_err(list.add(user("bob")));
    assert_eq!(text, "User 'bob' already exists");
    assert_eq!(list.users.len(), 1);
}

// Pinned: "Bob" and "bob" can both exist (follows from case-sensitive lookup),
// and an empty username is accepted.
#[test]
fn add_allows_case_variants_and_empty_username() {
    let mut list = users(vec![user("bob")]);
    list.add(user("Bob")).unwrap();
    list.add(user("")).unwrap();
    assert_eq!(list.users.len(), 3);
}

#[test]
fn remove_deletes_only_the_named_user() {
    let mut list = users(vec![user("alice"), user("bob"), user("carol")]);
    list.remove("bob").unwrap();
    let names: Vec<&str> = list.users.iter().map(|u| u.username.as_str()).collect();
    assert_eq!(names, vec!["alice", "carol"]);
}

#[test]
fn remove_unknown_user_is_a_config_error() {
    let mut list = users(vec![user("alice")]);
    assert_eq!(config_err(list.remove("bob")), "User 'bob' not found");
    assert_eq!(list.users.len(), 1);
}

// ---------------------------------------------------------------------------
// Users: password check
// ---------------------------------------------------------------------------

#[test]
fn verify_password_accepts_the_right_password() {
    let list = users(vec![user_with_password("bob", "correct horse")]);
    assert!(list.verify_password("bob", "correct horse").unwrap());
}

#[test]
fn verify_password_rejects_a_wrong_or_empty_password() {
    let list = users(vec![user_with_password("bob", "correct horse")]);
    assert!(!list.verify_password("bob", "wrong").unwrap());
    assert!(!list.verify_password("bob", "").unwrap());
    assert!(!list.verify_password("bob", "Correct Horse").unwrap());
}

// Unknown user is an error, wrong password is `Ok(false)`. The socket login
// treats both as WrongCredentials, so the difference is not visible to clients.
#[test]
fn verify_password_unknown_user_is_a_config_error() {
    let list = users(vec![user_with_password("bob", "pw")]);
    assert_eq!(
        config_err(list.verify_password("alice", "pw")),
        "User 'alice' not found"
    );
}

#[test]
fn verify_password_with_a_broken_hash_is_a_config_error() {
    let list = users(vec![user("bob")]);
    let text = config_err(list.verify_password("bob", "pw"));
    assert!(text.starts_with("Failed to verify password"), "{text}");
}

#[test]
fn hash_password_makes_verifiable_salted_bcrypt_hashes() {
    let first = hash_password("secret").unwrap();
    let second = hash_password("secret").unwrap();
    assert!(first.starts_with("$2"), "{first}");
    assert!(
        first.contains(&format!("${:02}$", bcrypt::DEFAULT_COST)),
        "{first}"
    );
    assert_ne!(first, second, "salts should differ");
    assert!(bcrypt::verify("secret", &first).unwrap());
    assert!(!bcrypt::verify("other", &first).unwrap());
}

// ---------------------------------------------------------------------------
// Users: tokens, serde, DisplayUser
// ---------------------------------------------------------------------------

#[test]
fn generated_telegram_tokens_are_six_to_eight_digits() {
    let mut seen = HashSet::new();
    for _ in 0..200 {
        let token = generate_telegram_token();
        assert!((6..=8).contains(&token.len()), "{token}");
        assert!(token.bytes().all(|b| b.is_ascii_digit()), "{token}");
        let number: u32 = token.parse().unwrap();
        assert!((100_000..=99_999_999).contains(&number), "{token}");
        seen.insert(token);
    }
    assert!(seen.len() > 1, "tokens should be random");
}

#[test]
fn users_json_shape() {
    let list = users(vec![User {
        username: "bob".into(),
        password_hash: "h".into(),
        telegram_token: "123456".into(),
        telegram_chat_id: None,
    }]);
    assert_eq!(
        serde_json::to_value(&list).unwrap(),
        serde_json::json!({"users": [{
            "username": "bob",
            "password_hash": "h",
            "telegram_token": "123456",
            "telegram_chat_id": null
        }]})
    );
}

#[test]
fn users_json_without_chat_id_still_parses() {
    let parsed: Users = serde_json::from_str(
        r#"{"users":[{"username":"bob","password_hash":"h","telegram_token":"1"}]}"#,
    )
    .unwrap();
    assert_eq!(parsed.users[0].telegram_chat_id, None);
}

// Pinned: `users` has no serde default, so a file containing just `{}` fails
// to load (and the server would refuse to start). Should flip if a default
// is added.
#[test]
fn users_json_without_users_key_is_rejected() {
    assert!(serde_json::from_str::<Users>("{}").is_err());
}

#[test]
fn display_user_copies_only_name_and_chat_id() {
    let mut full = user_with_password("bob", "pw");
    full.telegram_chat_id = Some(42);
    let from_ref = DisplayUser::from(&full);
    let from_owned = DisplayUser::from(full);
    for display in [from_ref, from_owned] {
        assert_eq!(
            serde_json::to_value(&display).unwrap(),
            serde_json::json!({"username": "bob", "telegram_chat_id": 42})
        );
    }
}

#[test]
fn display_user_display_text() {
    let plain = DisplayUser {
        username: "bob".into(),
        telegram_chat_id: None,
    };
    assert_eq!(plain.to_string(), "username: bob");
    let linked = DisplayUser {
        username: "bob".into(),
        telegram_chat_id: Some(-100),
    };
    assert_eq!(linked.to_string(), "username: bob, chat_id: -100");
}

// Nothing in this test binary loads the users, so the global is still empty.
#[test]
fn global_users_are_not_loaded_in_tests() {
    assert!(get_users().is_none());
}

// Known issue (finding 24): `User` derives `Debug` with the password hash and
// the Telegram token in it, so any `{:?}` of a user or of `Users` logs them.
#[test]
#[ignore = "known issue (finding 24): User's Debug output includes the password hash and Telegram token"]
fn user_debug_must_not_print_secrets() {
    let secret = User {
        username: "bob".into(),
        password_hash: "hash-secret-value".into(),
        telegram_token: "99887766".into(),
        telegram_chat_id: None,
    };
    let text = format!("{secret:?}");
    assert!(!text.contains("hash-secret-value"), "{text}");
    assert!(!text.contains("99887766"), "{text}");
}

// ---------------------------------------------------------------------------
// JsonStore (through a test store whose path is set per thread)
// ---------------------------------------------------------------------------

thread_local! {
    static TEST_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

fn set_path(path: Option<PathBuf>) {
    TEST_PATH.with(|p| *p.borrow_mut() = path);
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct TestStore {
    name: String,
    count: u32,
}

impl JsonStore for TestStore {
    const NAME: &'static str = "test";
    fn path() -> Option<PathBuf> {
        TEST_PATH.with(|p| p.borrow().clone())
    }
}

fn sample_store() -> TestStore {
    TestStore {
        name: "a".into(),
        count: 3,
    }
}

#[test]
fn load_without_a_path_gives_the_default() {
    set_path(None);
    assert_eq!(TestStore::load().unwrap(), TestStore::default());
}

#[test]
fn load_of_a_missing_file_gives_the_default() {
    let dir = tempfile::tempdir().unwrap();
    set_path(Some(dir.path().join("missing.json")));
    assert_eq!(TestStore::load().unwrap(), TestStore::default());
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    set_path(Some(dir.path().join("store.json")));
    sample_store().save().unwrap();
    assert_eq!(TestStore::load().unwrap(), sample_store());
}

#[test]
fn save_creates_missing_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("store.json");
    set_path(Some(path.clone()));
    sample_store().save().unwrap();
    assert!(path.is_file());
}

#[test]
fn save_writes_pretty_json_and_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    set_path(Some(path.clone()));
    TestStore {
        name: "old".into(),
        count: 1,
    }
    .save()
    .unwrap();
    sample_store().save().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, "{\n  \"name\": \"a\",\n  \"count\": 3\n}");
}

#[test]
fn save_without_a_path_is_a_config_error() {
    set_path(None);
    assert_eq!(
        config_err(sample_store().save()),
        "Failed to determine path for test file"
    );
}

#[test]
fn save_under_a_regular_file_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    set_path(Some(blocker.join("child.json")));
    let text = config_err(sample_store().save());
    assert!(text.starts_with("Failed to create directory"), "{text}");
}

#[test]
fn load_of_invalid_json_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    std::fs::write(&path, "{not json").unwrap();
    set_path(Some(path));
    let text = config_err(TestStore::load());
    assert!(text.starts_with("Failed to parse test file"), "{text}");
}

#[test]
fn load_of_the_wrong_shape_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    std::fs::write(&path, r#"{"name": 5, "count": "x"}"#).unwrap();
    set_path(Some(path));
    let text = config_err(TestStore::load());
    assert!(text.starts_with("Failed to parse test file"), "{text}");
}

// Pinned (finding 25): an empty file (for example left by a crash during a
// non-atomic save) is a parse error, not a default, so startup would fail.
#[test]
fn load_of_an_empty_file_is_a_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    std::fs::write(&path, "").unwrap();
    set_path(Some(path));
    let text = config_err(TestStore::load());
    assert!(text.starts_with("Failed to parse test file"), "{text}");
}

#[test]
fn load_of_a_directory_is_a_read_error() {
    let dir = tempfile::tempdir().unwrap();
    set_path(Some(dir.path().to_path_buf()));
    let text = config_err(TestStore::load());
    assert!(text.starts_with("Failed to read test file"), "{text}");
}

// Pinned: unknown fields are ignored (no `deny_unknown_fields`).
#[test]
fn load_ignores_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    std::fs::write(&path, r#"{"name":"a","count":1,"extra":true}"#).unwrap();
    set_path(Some(path));
    assert_eq!(
        TestStore::load().unwrap(),
        TestStore {
            name: "a".into(),
            count: 1
        }
    );
}

// Known issue (finding 25): files are written with the default umask, so
// `users.json` (password hashes, Telegram tokens) is normally world-readable
// (0644). Wanted: owner-only (0600). Passes only under a restrictive umask.
#[cfg(unix)]
#[test]
#[ignore = "known issue (finding 25): saved files are not restricted to the owner"]
fn saved_file_is_private_to_the_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.json");
    set_path(Some(path.clone()));
    sample_store().save().unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "mode is {mode:o}");
}

// ---------------------------------------------------------------------------
// PersistentConfig and paths
// ---------------------------------------------------------------------------

#[test]
fn persistent_config_default_is_empty() {
    let config = PersistentConfig::default();
    assert!(config.telegram_token.is_none());
    assert!(config.webhook_urls.is_empty());
}

#[test]
fn persistent_config_missing_webhook_urls_defaults_to_empty() {
    let parsed: PersistentConfig = serde_json::from_str(r#"{"telegram_token":"t"}"#).unwrap();
    assert_eq!(parsed.telegram_token.as_deref(), Some("t"));
    assert!(parsed.webhook_urls.is_empty());
}

#[test]
fn persistent_config_missing_token_is_none_and_round_trips() {
    let parsed: PersistentConfig =
        serde_json::from_str(r#"{"webhook_urls":["http://a","http://b"]}"#).unwrap();
    assert!(parsed.telegram_token.is_none());
    let text = serde_json::to_string(&parsed).unwrap();
    let again: PersistentConfig = serde_json::from_str(&text).unwrap();
    assert_eq!(again.webhook_urls, vec!["http://a", "http://b"]);
}

// Known issue (finding 24): the bot token is part of the derived Debug output.
#[test]
#[ignore = "known issue (finding 24): PersistentConfig's Debug output includes the Telegram bot token"]
fn persistent_config_debug_must_not_print_the_bot_token() {
    let config = PersistentConfig {
        telegram_token: Some("123456:SECRET-BOT-TOKEN".into()),
        webhook_urls: vec![],
    };
    let text = format!("{config:?}");
    assert!(!text.contains("SECRET-BOT-TOKEN"), "{text}");
}

// The directories depend on the machine, so only the file names and the app
// name are checked. Skipped where no home directory exists.
#[test]
fn config_paths_end_with_the_requested_names() {
    if let Some(path) = paths::config_file_path("config.json") {
        assert_eq!(path.file_name().unwrap(), "config.json");
        assert!(
            path.to_string_lossy()
                .to_lowercase()
                .contains("knightwatch")
        );
    }
    if let Some(path) = paths::data_file_path("users.json") {
        assert_eq!(path.file_name().unwrap(), "users.json");
        assert!(
            path.to_string_lossy()
                .to_lowercase()
                .contains("knightwatch")
        );
    }
    if let Some(path) = paths::log_dir() {
        assert_eq!(path.file_name().unwrap(), "logs");
        assert!(
            path.to_string_lossy()
                .to_lowercase()
                .contains("knightwatch")
        );
    }
}

#[test]
fn persistent_config_and_users_use_their_own_files() {
    if let (Some(config), Some(users)) = (PersistentConfig::path(), Users::path()) {
        assert_eq!(config.file_name().unwrap(), "config.json");
        assert_eq!(users.file_name().unwrap(), "users.json");
    }
}

// ---------------------------------------------------------------------------
// CliArgs: defaults and flags
// ---------------------------------------------------------------------------

// Pinned (finding 27): by default the API, TCP and WebSocket servers bind to
// 0.0.0.0 and authentication is off. Dangerous commands are off by default.
#[test]
fn defaults() {
    let args = parse(&[]);
    assert!(args.command.is_none());
    assert_eq!(args.host, "0.0.0.0");
    assert_eq!(args.port, 8083);
    assert!(!args.enable_auth);
    assert!(!args.enable_shutdown);
    assert_eq!(args.session_idle_timeout_mins, 30);
    assert_eq!(args.session_ttl_hours, 12);
    assert!(!args.no_api);
    assert!(!args.no_dashboard);
    assert!(args.pid.is_empty());
    assert!(!args.telegram);
    assert!(args.webhook_urls.is_empty());
    assert!(!args.with_webhook);
    assert!(!args.top_processes);
    assert_eq!(args.limit_processes, 5);
    assert!(!args.system_resources);
    assert!(!args.docker);
    assert!(!args.tcp_socket);
    assert_eq!(args.tcp_host, "0.0.0.0");
    assert_eq!(args.tcp_port, 8084);
    assert!(!args.ws_socket);
    assert_eq!(args.ws_host, "0.0.0.0");
    assert_eq!(args.ws_port, 8085);
    assert!(!args.allow_process_commands);
    assert!(!args.allow_system_resources_commands);
    assert!(!args.allow_docker_commands);
}

#[test]
fn flags_and_values_are_parsed() {
    let args = parse(&[
        "--host",
        "127.0.0.1",
        "-p",
        "9000",
        "--enable-auth",
        "--enable-shutdown",
        "--telegram",
        "--with-webhook",
        "--top-processes",
        "--limit-processes",
        "3",
        "--system-resources",
        "--docker",
        "--tcp-socket",
        "--tcp-port",
        "7001",
        "--ws-socket",
        "--ws-host",
        "::1",
        "--allow-process-commands",
        "--allow-docker-commands",
    ]);
    assert_eq!(args.host, "127.0.0.1");
    assert_eq!(args.port, 9000);
    assert!(args.enable_auth);
    assert!(args.enable_shutdown);
    assert!(args.telegram);
    assert!(args.with_webhook);
    assert!(args.top_processes);
    assert_eq!(args.limit_processes, 3);
    assert!(args.system_resources);
    assert!(args.docker);
    assert!(args.tcp_socket);
    assert_eq!(args.tcp_port, 7001);
    assert!(args.ws_socket);
    assert_eq!(args.ws_host, "::1");
    assert!(args.allow_process_commands);
    assert!(args.allow_docker_commands);
}

#[test]
fn repeatable_options_collect_in_order() {
    let args = parse(&[
        "--pid",
        "10",
        "--pid",
        "2",
        "--webhook",
        "http://a",
        "--webhook",
        "http://b?webhook_events=tick",
    ]);
    assert_eq!(args.pid, vec![10, 2]);
    assert_eq!(
        args.webhook_urls,
        vec!["http://a", "http://b?webhook_events=tick"]
    );
}

#[test]
fn bad_values_are_rejected() {
    assert!(try_parse(&["--port", "abc"]).is_err());
    assert!(try_parse(&["--port", "70000"]).is_err());
    assert!(try_parse(&["--pid", "-1"]).is_err());
    assert!(try_parse(&["--limit-processes", "x"]).is_err());
    assert!(try_parse(&["--no-such-flag"]).is_err());
}

#[test]
fn server_address_joins_host_and_port() {
    let default = AppConfig {
        args: parse(&[]),
        persistent: PersistentConfig::default(),
    };
    assert_eq!(default.server_address(), "0.0.0.0:8083");
    let custom = AppConfig {
        args: parse(&["--host", "127.0.0.1", "--port", "9000"]),
        persistent: PersistentConfig::default(),
    };
    assert_eq!(custom.server_address(), "127.0.0.1:9000");
}

// Documents why handler tests cannot run in-process: the config is a
// write-once static that nothing in the test binary sets.
#[test]
#[should_panic(expected = "Config not initialized")]
fn get_config_panics_before_init() {
    let _ = super::get_config();
}

// ---------------------------------------------------------------------------
// CliArgs: session timeouts
// ---------------------------------------------------------------------------

#[test]
fn session_timeouts_default_values() {
    let (idle, ttl) = parse(&[]).session_timeouts();
    assert_eq!(idle, Duration::from_secs(30 * 60));
    assert_eq!(ttl, Duration::from_secs(12 * 3600));
}

#[test]
fn session_timeouts_use_the_given_values() {
    let (idle, ttl) = parse(&[
        "--session-idle-timeout-mins",
        "5",
        "--session-ttl-hours",
        "2",
    ])
    .session_timeouts();
    assert_eq!(idle, Duration::from_secs(300));
    assert_eq!(ttl, Duration::from_secs(7200));
}

// Pinned: 0 disables expiry by returning `Duration::MAX`. Anything that adds
// this to an `Instant` would overflow and panic (finding 26: check the session
// code when it is covered).
#[test]
fn session_timeouts_zero_means_unlimited() {
    let (idle, ttl) = parse(&[
        "--session-idle-timeout-mins",
        "0",
        "--session-ttl-hours",
        "0",
    ])
    .session_timeouts();
    assert_eq!(idle, Duration::MAX);
    assert_eq!(ttl, Duration::MAX);
}

#[test]
fn session_timeouts_zero_applies_to_each_value_separately() {
    let (idle, ttl) = parse(&["--session-idle-timeout-mins", "0"]).session_timeouts();
    assert_eq!(idle, Duration::MAX);
    assert_eq!(ttl, Duration::from_secs(12 * 3600));
}

// Known issue (finding 26): `mins * 60` and `hours * 3600` are plain u64
// multiplications, so a huge value panics in debug builds and wraps to a wrong
// (small) duration in release builds. Wanted: saturate (for example to
// `Duration::MAX`).
#[test]
#[ignore = "known issue (finding 26): session timeout arithmetic overflows for huge values"]
fn session_timeouts_huge_values_should_not_overflow() {
    let huge = u64::MAX.to_string();
    let (idle, ttl) = parse(&[
        "--session-idle-timeout-mins",
        &huge,
        "--session-ttl-hours",
        &huge,
    ])
    .session_timeouts();
    let hundred_years = Duration::from_secs(100 * 365 * 24 * 3600);
    assert!(idle > hundred_years, "{idle:?}");
    assert!(ttl > hundred_years, "{ttl:?}");
}

// ---------------------------------------------------------------------------
// CliArgs: cfg-gated helpers
// ---------------------------------------------------------------------------

#[test]
fn blind_helper_follows_the_screenshot_feature() {
    let args = parse(&[]);
    #[cfg(feature = "screenshot")]
    {
        assert!(!args.is_blind());
        assert!(parse(&["--blind"]).is_blind());
    }
    #[cfg(not(feature = "screenshot"))]
    {
        assert!(args.is_blind());
    }
}

#[test]
fn screen_commands_helper_follows_the_screenshot_feature() {
    let args = parse(&[]);
    #[cfg(feature = "screenshot")]
    {
        assert!(!args.is_screen_commands_allowed());
        assert!(parse(&["--allow-screen-commands"]).is_screen_commands_allowed());
    }
    #[cfg(not(feature = "screenshot"))]
    {
        assert!(!args.is_screen_commands_allowed());
    }
}

#[test]
fn systemd_helpers_follow_the_platform() {
    let args = parse(&[]);
    assert!(!args.is_systemd_enabled());
    assert!(!args.is_systemd_commands_allowed());
    #[cfg(target_os = "linux")]
    {
        let on = parse(&["--systemd", "--allow-systemd-commands"]);
        assert!(on.is_systemd_enabled());
        assert!(on.is_systemd_commands_allowed());
    }
}

// ---------------------------------------------------------------------------
// CliArgs: subcommands
// ---------------------------------------------------------------------------

#[test]
fn users_subcommands() {
    match parse(&["users", "add", "bob"]).command {
        Some(Command::Users {
            action: UsersAction::Add { username },
        }) => assert_eq!(username, "bob"),
        other => panic!("unexpected {other:?}"),
    }
    match parse(&["users", "remove", "bob"]).command {
        Some(Command::Users {
            action: UsersAction::Remove { username },
        }) => assert_eq!(username, "bob"),
        other => panic!("unexpected {other:?}"),
    }
    match parse(&["users", "token", "bob"]).command {
        Some(Command::Users {
            action: UsersAction::Token { username },
        }) => assert_eq!(username, "bob"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(
        parse(&["users", "list"]).command,
        Some(Command::Users {
            action: UsersAction::List
        })
    ));
    assert!(matches!(
        parse(&["users", "clear"]).command,
        Some(Command::Users {
            action: UsersAction::Clear
        })
    ));
}

#[test]
fn users_subcommand_errors() {
    assert!(try_parse(&["users", "add"]).is_err());
    assert!(try_parse(&["users", "remove"]).is_err());
    assert!(try_parse(&["users", "frobnicate"]).is_err());
    assert!(try_parse(&["users"]).is_err());
}

#[test]
fn global_flags_before_a_subcommand_still_apply() {
    let args = parse(&["--port", "9000", "users", "list"]);
    assert_eq!(args.port, 9000);
    assert!(matches!(args.command, Some(Command::Users { .. })));
}

#[test]
fn config_telegram_token_subcommands() {
    match parse(&["config", "set", "telegram-token", "abc"]).command {
        Some(Command::Config {
            action:
                ConfigAction::Set {
                    field: ConfigField::TelegramToken { value, clear },
                },
        }) => {
            assert_eq!(value.as_deref(), Some("abc"));
            assert!(!clear);
        }
        other => panic!("unexpected {other:?}"),
    }
    match parse(&["config", "set", "telegram-token", "--clear"]).command {
        Some(Command::Config {
            action:
                ConfigAction::Set {
                    field: ConfigField::TelegramToken { value, clear },
                },
        }) => {
            assert!(value.is_none());
            assert!(clear);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(
        parse(&["config", "get", "telegram-token"]).command,
        Some(Command::Config {
            action: ConfigAction::Get { .. }
        })
    ));
}

#[test]
fn config_webhook_urls_subcommand() {
    match parse(&[
        "config",
        "set",
        "webhook-urls",
        "--add",
        "http://a",
        "--add",
        "http://b",
        "--remove",
        "http://c",
    ])
    .command
    {
        Some(Command::Config {
            action:
                ConfigAction::Set {
                    field: ConfigField::WebhookUrls { add, remove, clear },
                },
        }) => {
            assert_eq!(add, vec!["http://a", "http://b"]);
            assert_eq!(remove, vec!["http://c"]);
            assert!(!clear);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn conflicting_config_options_are_rejected() {
    let kind = |args: &[&str]| try_parse(args).unwrap_err().kind();
    assert_eq!(
        kind(&["config", "set", "telegram-token", "abc", "--clear"]),
        clap::error::ErrorKind::ArgumentConflict
    );
    assert_eq!(
        kind(&[
            "config",
            "set",
            "webhook-urls",
            "--clear",
            "--add",
            "http://a"
        ]),
        clap::error::ErrorKind::ArgumentConflict
    );
    assert_eq!(
        kind(&[
            "config",
            "set",
            "webhook-urls",
            "--clear",
            "--remove",
            "http://a"
        ]),
        clap::error::ErrorKind::ArgumentConflict
    );
}
