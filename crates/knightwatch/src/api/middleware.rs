use axum::http::StatusCode;

use crate::prelude::*;

pub async fn auth_middleware(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let Some(users) = get_users().filter(|u| !u.users.is_empty()) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let Some(token) = token else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let (idle, ttl) = get_config().args.session_timeouts();
    let user = super::session::get_sessions()
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .get_valid(token, idle, ttl)
        .and_then(|session| users.find(&session.username))
        .map(DisplayUser::from)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}
