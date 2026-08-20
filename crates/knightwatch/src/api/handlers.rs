use axum::{body::Body, http::StatusCode, response::Response};
use std::{sync::OnceLock, time::Instant};

use super::{models::Vite, routers::create_routers};
use crate::prelude::*;

pub static START_TIME: OnceLock<Instant> = OnceLock::new();

fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

#[cfg(debug_assertions)]
async fn serve_dashboard(uri: axum::http::Uri) -> Response {
    fn bad_gateway(msg: &'static str) -> Response {
        let mut response = Response::new(Body::from(msg));
        *response.status_mut() = StatusCode::BAD_GATEWAY;
        response
    }

    let vite_url = uri.query().map_or_else(
        || format!("http://localhost:5173{}", uri.path()),
        |q| format!("http://localhost:5173{}?{}", uri.path(), q),
    );
    match reqwest::Client::new().get(&vite_url).send().await {
        Ok(res) => {
            let status = res.status();
            let headers = res.headers().clone();
            let bytes = match res.bytes().await {
                Ok(b) => b,
                Err(err) => {
                    error!(?err, "Failed to read Vite response body");
                    return bad_gateway("Failed to read Vite response body");
                }
            };
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = status;
            if let Some(ct) = headers.get(reqwest::header::CONTENT_TYPE) {
                response
                    .headers_mut()
                    .insert(reqwest::header::CONTENT_TYPE, ct.clone());
            }
            response
        }
        Err(_) => bad_gateway("Vite dev server not running on :5173"),
    }
}

#[cfg(not(debug_assertions))]
async fn serve_dashboard(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let is_spa_route = path == "dashboard" || path == "index.html" || path.is_empty();
    let asset_path = if is_spa_route { "index.html" } else { path };
    if let Some(content) = super::models::DashboardAssets::get(asset_path) {
        let mime = mime_guess::from_path(asset_path)
            .first_or_octet_stream()
            .to_string();
        let mut response = Response::new(Body::from(content.data));
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&mime) {
            response
                .headers_mut()
                .insert(reqwest::header::CONTENT_TYPE, value);
        }
        response
    } else {
        let mut response = Response::new(Body::from("404 Not Found"));
        *response.status_mut() = StatusCode::NOT_FOUND;
        response
    }
}

pub fn init_api_server(cancel_token: tokio_util::sync::CancellationToken) -> Result<Option<Vite>> {
    let config = get_config();
    if config.args.no_api {
        return Ok(None);
    }
    init_start_time();
    let mut app = create_routers(config, cancel_token.clone());
    #[cfg(debug_assertions)]
    let vite = if config.args.no_dashboard {
        None
    } else {
        app = app.fallback(serve_dashboard);
        crate::utils::start_dev_server().map(Vite::new)
    };
    #[cfg(not(debug_assertions))]
    let vite = {
        if !config.args.no_dashboard {
            app = app.fallback(serve_dashboard);
        }
        None
    };
    let api_listener = crate::utils::get_listener(&config.server_address())?;
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
    info!("API server started");
    if !config.args.no_dashboard {
        info!("Dashboard available at /");
    }
    Ok(vite)
}
