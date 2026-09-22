use crate::event;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct HistoryQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    pub event: Option<String>,
    pub source: Option<event::EventSource>,
    pub limit: Option<usize>,
}

impl HistoryQuery {
    pub fn filter(&self, event: &event::StoredEvent) -> bool {
        self.since
            .as_ref()
            .is_none_or(|since| &event.timestamp >= since)
            && self
                .until
                .as_ref()
                .is_none_or(|until| &event.timestamp <= until)
            && self.event.as_ref().is_none_or(|ev| &event.event == ev)
            && self.source.is_none_or(|src| event.source == src)
    }
}