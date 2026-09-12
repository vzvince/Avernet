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
        let (status, body) = self.into_parts();
        (status, Json(body)).into_response()
    }
}

impl BridgeError {
    /// Render this error as `(StatusCode, body Value)` — the same shape
    /// [`IntoResponse`] produces, but split out so a caller can (a) store the
    /// body in the idempotency ledger via `complete_with_status` and (b) return
    /// the exact same status+body as the original response on a same-id retry.
    /// Used by `chat.abort`'s 410 `run_terminated` path so ledger replay returns
    /// 410 (not the default in-flight 200 ack).
    pub fn into_parts(self) -> (StatusCode, serde_json::Value) {
        let body = json!({
            "ok": false,
            "error": { "code": self.code, "message": self.message, "retryable": self.retryable }
        });
        (self.status, body)
    }
}
