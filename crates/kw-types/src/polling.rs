use std::time::Duration;

#[cfg(feature = "server")]
#[derive(Debug)]
pub struct Poll {
    pub interval: Duration,
    pub interval_timer: Option<tokio::time::Interval>,
}

#[cfg(feature = "server")]
impl Poll {
    #[must_use]
    pub const fn new(secs: u64) -> Self {
        Self {
            interval: Duration::from_secs(secs),
            interval_timer: None,
        }
    }

    pub fn pause(&mut self) {
        self.interval_timer = None;
    }

    pub fn resume(&mut self) {
        self.interval_timer = Some(tokio::time::interval(self.interval));
    }

    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
        if !self.is_paused() {
            self.interval_timer = Some(tokio::time::interval(self.interval));
        }
    }

    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.interval_timer.is_none()
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PollStatus {
    pub interval: u128,
    pub paused: bool,
}

#[cfg(feature = "server")]
impl From<&Poll> for PollStatus {
    fn from(poll: &Poll) -> Self {
        Self {
            interval: poll.interval.as_millis(),
            paused: poll.is_paused(),
        }
    }
}
