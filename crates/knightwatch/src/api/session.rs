use std::{
    sync::{OnceLock, RwLock},
    time::{Duration, Instant},
};

static SESSIONS: OnceLock<RwLock<Sessions>> = OnceLock::new();

pub fn get_sessions() -> &'static RwLock<Sessions> {
    SESSIONS.get_or_init(|| RwLock::new(Sessions::default()))
}

#[derive(Debug)]
pub struct Session {
    pub username: String,
    pub token: String,
    created_at: Instant,
    last_seen: Instant,
}

impl Session {
    pub fn new(username: String, token: String) -> Self {
        let now = Instant::now();
        Self { username, token, created_at: now, last_seen: now }
    }

    fn is_expired(&self, idle: Duration, ttl: Duration) -> bool {
        let now = Instant::now();
        now.duration_since(self.last_seen) > idle || now.duration_since(self.created_at) > ttl
    }
}

#[derive(Debug, Default)]
pub struct Sessions {
    inner: std::collections::HashMap<String, Session>,
}

impl Sessions {
    pub fn insert(&mut self, session: Session) {
        self.inner.insert(session.token.clone(), session);
    }

    /// The only read path auth should use — touches last_seen, evicts if expired.
    pub fn get_valid(&mut self, token: &str, idle: Duration, ttl: Duration) -> Option<&Session> {
        if self.inner.get(token)?.is_expired(idle, ttl) {
            self.inner.remove(token);
            return None;
        }
        let session = self.inner.get_mut(token)?;
        session.last_seen = Instant::now();
        Some(session)
    }

    pub fn remove_by_token(&mut self, token: &str) -> bool {
        self.inner.remove(token).is_some()
    }

    #[allow(unused)]
    pub fn remove_by_user(&mut self, username: &str) {
        self.inner.retain(|_, s| s.username != username);
    }

    fn retain_valid(&mut self, idle: Duration, ttl: Duration) {
        self.inner.retain(|_, s| !s.is_expired(idle, ttl));
    }
}

pub fn spawn_session_reaper(idle: Duration, ttl: Duration, cancel: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Ok(mut sessions) = get_sessions().write() {
                        sessions.retain_valid(idle, ttl);
                    }
                }
                () = cancel.cancelled() => break,
            }
        }
    });
}
