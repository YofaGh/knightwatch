mod dispatcher;
mod structs;

use crate::prelude::*;

pub fn init_webhook_dispatcher(cancel_token: tokio_util::sync::CancellationToken) {
    let config = get_config();
    if !config.args.with_webhook {
        return;
    }
    let mut urls = config.persistent.webhook_urls.clone();
    urls.extend(config.args.webhook_urls.clone());
    urls.dedup();
    if urls.is_empty() {
        return;
    }
    info!(count = urls.len(), "starting webhook dispatcher");
    tokio::spawn(dispatcher::run_dispatcher(urls, cancel_token));
}
