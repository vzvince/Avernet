use std::sync::Arc;
use axum::{extract::State, http::HeaderMap, response::{IntoResponse, Response}, routing::post, Json, Router};
use bcs_protocol::BCN_PROTOCOL_VERSION_HEADER;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{config::ProviderConfig, error::BridgeError, run::RunRegistry};

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
    pub runs: RunRegistry,
    // InteractionRegistry lands with Task 12.
}
impl AppState {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            idem: crate::idempotency::IdempotencyLedger::new(),
            sessions: crate::session::SessionStore::new(),
            runs: RunRegistry::new(),
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
    match dispatch(state, headers, req).await {
        Ok(resp) => resp,
        Err(err) => err.into_response(),
    }
}

async fn dispatch(
    state: Arc<AppState>,
    headers: HeaderMap,
    req: DownstreamRequest,
) -> Result<Response, BridgeError> {
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
        "chat.send" => handle_chat_send(state, headers, req).await,
        "chat.inject" | "chat.abort" | "interaction.resolve" => {
            // Tasks 12/13 implement these; chat.send is wired here.
            Err(BridgeError::unavailable("not yet implemented"))
        }
        other => Err(BridgeError::unsupported_method(other)),
    }
}

async fn handle_chat_send(
    state: Arc<AppState>,
    headers: HeaderMap,
    req: DownstreamRequest,
) -> Result<Response, BridgeError> {
    // 1. X-BCN-Protocol-Version: 2.0
    let pv = headers
        .get(BCN_PROTOCOL_VERSION_HEADER)
        .and_then(|v| v.to_str().ok());
    if pv != Some("2.0") {
        return Err(BridgeError::invalid_request("X-BCN-Protocol-Version 2.0 required"));
    }
    // 2. session_id present
    let session_id = req
        .session_id
        .as_ref()
        .ok_or_else(|| BridgeError::invalid_request("session_id required"))?
        .clone();
    // 3. message present
    if req.message.is_none() {
        return Err(BridgeError::invalid_request("message required"));
    }
    // 4. bot exists
    let bot = state
        .config
        .bot(&req.to_bot.provider_bot_ref)
        .ok_or_else(|| BridgeError::bot_not_found(&req.to_bot.provider_bot_ref))?
        .clone();

    let run_id = req.id.clone();
    let fp = crate::run::body_fingerprint(&req, &session_id);

    // 5. Idempotency: same id present?
    if let Some(handle) = state.runs.get(&run_id) {
        if !handle.matches(&fp) {
            // same id, different body → conflict
            return Err(BridgeError::conflict());
        }
        if handle.is_terminal() {
            // terminal: replay buffered frames as a fresh one-shot stream
            return Ok(crate::run::terminal_replay_response(handle));
        }
        // active: re-attach — replay buffer then follow broadcast
        return Ok(crate::run::sse_response(crate::run::forward_stream(handle)));
    }

    // 6. New run: reserve the session slot (429 if a different run is busy).
    state
        .sessions
        .try_start_run(&bot.provider_bot_ref, &session_id, &run_id)
        .await
        .map_err(|_| BridgeError::rate_limited())?;

    // 7. Atomic begin (handle re-attach race between get() and begin()).
    let (handle, is_new) = state.runs.begin(&run_id, fp);
    if is_new {
        return Ok(crate::run::spawn_run(state, req, bot, session_id, handle));
    }
    // Lost the race: another concurrent same-id request won. Re-attach (do not
    // spawn a duplicate driver). The slot we reserved is for the same run_id,
    // so releasing it would wrongly clear the winner's reservation — leave it.
    Ok(crate::run::sse_response(crate::run::forward_stream(handle)))
}
