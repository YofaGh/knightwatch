use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use axum_extra::{TypedHeader, headers};

use kw_types::api::{HealthResponse, LoginRequest, LoginResponse};

use super::super::{session, utils::internal_server_error};
use crate::observability::history;

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

pub async fn login(Json(body): Json<LoginRequest>) -> Result<Json<LoginResponse>, StatusCode> {
    let Some(users) = crate::config::get_users().filter(|u| !u.users.is_empty()) else {
        return Err(StatusCode::NOT_FOUND);
    };
    match users.verify_password(&body.username, &body.password) {
        Ok(true) => {}
        _ => return Err(StatusCode::UNAUTHORIZED),
    }
    let token = uuid::Uuid::new_v4().to_string();
    let session = session::Session {
        username: body.username,
        token: token.clone(),
    };
    session::get_sessions()
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .insert(session);
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
    Query(query): Query<history::HistoryQuery>,
) -> Result<Json<Vec<history::StoredEvent>>, (StatusCode, String)> {
    history::query_history(query)
        .await
        .map(Json)
        .map_err(|err| internal_server_error(&err))
}
