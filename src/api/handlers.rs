use axum::{Router, routing::{get, post}};
use tokio_util::sync::CancellationToken;

use super::end_points::*;
use crate::prelude::*;

fn create_router(cancel_token: CancellationToken) -> Router {
    Router::new()
        // ── Screenshot ────────────────────────────────────────────────────
        .route("/health", get(health))
        .route("/screenshot", get(screenshot))
        .route("/view", get(view))
        .route("/view.css", get(view_css))
        .route("/view.js", get(view_js))
        // ── Process tracking ──────────────────────────────────────────────
        .route("/process", get(process_tree)) // full tree
        .route("/process/root", get(process_root)) // root only
        .route("/process/children", get(process_children)) // children only
        .route("/process/status", get(process_status)) // lightweight summary
        .route("/shutdown", post(shutdown))
        .with_state(cancel_token)
}

pub fn init_api_server(cancel_token: CancellationToken) -> Result<()> {
    let config = get_config();
    if config.args.no_server {
        return Ok(());
    }
    let api_listener = crate::core::utils::get_listener(&get_config().server_address())?;
    let app = create_router(cancel_token.clone());
    tokio::spawn(async move {
        axum::serve(api_listener, app)
            .with_graceful_shutdown(async move {
                cancel_token.cancelled().await;
            })
            .await
            .expect("API server failed");
        tracing::info!("API server stopped gracefully");
    });
    Ok(())
}
