use std::time::Duration;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PollStatus {
    pub interval: u128,
    pub paused: bool,
}

impl PollStatus {
    #[must_use]
    pub const fn new(interval: Duration, paused: bool) -> Self {
        Self {
            interval: interval.as_millis(),
            paused,
        }
    }

    #[must_use]
    pub const fn new_some(interval: Duration, paused: bool) -> Option<Self> {
        Some(Self::new(interval, paused))
    }
}
