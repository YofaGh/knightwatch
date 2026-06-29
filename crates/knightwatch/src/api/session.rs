use std::sync::{OnceLock, RwLock};

static SESSIONS: OnceLock<RwLock<Sessions>> = OnceLock::new();

pub fn get_sessions() -> &'static RwLock<Sessions> {
    SESSIONS.get().expect("Sessions not initialized")
}

pub fn init_sessions() {
    SESSIONS
        .set(RwLock::new(Sessions::default()))
        .expect("Sessions already initialized");
}

#[derive(Debug)]
pub struct Session {
    pub username: String,
    pub token: String,
}

#[derive(Debug, Default)]
pub struct Sessions {
    inner: std::collections::HashMap<String, Session>,
}

impl Sessions {
    pub fn insert(&mut self, session: Session) {
        self.inner.insert(session.token.clone(), session);
    }

    pub fn get(&self, token: &str) -> Option<&Session> {
        self.inner.get(token)
    }

    pub fn remove_by_token(&mut self, token: &str) -> bool {
        self.inner.remove(token).is_some()
    }

    #[allow(unused)]
    pub fn remove_by_user(&mut self, username: &str) {
        self.inner.retain(|_, s| s.username != username);
    }
}
