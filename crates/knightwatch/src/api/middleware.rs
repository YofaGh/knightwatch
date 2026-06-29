use axum::http::StatusCode;

pub async fn auth_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let users = crate::config::get_users();
    if users.users.is_empty() {
        return Ok(next.run(request).await);
    }
    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let Some(token) = token else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if super::session::get_sessions()
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .get(token)
        .is_none()
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}
