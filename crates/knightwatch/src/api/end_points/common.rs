use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use axum_extra::{TypedHeader, headers};

use kw_types::api::{HealthResponse, InfoResponse, LoginRequest, LoginResponse};

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

pub async fn info() -> Json<InfoResponse> {
    let args = &crate::prelude::get_config().args;
    Json(InfoResponse {
        auth_enabled: args.enable_auth,
        shutdown_enabled: args.enable_shutdown,
        blind: args.is_blind(),
        pid: crate::process_tracker::get_root_pids().await,
        top_processes: args.top_processes,
        limit_processes: args.limit_processes,
        telegram_bot: args.telegram,
        system_resources: args.system_resources,
        systemd: args.systemd,
        docker: args.docker,
        allow_process_commands: args.allow_process_commands,
        allow_screen_commands: args.is_screen_commands_allowed(),
        allow_system_resources_commands: args.allow_system_resources_commands,
        allow_systemd_commands: args.allow_systemd_commands,
        allow_docker_commands: args.allow_docker_commands,
    })
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
