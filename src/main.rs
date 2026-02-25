mod api;
mod config;
mod core;
mod errors;
mod prelude;

#[tokio::main]
async fn main() -> Result<(), crate::errors::Error> {
    core::telemetry::init_tracing()?;
    config::init_config()?;
    core::process_tracker::init_process_tracker();
    tokio::join!(async {
        if config::get_config().args.no_server {
            return;
        }
        match crate::api::init_api_server() {
            Ok(handle) => match handle.await {
                Ok(_) => tracing::info!("API server stopped gracefully"),
                Err(e) => tracing::error!(?e, "API server error"),
            },
            Err(e) => {
                tracing::error!(?e, "Failed to init API");
            }
        }
    },);
    Ok(())
}
