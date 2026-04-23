mod api;
mod config;
mod errors;
mod prelude;
mod process_tracker;
mod screen_capture;
mod telegram_bot;
mod telemetry;
mod types;
mod utils;
mod webhook;

#[tokio::main]
async fn main() -> Result<(), errors::Error> {
    telemetry::init_tracing()?;
    let config = config::init_config()?;
    if let Some(action) = config.args.command.as_ref() {
        return config::handle_config_command(action);
    }
    process_tracker::init_process_tracker();
    let cancel_token = tokio_util::sync::CancellationToken::new();
    api::init_api_server(cancel_token.clone())?;
    webhook::init_webhook_dispatcher(cancel_token.clone());
    let telegram_bot_handle = telegram_bot::init_bot(cancel_token.clone());
    while !cancel_token.is_cancelled() {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    if let Some(handle) = telegram_bot_handle {
        handle.shutdown().unwrap().await;
    }
    Ok(())
}
