use axum::http::StatusCode;

pub fn validate_token(provided: Option<&str>, expected: &str) -> Result<(), StatusCode> {
    match provided {
        Some(token) if token == expected => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
