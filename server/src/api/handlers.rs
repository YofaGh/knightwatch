use axum::{
    Router,
    routing::{get, post},
};
use tokio_util::sync::CancellationToken;

use super::{end_points::*, models::AppState};
use crate::prelude::*;

#[derive(rust_embed::Embed)]
#[folder = "../target/site/"]
struct Assets;

async fn static_handler(uri: axum::http::Uri) -> impl axum::response::IntoResponse {
    use axum::{body::Body, http::Response};
    let path = uri.path().trim_start_matches('/');
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(axum::http::header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}

fn init_start_time() {
    super::constants::START_TIME.get_or_init(std::time::Instant::now);
}

fn create_api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/shutdown", post(shutdown))
        .route("/config", get(config))
        // ── Screenshot ────────────────────────────────────────────────────
        .route("/screenshot", get(screenshot))
        // ── Process tracking ──────────────────────────────────────────────
        .route("/root_pids", get(root_pids))
        .route("/process/{root_pid}", get(process_tree))
        .route("/process/root/{root_pid}", get(process_root))
        .route("/process/children/{root_pid}", get(process_children))
        .route("/process/status/{root_pid}", get(process_status))
        .route("/top-processes", get(top_processes))
        // ── System Monitoring ──────────────────────────────────────────────
        .route("/system", get(system_snapshot))
        .route("/cpu", get(cpu_snapshot))
        .route("/memory", get(memory_snapshot))
        .route("/disks", get(disks_snapshots))
        .route("/networks", get(networks_snapshot))
        .route("/gpus", get(gpus_snapshots))
        .route("/battery", get(battery_snapshot))
        .route("/host-info", get(host_info_snapshot))
        .route("/temperatures", get(temperatures_snapshots))
}

fn create_app(cancel_token: CancellationToken, api: bool, dashboard: bool) -> Router {
    let mut state = AppState {
        cancel_token: cancel_token.clone(),
        leptos_options: None,
    };
    let mut app = Router::new();
    if api {
        app = app.nest("/api", create_api_router());
    }
    if dashboard {
        use leptos_axum::LeptosRoutes;
        let conf = leptos_config::get_configuration(None).unwrap();
        let leptos_options = conf.leptos_options.clone();
        state.leptos_options = Some(leptos_options.clone());
        let routes = leptos_axum::generate_route_list(crate::dashboard::app::App);
        app = app
            .leptos_routes(&state, routes, {
                move || crate::dashboard::app::shell(leptos_options.clone())
            })
            .fallback(static_handler)
            .layer(leptos::prelude::provide_context(state.cancel_token.clone()));
    }
    app.with_state(state)
}

pub fn init_api_server(cancel_token: CancellationToken) -> Result<()> {
    let config = get_config();
    let api = !config.args.no_api;
    let dashboard = !config.args.no_dashboard;
    if !api && !dashboard {
        return Ok(());
    }
    init_start_time();
    let api_listener = crate::utils::get_listener(&config.server_address())?;
    let app = create_app(cancel_token.clone(), api, dashboard);
    tokio::spawn(async move {
        if let Err(err) = axum::serve(api_listener, app)
            .with_graceful_shutdown(async move {
                cancel_token.cancelled().await;
            })
            .await
        {
            error!(?err, "API server error");
        } else {
            info!("API server stopped gracefully");
        }
    });
    crate::utils::print_local_ips(config.args.port);
    if api {
        info!("API server started");
    }
    if dashboard {
        info!("Web Dashboard started at /dashboard");
    }
    Ok(())
}
