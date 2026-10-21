#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use super::*;
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(30);
const TTL: Duration = Duration::from_secs(60);
const FOREVER: Duration = Duration::MAX;

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn ago(n: u64) -> Instant {
    Instant::now()
        .checked_sub(secs(n))
        .expect("the system clock is too young to go back that far")
}

fn fresh(user: &str, token: &str) -> Session {
    Session::new(user.to_string(), token.to_string())
}

/// A session created `created` seconds ago and last used `seen` seconds ago.
fn aged(user: &str, token: &str, created: u64, seen: u64) -> Session {
    let mut s = fresh(user, token);
    s.created_at = ago(created);
    s.last_seen = ago(seen);
    s
}

fn unique(tag: &str) -> String {
    format!("{tag}-{}", uuid::Uuid::new_v4())
}

fn sessions_with(list: Vec<Session>) -> Sessions {
    let mut sessions = Sessions::default();
    for s in list {
        sessions.insert(s);
    }
    sessions
}

fn is_present(sessions: &Sessions, token: &str) -> bool {
    sessions.inner.contains_key(token)
}

fn has_global(token: &str) -> bool {
    get_sessions().read().unwrap().inner.contains_key(token)
}

// ───────────────────────── Session ─────────────────────────

#[test]
fn new_session_keeps_username_and_token() {
    let s = fresh("alice", "tok");
    assert_eq!(s.username, "alice");
    assert_eq!(s.token, "tok");
}

#[test]
fn new_session_starts_with_equal_created_and_last_seen() {
    let s = fresh("alice", "tok");
    assert_eq!(s.created_at, s.last_seen);
}

#[test]
fn new_session_is_not_expired() {
    assert!(!fresh("alice", "tok").is_expired(IDLE, TTL));
}

#[test]
fn idle_longer_than_the_limit_expires() {
    assert!(aged("alice", "tok", 40, 40).is_expired(IDLE, TTL));
}

#[test]
fn idle_within_the_limit_stays_valid() {
    assert!(!aged("alice", "tok", 10, 10).is_expired(IDLE, TTL));
}

#[test]
fn age_beyond_the_ttl_expires_even_if_just_used() {
    assert!(aged("alice", "tok", 90, 0).is_expired(IDLE, TTL));
}

#[test]
fn age_and_idle_both_within_limits_stays_valid() {
    assert!(!aged("alice", "tok", 50, 10).is_expired(IDLE, TTL));
}

#[test]
fn forever_limits_never_expire() {
    assert!(!aged("alice", "tok", 90, 90).is_expired(FOREVER, FOREVER));
}

#[test]
fn forever_idle_still_enforces_the_ttl() {
    assert!(aged("alice", "tok", 90, 0).is_expired(FOREVER, TTL));
}

#[test]
fn forever_ttl_still_enforces_the_idle_limit() {
    assert!(aged("alice", "tok", 40, 40).is_expired(IDLE, FOREVER));
}

// ───────────────────────── Sessions: lookup ─────────────────────────

#[test]
fn get_valid_returns_the_session_for_its_token() {
    let mut s = sessions_with(vec![fresh("alice", "t1")]);
    let name = s.get_valid("t1", IDLE, TTL).map(|x| x.username.clone());
    assert_eq!(name.as_deref(), Some("alice"));
}

#[test]
fn get_valid_unknown_token_is_none() {
    let mut s = sessions_with(vec![fresh("alice", "t1")]);
    assert!(s.get_valid("nope", IDLE, TTL).is_none());
}

#[test]
fn get_valid_on_an_empty_store_is_none() {
    let mut s = Sessions::default();
    assert!(s.get_valid("t1", IDLE, TTL).is_none());
}

#[test]
fn get_valid_empty_token_is_none() {
    // `Authorization: Bearer ` (nothing after the prefix) ends up here.
    let mut s = sessions_with(vec![fresh("alice", "t1")]);
    assert!(s.get_valid("", IDLE, TTL).is_none());
}

#[test]
fn tokens_are_case_sensitive() {
    let mut s = sessions_with(vec![fresh("alice", "AbC")]);
    assert!(s.get_valid("abc", IDLE, TTL).is_none());
    assert!(s.get_valid("AbC", IDLE, TTL).is_some());
}

#[test]
fn get_valid_finds_the_right_session_among_several() {
    let mut s = sessions_with(vec![
        fresh("alice", "t1"),
        fresh("bob", "t2"),
        fresh("carol", "t3"),
    ]);
    let name = s.get_valid("t2", IDLE, TTL).map(|x| x.username.clone());
    assert_eq!(name.as_deref(), Some("bob"));
}

// ───────────────────────── Sessions: touch and sliding expiry ─────────────────────────

#[test]
fn get_valid_refreshes_last_seen() {
    let mut s = sessions_with(vec![aged("alice", "t", 20, 20)]);
    assert!(s.get_valid("t", IDLE, TTL).is_some());
    let seen = s.inner.get("t").unwrap().last_seen;
    assert!(seen.elapsed() < secs(5));
}

#[test]
fn get_valid_does_not_change_created_at() {
    let mut s = sessions_with(vec![aged("alice", "t", 20, 10)]);
    let before = s.inner.get("t").unwrap().created_at;
    assert!(s.get_valid("t", IDLE, TTL).is_some());
    assert_eq!(s.inner.get("t").unwrap().created_at, before);
}

#[test]
fn using_a_session_resets_its_idle_clock() {
    // Idle for 20 s. With a 25 s limit the first use is fine and resets the clock, so a stricter
    // 10 s limit passes right after. Without the touch the second call would see 20 s idle.
    let mut s = sessions_with(vec![aged("alice", "t", 20, 20)]);
    assert!(s.get_valid("t", secs(25), TTL).is_some());
    assert!(s.get_valid("t", secs(10), TTL).is_some());
}

#[test]
fn using_a_session_does_not_extend_the_ttl() {
    // Created 50 s ago, idle 0 s. Fine under a 60 s TTL, expired under a 40 s TTL however often
    // it was used.
    let mut s = sessions_with(vec![aged("alice", "t", 50, 0)]);
    assert!(s.get_valid("t", IDLE, TTL).is_some());
    assert!(s.get_valid("t", IDLE, secs(40)).is_none());
}

// ───────────────────────── Sessions: eviction on lookup ─────────────────────────

#[test]
fn get_valid_idle_expired_session_is_none_and_evicted() {
    let mut s = sessions_with(vec![aged("alice", "t", 40, 40)]);
    assert!(s.get_valid("t", IDLE, TTL).is_none());
    assert!(!is_present(&s, "t"));
}

#[test]
fn get_valid_ttl_expired_session_is_none_and_evicted() {
    let mut s = sessions_with(vec![aged("alice", "t", 90, 0)]);
    assert!(s.get_valid("t", IDLE, TTL).is_none());
    assert!(!is_present(&s, "t"));
}

#[test]
fn an_evicted_session_stays_gone_even_with_generous_limits() {
    let mut s = sessions_with(vec![aged("alice", "t", 40, 40)]);
    assert!(s.get_valid("t", IDLE, TTL).is_none());
    assert!(s.get_valid("t", FOREVER, FOREVER).is_none());
}

#[test]
fn evicting_one_session_leaves_the_others() {
    let mut s = sessions_with(vec![aged("alice", "old", 90, 90), fresh("bob", "live")]);
    assert!(s.get_valid("old", IDLE, TTL).is_none());
    assert!(s.get_valid("live", IDLE, TTL).is_some());
}

// ───────────────────────── Sessions: insert ─────────────────────────

#[test]
fn inserting_the_same_token_replaces_the_session() {
    let mut s = sessions_with(vec![fresh("alice", "t")]);
    s.insert(fresh("bob", "t"));
    assert_eq!(s.inner.len(), 1);
    let name = s.get_valid("t", IDLE, TTL).map(|x| x.username.clone());
    assert_eq!(name.as_deref(), Some("bob"));
}

#[test]
fn replacing_a_session_gives_it_fresh_timers() {
    let mut s = sessions_with(vec![aged("alice", "t", 90, 90)]);
    s.insert(fresh("alice", "t"));
    assert!(s.get_valid("t", IDLE, TTL).is_some());
}

#[test]
fn one_user_can_hold_several_sessions() {
    let mut s = sessions_with(vec![fresh("alice", "t1"), fresh("alice", "t2")]);
    assert!(s.get_valid("t1", IDLE, TTL).is_some());
    assert!(s.get_valid("t2", IDLE, TTL).is_some());
}

// ───────────────────────── Sessions: removal ─────────────────────────

#[test]
fn remove_by_token_removes_only_that_session() {
    let mut s = sessions_with(vec![fresh("alice", "t1"), fresh("alice", "t2")]);
    assert!(s.remove_by_token("t1"));
    assert!(!is_present(&s, "t1"));
    assert!(is_present(&s, "t2"));
}

#[test]
fn remove_by_token_is_false_the_second_time() {
    let mut s = sessions_with(vec![fresh("alice", "t1")]);
    assert!(s.remove_by_token("t1"));
    assert!(!s.remove_by_token("t1"));
}

#[test]
fn remove_by_token_unknown_is_false() {
    let mut s = sessions_with(vec![fresh("alice", "t1")]);
    assert!(!s.remove_by_token("nope"));
    assert!(is_present(&s, "t1"));
}

#[test]
fn a_removed_token_no_longer_validates() {
    let mut s = sessions_with(vec![fresh("alice", "t1")]);
    assert!(s.remove_by_token("t1"));
    assert!(s.get_valid("t1", IDLE, TTL).is_none());
}

#[test]
fn remove_by_user_removes_all_of_that_users_sessions_only() {
    let mut s = sessions_with(vec![
        fresh("alice", "a1"),
        fresh("alice", "a2"),
        fresh("bob", "b1"),
    ]);
    s.remove_by_user("alice");
    assert!(!is_present(&s, "a1"));
    assert!(!is_present(&s, "a2"));
    assert!(is_present(&s, "b1"));
}

#[test]
fn remove_by_user_unknown_user_changes_nothing() {
    let mut s = sessions_with(vec![fresh("alice", "a1")]);
    s.remove_by_user("nobody");
    assert_eq!(s.inner.len(), 1);
}

#[test]
fn remove_by_user_is_case_sensitive() {
    let mut s = sessions_with(vec![fresh("alice", "a1")]);
    s.remove_by_user("ALICE");
    assert!(is_present(&s, "a1"));
}

// ───────────────────────── Sessions: retain_valid ─────────────────────────

#[test]
fn retain_valid_drops_expired_sessions_and_keeps_live_ones() {
    let mut s = sessions_with(vec![
        fresh("alice", "live"),
        aged("alice", "idle", 40, 40),
        aged("alice", "old", 90, 0),
        aged("alice", "both", 90, 90),
    ]);
    s.retain_valid(IDLE, TTL);
    assert!(is_present(&s, "live"));
    assert!(!is_present(&s, "idle"));
    assert!(!is_present(&s, "old"));
    assert!(!is_present(&s, "both"));
    assert_eq!(s.inner.len(), 1);
}

#[test]
fn retain_valid_with_forever_limits_keeps_everything() {
    let mut s = sessions_with(vec![aged("alice", "a", 90, 90), aged("bob", "b", 500, 400)]);
    s.retain_valid(FOREVER, FOREVER);
    assert_eq!(s.inner.len(), 2);
}

#[test]
fn retain_valid_on_an_empty_store_is_fine() {
    let mut s = Sessions::default();
    s.retain_valid(IDLE, TTL);
    assert!(s.inner.is_empty());
}

#[test]
fn retain_valid_does_not_touch_last_seen() {
    // Only `get_valid` counts as use. The background sweep must not keep sessions alive.
    let mut s = sessions_with(vec![aged("alice", "t", 20, 20)]);
    s.retain_valid(IDLE, TTL);
    let seen = s.inner.get("t").unwrap().last_seen;
    assert!(seen.elapsed() >= secs(15));
}

// ───────────────────────── Global store ─────────────────────────

#[test]
fn get_sessions_always_returns_the_same_store() {
    assert!(std::ptr::eq(get_sessions(), get_sessions()));
}

#[test]
fn global_store_round_trip() {
    let token = unique("global");
    get_sessions()
        .write()
        .unwrap()
        .insert(fresh("alice", &token));
    let name = get_sessions()
        .write()
        .unwrap()
        .get_valid(&token, IDLE, TTL)
        .map(|s| s.username.clone());
    assert_eq!(name.as_deref(), Some("alice"));
    assert!(get_sessions().write().unwrap().remove_by_token(&token));
    assert!(
        get_sessions()
            .write()
            .unwrap()
            .get_valid(&token, IDLE, TTL)
            .is_none()
    );
}

// ───────────────────────── Reaper ─────────────────────────
// The reaper's interval fires once immediately, so a freshly spawned reaper sweeps right away.
// The 300 s period and cancellation are not observable without paused tokio time, so they are
// not covered here.

#[tokio::test]
async fn reaper_sweeps_idle_expired_sessions_on_start_and_keeps_live_ones() {
    let old = unique("reaper-idle-old");
    let live = unique("reaper-idle-live");
    {
        let mut guard = get_sessions().write().unwrap();
        guard.insert(aged("alice", &old, 120, 120));
        guard.insert(fresh("bob", &live));
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    spawn_session_reaper(secs(60), FOREVER, cancel.clone());

    let mut gone = false;
    for _ in 0..200 {
        if !has_global(&old) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    cancel.cancel();

    assert!(gone, "the reaper did not remove the idle-expired session");
    assert!(has_global(&live));
    get_sessions().write().unwrap().remove_by_token(&live);
}

#[tokio::test]
async fn reaper_sweeps_ttl_expired_sessions_on_start() {
    let old = unique("reaper-ttl-old");
    let live = unique("reaper-ttl-live");
    {
        let mut guard = get_sessions().write().unwrap();
        // Used a moment ago but created 120 s ago, so only the TTL (60 s) expires it.
        guard.insert(aged("alice", &old, 120, 0));
        guard.insert(fresh("bob", &live));
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    spawn_session_reaper(FOREVER, secs(60), cancel.clone());

    let mut gone = false;
    for _ in 0..200 {
        if !has_global(&old) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    cancel.cancel();

    assert!(gone, "the reaper did not remove the ttl-expired session");
    assert!(has_global(&live));
    get_sessions().write().unwrap().remove_by_token(&live);
}

// ───────────────────────── Known issue ─────────────────────────

#[test]
#[ignore = "known issue (finding 28): Debug on Session and Sessions prints the bearer tokens"]
fn debug_output_does_not_contain_the_token() {
    let secret = "secret-token-123";
    let one = format!("{:?}", fresh("alice", secret));
    assert!(!one.contains(secret), "Session Debug leaks the token");
    let all = format!("{:?}", sessions_with(vec![fresh("alice", secret)]));
    assert!(!all.contains(secret), "Sessions Debug leaks the token");
}
