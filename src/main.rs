mod api;
mod config;
mod core;
mod errors;
mod prelude;
mod process_tracker;
mod screen_capture;
mod telegram_bot;

#[tokio::main]
async fn main() -> Result<(), errors::Error> {
    core::telemetry::init_tracing()?;
    let config = config::init_config()?;
    if let Some(action) = config.args.command.as_ref() {
        return config::handle_config_command(action);
    }
    process_tracker::init_process_tracker();
    let _telegram_bot_handle = telegram_bot::init_bot(); // handle.shutdown().unwrap().await;
    let api_server_handle = api::init_api_server()?;
    if let Some(handle) = api_server_handle {
        match handle.await {
            Ok(_) => tracing::info!("API server stopped gracefully"),
            Err(e) => tracing::error!(?e, "API server error"),
        }
    }
    Ok(())
}
