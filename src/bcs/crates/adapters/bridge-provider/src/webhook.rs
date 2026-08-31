use std::sync::Arc;
use axum::{extract::State, http::HeaderMap, response::{IntoResponse, Response}, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{config::ProviderConfig, error::BridgeError};

#[derive(Debug, Deserialize)]
pub struct ToBot { pub provider_id: String, pub provider_bot_ref: String }

#[derive(Debug, Deserialize)]
pub struct DownstreamRequest {
    pub id: String,
    pub method: String,
    pub to_bot: ToBot,
    pub session_id: Option<String>,
    pub message: Option<Value>,
    pub from: Option<Value>,   // {"kind","name","actor_id"}；inject 前置注入用 name
    pub timeout_ms: Option<u64>,
    pub params: Option<Value>,
}

pub struct AppState {
    pub config: ProviderConfig,
    pub idem: crate::idempotency::IdempotencyLedger,
    pub sessions: crate::session::SessionStore,
}
impl AppState {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            idem: crate::idempotency::IdempotencyLedger::new(),
            sessions: crate::session::SessionStore::new(),
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new().route("/webhook", post(handle_webhook)).with_state(state)
}

async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DownstreamRequest>,
) -> Response {
    match dispatch(&state, &headers, &req) {
        Ok(resp) => resp,
        Err(err) => err.into_response(),
    }
}

fn dispatch(state: &AppState, headers: &HeaderMap, req: &DownstreamRequest)
    -> Result<Response, BridgeError>
{
    // 1. token
    let auth = headers.get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok()).unwrap_or_default();
    let expected = format!("Bearer {}", state.config.bcs_to_provider_token);
    if auth != expected { return Err(BridgeError::unauthorized()); }
    // 2. provider_id
    if req.to_bot.provider_id != state.config.provider_id {
        return Err(BridgeError::provider_id_mismatch());
    }
    // 3. method
    match req.method.as_str() {
        "bot.ping" => Ok(Json(json!({"ok": true})).into_response()),
        "chat.send" | "chat.inject" | "chat.abort" | "interaction.resolve" => {
            // 后续任务实现；先返回 503 占位……
            Err(BridgeError::unavailable("not yet implemented"))
        }
        other => Err(BridgeError::unsupported_method(other)),
    }
}
