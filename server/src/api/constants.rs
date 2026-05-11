use std::sync::OnceLock;

pub static START_TIME: OnceLock<std::time::Instant> = OnceLock::new();
