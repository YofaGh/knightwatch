#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::net::{IpAddr, Ipv4Addr};

use super::{
    login::{FailedLogins, LOCKOUT_WINDOW, LoginOutcome, MAX_FAILED_LOGINS, check_login},
    users::{User, Users},
};

// ───────────────────────── Helpers ─────────────────────────

/// One user, "alice" / "right", hashed at the lowest cost so tests stay fast.
fn users() -> Users {
    Users {
        users: vec![User {
            username: "alice".into(),
            password_hash: bcrypt::hash("right", 4).unwrap(),
            telegram_token: "123456".into(),
            telegram_chat_id: None,
        }],
    }
}

#[allow(clippy::unnecessary_wraps)] // shaped like the `ip` argument it fills
fn ip(last: u8) -> Option<IpAddr> {
    Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, last)))
}

async fn attempt(
    users: &Users,
    limits: &FailedLogins,
    username: &str,
    password: &str,
    from: Option<IpAddr>,
) -> LoginOutcome {
    check_login(users, limits, username, password.into(), from).await
}

fn is_success(outcome: &LoginOutcome) -> bool {
    matches!(outcome, LoginOutcome::Success(u) if u.username == "alice")
}

async fn fail_until_locked(users: &Users, limits: &FailedLogins, from: Option<IpAddr>) {
    for _ in 0..MAX_FAILED_LOGINS {
        let outcome = attempt(users, limits, "alice", "wrong", from).await;
        assert!(matches!(outcome, LoginOutcome::WrongCredentials));
    }
}

// ───────────────────────── Credentials ─────────────────────────

#[tokio::test]
async fn the_right_password_logs_in() {
    let outcome = attempt(&users(), &FailedLogins::default(), "alice", "right", ip(1)).await;
    assert!(is_success(&outcome));
}

#[tokio::test]
async fn a_wrong_password_is_rejected() {
    let outcome = attempt(&users(), &FailedLogins::default(), "alice", "wrong", ip(1)).await;
    assert!(matches!(outcome, LoginOutcome::WrongCredentials));
}

#[tokio::test]
async fn an_unknown_user_is_rejected_like_a_wrong_password() {
    let outcome = attempt(
        &users(),
        &FailedLogins::default(),
        "mallory",
        "right",
        ip(1),
    )
    .await;
    assert!(matches!(outcome, LoginOutcome::WrongCredentials));
}

#[tokio::test]
async fn an_unknown_user_still_pays_for_a_bcrypt_check() {
    // The dummy hash is DEFAULT_COST, so even the fastest machine takes well
    // over a millisecond; an early return would take microseconds.
    let started = std::time::Instant::now();
    let _ = attempt(&users(), &FailedLogins::default(), "mallory", "x", ip(1)).await;
    assert!(started.elapsed() >= std::time::Duration::from_millis(5));
}

#[tokio::test]
async fn a_broken_stored_hash_is_a_wrong_password_not_a_crash() {
    let mut list = users();
    list.users[0].password_hash = "not a bcrypt hash".into();
    let outcome = attempt(&list, &FailedLogins::default(), "alice", "right", ip(1)).await;
    assert!(matches!(outcome, LoginOutcome::WrongCredentials));
}

// ───────────────────────── Failed-login limit ─────────────────────────

#[tokio::test]
async fn too_many_failures_lock_out_that_ip_even_with_the_right_password() {
    let (users, limits) = (users(), FailedLogins::default());
    fail_until_locked(&users, &limits, ip(1)).await;

    let outcome = attempt(&users, &limits, "alice", "right", ip(1)).await;
    assert!(matches!(outcome, LoginOutcome::TooManyAttempts));
}

#[tokio::test]
async fn a_locked_ip_does_not_lock_out_another_ip() {
    let (users, limits) = (users(), FailedLogins::default());
    fail_until_locked(&users, &limits, ip(1)).await;

    assert!(is_success(
        &attempt(&users, &limits, "alice", "right", ip(2)).await
    ));
}

#[tokio::test]
async fn failures_for_an_unknown_user_count_too() {
    let (users, limits) = (users(), FailedLogins::default());
    for _ in 0..MAX_FAILED_LOGINS {
        let _ = attempt(&users, &limits, "mallory", "x", ip(1)).await;
    }
    let outcome = attempt(&users, &limits, "alice", "right", ip(1)).await;
    assert!(matches!(outcome, LoginOutcome::TooManyAttempts));
}

#[tokio::test]
async fn a_success_clears_the_count() {
    let (users, limits) = (users(), FailedLogins::default());
    for _ in 0..MAX_FAILED_LOGINS - 1 {
        let _ = attempt(&users, &limits, "alice", "wrong", ip(1)).await;
    }
    assert!(is_success(
        &attempt(&users, &limits, "alice", "right", ip(1)).await
    ));
    // a fresh run of failures is needed again before the lock
    for _ in 0..MAX_FAILED_LOGINS - 1 {
        let _ = attempt(&users, &limits, "alice", "wrong", ip(1)).await;
    }
    assert!(is_success(
        &attempt(&users, &limits, "alice", "right", ip(1)).await
    ));
}

#[tokio::test(start_paused = true)]
async fn the_lock_lifts_after_the_window() {
    let (users, limits) = (users(), FailedLogins::default());
    fail_until_locked(&users, &limits, ip(1)).await;

    tokio::time::advance(LOCKOUT_WINDOW).await;

    assert!(is_success(
        &attempt(&users, &limits, "alice", "right", ip(1)).await
    ));
}

#[tokio::test]
async fn a_login_with_no_known_ip_is_never_locked() {
    let (users, limits) = (users(), FailedLogins::default());
    fail_until_locked(&users, &limits, None).await;

    assert!(is_success(
        &attempt(&users, &limits, "alice", "right", None).await
    ));
}
