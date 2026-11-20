//! Password checks shared by the HTTP and socket logins: bcrypt off the async
//! threads, the same cost for unknown users, and a per-IP failure limit.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use tokio::time::Instant;

use super::users::{User, Users, get_users};
use crate::prelude::*;

/// Failures from one IP before it is refused for [`LOCKOUT_WINDOW`].
pub const MAX_FAILED_LOGINS: u32 = 5;
/// How long failures are remembered, counted from the latest one.
pub const LOCKOUT_WINDOW: Duration = Duration::from_mins(15);

#[derive(Debug)]
pub enum LoginOutcome {
    Success(User),
    WrongCredentials,
    TooManyAttempts,
}

/// Failed logins per IP: the count and the time of the latest failure.
#[derive(Default)]
pub struct FailedLogins {
    inner: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

impl FailedLogins {
    fn is_locked(&self, ip: IpAddr) -> bool {
        let Ok(map) = self.inner.lock() else {
            return false;
        };
        map.get(&ip)
            .is_some_and(|&(n, last)| n >= MAX_FAILED_LOGINS && last.elapsed() < LOCKOUT_WINDOW)
    }

    fn record_failure(&self, ip: IpAddr) {
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        // shortcut: stale entries are pruned on every failure (O(n)); fine for
        // the few IPs a monitor sees, use a timed cache if that ever grows.
        map.retain(|_, (_, last)| last.elapsed() < LOCKOUT_WINDOW);
        let entry = map.entry(ip).or_insert_with(|| (0, Instant::now()));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = Instant::now();
    }

    fn clear(&self, ip: IpAddr) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(&ip);
        }
    }
}

static FAILED_LOGINS: LazyLock<FailedLogins> = LazyLock::new(FailedLogins::default);

/// Checked for unknown users so they take as long as a wrong password.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| bcrypt::hash("knightwatch-dummy", bcrypt::DEFAULT_COST).unwrap_or_default());

/// `ip` is `None` when the caller cannot tell; such logins are never locked.
pub async fn check_login(
    users: &Users,
    limits: &FailedLogins,
    username: &str,
    password: String,
    ip: Option<IpAddr>,
) -> LoginOutcome {
    if ip.is_some_and(|ip| limits.is_locked(ip)) {
        return LoginOutcome::TooManyAttempts;
    }
    let user = users.find(username).cloned();
    let hash = user.as_ref().map(|u| u.password_hash.clone());
    let verified = tokio::task::spawn_blocking(move || {
        let hash = hash.as_deref().unwrap_or(DUMMY_HASH.as_str());
        bcrypt::verify(password, hash).unwrap_or(false)
    })
    .await
    .unwrap_or(false);

    if let Some(user) = user.filter(|_| verified) {
        if let Some(ip) = ip {
            limits.clear(ip);
        }
        return LoginOutcome::Success(user);
    }
    if let Some(ip) = ip {
        limits.record_failure(ip);
    }
    LoginOutcome::WrongCredentials
}

/// Checks against the loaded users; `None` when no users are configured.
pub async fn login(username: &str, password: String, ip: Option<IpAddr>) -> Option<LoginOutcome> {
    let users = get_users().filter(|u| !u.users.is_empty())?;
    let outcome = check_login(users, &FAILED_LOGINS, username, password, ip).await;
    if matches!(outcome, LoginOutcome::TooManyAttempts) {
        warn!("Refused a login from {ip:?}: too many failed attempts");
    }
    Some(outcome)
}
