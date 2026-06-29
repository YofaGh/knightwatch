mod api;
mod config;
mod docker_tracker;
mod errors;
mod events;
mod macros;
mod prelude;
mod process_tracker;
mod screen_capture;
mod sse;
mod system_resources;
mod systemd;
mod telegram_bot;
mod telemetry;
mod types;
mod utils;
mod webhook;

#[tokio::main]
async fn main() -> Result<(), errors::Error> {
    telemetry::init_tracing()?;
    let config = config::init_config()?;
    config::load_users()?;
    if let Some(action) = config.args.command.as_ref() {
        return config::handle_command(action);
    }
    #[cfg(feature = "screenshot")]
    screen_capture::init_screen_capture();
    process_tracker::init_process_tracker();
    system_resources::init_system_resources();
    docker_tracker::init_docker_tracker();
    systemd::init_systemd_monitor();
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let vite = api::init_api_server(cancel_token.clone())?;
    webhook::init_webhook_dispatcher(cancel_token.clone());
    sse::init_sse_dispatcher(cancel_token.clone());
    let telegram_bot = telegram_bot::init_bot(cancel_token.clone());
    tokio::select! {
        _ = cancel_token.cancelled() => {}
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C");
            cancel_token.cancel();
        }
    }
    tracing::warn!("Shutting down...");
    if let Some(vite) = vite {
        vite.stop();
    }
    if let Some(bot) = telegram_bot {
        bot.shutdown().await;
    }
    Ok(())
}
