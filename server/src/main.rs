#![recursion_limit = "256"]

mod api;
mod config;
mod dashboard;
mod errors;
mod prelude;
mod process_tracker;
mod screen_capture;
mod system_monitor;
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
    screen_capture::init_screen_capture();
    process_tracker::init_process_tracker();
    system_monitor::init_system_monitor();
    let cancel_token = tokio_util::sync::CancellationToken::new();
    api::init_api_server(cancel_token.clone())?;
    webhook::init_webhook_dispatcher(cancel_token.clone());
    let telegram_bot_handle = telegram_bot::init_bot(cancel_token.clone());
    tokio::select! {
        _ = cancel_token.cancelled() => {}
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C");
            cancel_token.cancel();
        }
    }
    tracing::warn!("Shutting down...");
    if let Some(handle) = telegram_bot_handle {
        handle.shutdown().unwrap().await;
    }
    Ok(())
}
