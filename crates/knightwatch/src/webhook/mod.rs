mod dispatcher;
mod target;

pub use target::WebhookTarget;

use crate::prelude::*;

pub fn init_webhook_dispatcher(cancel_token: tokio_util::sync::CancellationToken) {
    let config = get_config();
    if !config.args.with_webhook && config.args.webhook_urls.is_empty() {
        return;
    }
    let mut raw_urls = config.persistent.webhook_urls.clone();
    raw_urls.extend(config.args.webhook_urls.clone());
    raw_urls.dedup();
    if raw_urls.is_empty() {
        return;
    }

    let targets: Vec<WebhookTarget> = raw_urls
        .iter()
        .filter_map(|u| match WebhookTarget::parse(u) {
            Ok(t) => Some(t),
            Err(e) => {
                warn!("webhook: invalid url `{u}`: {e}");
                None
            }
        })
        .collect();

    if targets.is_empty() {
        return;
    }
    info!(
        count = targets.len(),
        urls = targets
            .iter()
            .map(|t| t.url.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        "starting webhook dispatcher"
    );
    tokio::spawn(dispatcher::run_dispatcher(targets, cancel_token));
}
