use url::Url;

use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct WebhookTarget {
    pub url: String,
    pub include_ticks: bool,
}

impl WebhookTarget {
    /// Parses a raw URL string, pulling `webhook_events` out of the query
    /// string (if present) and stripping it so it isn't sent to the actual
    /// endpoint.
    pub fn parse(raw: &str) -> Result<Self> {
        let mut url = Url::parse(raw)
            .map_err(|err| Error::Other(format!("Failed to parse url: {raw}, err: {err}")))?;

        let mut include_ticks = false;
        let remaining: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, v)| {
                if k == "webhook_events" {
                    include_ticks = v.split(',').any(|e| e == "tick");
                    false // drop this pair
                } else {
                    true
                }
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        // rebuild query string without our reserved param
        url.set_query(None);
        if !remaining.is_empty() {
            let mut serializer = url.query_pairs_mut();
            for (k, v) in &remaining {
                serializer.append_pair(k, v);
            }
        }

        Ok(Self {
            url: url.to_string(),
            include_ticks,
        })
    }
}
