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
    #[must_use]
    pub fn filter(&self, event: &event::StoredEvent) -> bool {
        let ts = event.timestamp.as_str();
        self.since
            .as_deref()
            .is_none_or(|since| compare_timestamps(ts, since, false).is_ge())
            && self
                .until
                .as_deref()
                .is_none_or(|until| compare_timestamps(ts, until, true).is_le())
            && self.event.as_ref().is_none_or(|ev| &event.event == ev)
            && self.source.is_none_or(|src| event.source == src)
    }
}

/// Compares an RFC 3339 timestamp with a bound as instants, so offsets and
/// fractions order correctly. Anything that does not parse is compared as text.
fn compare_timestamps(ts: &str, bound: &str, end_of_day: bool) -> std::cmp::Ordering {
    match (
        chrono::DateTime::parse_from_rfc3339(ts),
        parse_bound(bound, end_of_day),
    ) {
        (Ok(ts), Some(bound)) => ts.cmp(&bound),
        _ => ts.cmp(bound),
    }
}

/// An RFC 3339 bound, or a bare "YYYY-MM-DD" taken as the start (or, with
/// `end_of_day`, the last instant) of that UTC day.
fn parse_bound(bound: &str, end_of_day: bool) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(bound) {
        return Some(t);
    }
    let day = chrono::NaiveDate::parse_from_str(bound, "%Y-%m-%d").ok()?;
    let t = if end_of_day {
        day.and_hms_nano_opt(23, 59, 59, 999_999_999)?
    } else {
        day.and_hms_opt(0, 0, 0)?
    };
    Some(t.and_utc().fixed_offset())
}
