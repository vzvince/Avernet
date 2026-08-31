use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug)]
pub struct BridgeError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl BridgeError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self { status, code, message: message.into(), retryable }
    }
    pub fn invalid_request(m: impl Into<String>) -> Self { Self::new(StatusCode::BAD_REQUEST, "invalid_request", m, false) }
    pub fn unauthorized() -> Self { Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "invalid token", false) }
    pub fn provider_id_mismatch() -> Self { Self::new(StatusCode::FORBIDDEN, "provider_id_mismatch", "provider_id does not match this bridge", false) }
    pub fn bot_not_found(r: &str) -> Self { Self::new(StatusCode::NOT_FOUND, "bot_not_found", format!("bot {r} is not registered on this bridge"), false) }
    pub fn conflict() -> Self { Self::new(StatusCode::CONFLICT, "conflict", "same idempotency key with different body", false) }
    pub fn rate_limited() -> Self { Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "a run is already active for this session", true) }
    pub fn unsupported_method(m: &str) -> Self { Self::new(StatusCode::NOT_IMPLEMENTED, "unsupported_method", format!("method {m} is not supported"), false) }
    pub fn unavailable(m: impl Into<String>) -> Self { Self::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable", m, true) }
    pub fn timeout() -> Self { Self::new(StatusCode::GATEWAY_TIMEOUT, "timeout", "dependency timed out", true) }
    pub fn run_terminated() -> Self { Self::new(StatusCode::GONE, "run_terminated", "run is already terminal", false) }
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({
            "ok": false,
            "error": { "code": self.code, "message": self.message, "retryable": self.retryable }
        }))).into_response()
    }
}
