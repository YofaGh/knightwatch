use axum::http::StatusCode;

pub fn bad_request(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message)
}

pub fn not_found(message: String) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message)
}

pub fn internal_server_error(error: crate::errors::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
