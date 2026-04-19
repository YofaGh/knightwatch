use axum::{Router, routing::get};

use super::end_points::*;
use crate::prelude::*;

fn create_router() -> Router {
    Router::new()
        // ── Screenshot ────────────────────────────────────────────────────
        .route("/health",     get(health))
        .route("/screenshot", get(screenshot))
        .route("/view",       get(view))
        .route("/view.css", get(view_css))
        .route("/view.js", get(view_js))
        // ── Process tracking ──────────────────────────────────────────────
        .route("/process",          get(process_tree))     // full tree
        .route("/process/root",     get(process_root))     // root only
        .route("/process/children", get(process_children)) // children only
        .route("/process/status",   get(process_status))   // lightweight summary
}

pub fn init_api_server() -> Result<tokio::task::JoinHandle<()>> {
    let api_listener = crate::core::utils::get_listener(&get_config().server_address())?;
    let app = create_router();
    let api_server = tokio::spawn(async move {
        axum::serve(api_listener, app)
            .await
            .expect("API server failed");
    });
    Ok(api_server)
}
