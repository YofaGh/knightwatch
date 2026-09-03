mod api;
mod config;
mod docker_tracker;
mod errors;
mod events;
mod macros;
mod observability;
mod prelude;
mod process_tracker;
mod screen_capture;
mod sse;
mod system_resources;
mod systemd;
mod telegram_bot;
mod types;
mod utils;
mod webhook;

#[tokio::main]
async fn main() -> Result<(), errors::Error> {
    observability::telemetry::init_tracing()?;
    let config = config::init_config()?;
    config::load_users()?;
    if let Some(action) = config.args.command.as_ref() {
        return config::handle_command(action);
    }

    // initialize subsystems. this only sets up the channels mainly for interfaces to subscribe
    process_tracker::init_process_tracker();
    system_resources::init_system_resources();
    docker_tracker::init_docker_tracker();
    systemd::init_systemd_monitor().await;

    // create cancellation token to stop interfaces
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // initialize interfaces
    observability::history::init_event_tracer(cancel_token.clone());
    let vite = api::init_api_server(cancel_token.clone())?;
    webhook::init_webhook_dispatcher(cancel_token.clone());
    sse::init_sse_dispatcher(cancel_token.clone());
    let tg_bot = telegram_bot::init_bot(cancel_token.clone());

    // start subsystems
    #[cfg(feature = "screenshot")]
    screen_capture::start_screen_capture();
    process_tracker::start_process_tracker();
    system_resources::start_system_resources();
    docker_tracker::start_docker_tracker();
    systemd::start_systemd_monitor();

    // wait for cancel token or Ctrl+c signal
    tokio::select! {
        () = cancel_token.cancelled() => {}
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C");
            cancel_token.cancel();
        }
    }
    tracing::warn!("Shutting down...");

    // wait for vite child process to be killed. its only some in dev mode
    if let Some(vite) = vite {
        vite.stop();
    }

    // wait for telegram bot to be shutdown. this uses teloxide's shutdown dispatcher
    if let Some(bot) = tg_bot {
        bot.shutdown().await;
    }
    Ok(())
}
