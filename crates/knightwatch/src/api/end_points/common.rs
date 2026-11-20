use axum::{
    Extension,
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    response::Json,
};
use axum_extra::{TypedHeader, headers};

use kw_types::api::{HealthResponse, LoginRequest, LoginResponse};

use super::super::{session, utils::internal_server_error};
use crate::{config::LoginOutcome, observability::history};

pub async fn shutdown(
    State(cancel_token): State<tokio_util::sync::CancellationToken>,
) -> &'static str {
    cancel_token.cancel();
    "Shutting down…"
}

pub async fn health() -> Json<HealthResponse> {
    let uptime = super::super::handlers::START_TIME
        .get()
        .map_or(0, |t| t.elapsed().as_secs());
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: crate::utils::now_rfc3339(),
        version: crate::utils::get_version().to_string(),
        uptime: kw_utils::format_time(uptime),
    })
}

pub async fn info() -> Json<kw_types::Info> {
    Json(crate::utils::get_info().await)
}

/// The peer address is absent when the server runs without connect info
/// (tests); such logins are not rate-limited.
pub async fn login(
    connect_info: Option<Extension<ConnectInfo<std::net::SocketAddr>>>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let ip = connect_info.map(|Extension(ConnectInfo(addr))| addr.ip());
    match crate::config::login(&body.username, body.password, ip).await {
        None => return Err(StatusCode::NOT_FOUND),
        Some(LoginOutcome::Success(_)) => {}
        Some(LoginOutcome::WrongCredentials) => return Err(StatusCode::UNAUTHORIZED),
        Some(LoginOutcome::TooManyAttempts) => return Err(StatusCode::TOO_MANY_REQUESTS),
    }
    let token = uuid::Uuid::new_v4().to_string();
    let user_session = session::Session::new(body.username, token.clone());
    session::get_sessions()
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .insert(user_session);
    Ok(Json(LoginResponse { token }))
}

pub async fn logout(
    TypedHeader(auth): TypedHeader<headers::Authorization<headers::authorization::Bearer>>,
) -> StatusCode {
    session::get_sessions()
        .write()
        .map_or(StatusCode::INTERNAL_SERVER_ERROR, |mut sessions| {
            sessions.remove_by_token(auth.token());
            StatusCode::OK
        })
}

pub async fn history(
    Query(query): Query<kw_types::history::HistoryQuery>,
) -> Result<Json<Vec<kw_types::event::StoredEvent>>, (StatusCode, String)> {
    history::query_history(query)
        .await
        .map(Json)
        .map_err(|err| internal_server_error(&err))
}
