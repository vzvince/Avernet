//! `CfuseCodexAppServer` driver: runs `cfuse --codex app-server` over the
//! Codex JSON-RPC stdio protocol.
//!
//! This follows the production Codex path used by aix-engine:
//!
//! ```text
//! cfuse --codex app-server --listen stdio://
//!   initialize
//!   thread/start | thread/resume
//!   turn/start
//!   item/agentMessage/delta ...
//!   turn/completed
//! ```
//!
//! Unlike `codex exec --json`, app-server emits assistant text as small delta
//! notifications. The driver forwards those notifications immediately as
//! engine-neutral `chat_delta` events and uses `turn/completed` only as the
//! terminal boundary.

use std::collections::VecDeque;
use std::path::PathBuf;

use bcs_protocol::stream::StreamEvent;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::engine::cli::CliSession;
use crate::engine::{Engine, EngineKind, TurnError, TurnOutcome, TurnRequest};
use crate::sse;

/// Codex app-server JSON-RPC driver.
pub struct CfuseCodexAppServer {
    bin: PathBuf,
}

impl CfuseCodexAppServer {
    pub fn new(bin: PathBuf) -> Self {
        Self { bin }
    }
}

#[async_trait::async_trait]
impl Engine for CfuseCodexAppServer {
    fn kind(&self) -> EngineKind {
        EngineKind::CfuseCodex
    }

    async fn run_turn(
        &self,
        req: TurnRequest,
        events: Sender<StreamEvent>,
        abort: CancellationToken,
    ) -> Result<TurnOutcome, TurnError> {
        let args = vec![
            "--codex".to_string(),
            "app-server".to_string(),
            "--listen".to_string(),
            "stdio://".to_string(),
        ];
        let mut cli = CliSession::spawn(&self.bin, &args, &req.cwd, &[])
            .await
            .map_err(TurnError::Spawn)?;

        let mut next_id = 1_u64;
        let mut backlog = VecDeque::new();

        rpc_call(
            &mut cli,
            &mut next_id,
            &mut backlog,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "bridge-provider",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": null,
            }),
        )
        .await?;
        send_notification(&mut cli, "initialized", Value::Null).await?;

        let thread = if let Some(session_id) = &req.engine_session_id {
            rpc_call(
                &mut cli,
                &mut next_id,
                &mut backlog,
                "thread/resume",
                thread_resume_params(&req, session_id),
            )
            .await?
        } else {
            rpc_call(
                &mut cli,
                &mut next_id,
                &mut backlog,
                "thread/start",
                thread_start_params(&req),
            )
            .await?
        };
        let thread_id = extract_thread_id(&thread).ok_or_else(|| {
            TurnError::Protocol(format!("app-server thread response missing thread.id: {thread}"))
        })?;

        let turn = rpc_call(
            &mut cli,
            &mut next_id,
            &mut backlog,
            "turn/start",
            turn_start_params(&req, &thread_id),
        )
        .await?;
        let turn_id = extract_turn_id(&turn).ok_or_else(|| {
            TurnError::Protocol(format!("app-server turn response missing turn.id: {turn}"))
        })?;

        let mut deltas = String::new();
        loop {
            let value = tokio::select! {
                _ = abort.cancelled() => {
                    cli.kill().await;
                    return Err(TurnError::Aborted);
                }
                value = next_message(&mut cli, &mut backlog) => value?,
            };

            if is_server_request(&value) {
                respond_server_request(&mut cli, &value).await?;
                continue;
            }
            if !matches_turn(&value, &thread_id, &turn_id) {
                continue;
            }

            match value.get("method").and_then(Value::as_str).unwrap_or_default() {
                "item/agentMessage/delta" | "agent/output_chunk" => {
                    if let Some(delta) = extract_delta(&value) {
                        deltas.push_str(&delta);
                        if events.send(sse::chat_delta(&req.run_id, &delta)).await.is_err() {
                            cli.kill().await;
                            return Err(TurnError::Aborted);
                        }
                    }
                }
                "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
                    if let Some(delta) = extract_delta(&value)
                        && events
                            .send(sse::agent_thinking(&req.run_id, Some(delta), None))
                            .await
                            .is_err()
                    {
                        cli.kill().await;
                        return Err(TurnError::Aborted);
                    }
                }
                "turn/completed" | "agent/turn_completed" => {
                    if let Some(message) = completed_failure_message(&value) {
                        return Err(TurnError::EngineExited(message));
                    }
                    let final_text = if deltas.is_empty() {
                        completed_text(&value).unwrap_or_default()
                    } else {
                        deltas
                    };
                    cli.close_stdin();
                    return Ok(TurnOutcome {
                        engine_session_id: Some(thread_id),
                        final_text: Some(final_text),
                    });
                }
                "agent/turn_failed" => {
                    return Err(TurnError::EngineExited(
                        turn_failure_message(&value),
                    ));
                }
                _ => {}
            }
        }
    }
}

async fn rpc_call(
    cli: &mut CliSession,
    next_id: &mut u64,
    backlog: &mut VecDeque<Value>,
    method: &str,
    params: Value,
) -> Result<Value, TurnError> {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    send_line(cli, &request).await?;

    loop {
        let value = next_message(cli, backlog).await?;
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("app-server RPC failed");
                return Err(TurnError::EngineExited(sanitize_message(message)));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
        if value.get("method").is_some() {
            backlog.push_back(value);
        }
    }
}

async fn send_notification(
    cli: &mut CliSession,
    method: &str,
    params: Value,
) -> Result<(), TurnError> {
    send_line(
        cli,
        &json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }),
    )
    .await
}

async fn send_line(cli: &mut CliSession, value: &Value) -> Result<(), TurnError> {
    let line = serde_json::to_string(value)
        .map_err(|error| TurnError::Protocol(format!("encode app-server request: {error}")))?;
    cli.write_line(&line).await.map_err(TurnError::Io)
}

async fn next_message(
    cli: &mut CliSession,
    backlog: &mut VecDeque<Value>,
) -> Result<Value, TurnError> {
    if let Some(value) = backlog.pop_front() {
        return Ok(value);
    }
    let Some(line) = cli.next_line().await.map_err(TurnError::Io)? else {
        return Err(TurnError::EngineExited(
            "app-server stdout EOF before result".into(),
        ));
    };
    serde_json::from_str(&line)
        .map_err(|error| TurnError::Protocol(format!("parse app-server JSON: {error}")))
}

async fn respond_server_request(cli: &mut CliSession, request: &Value) -> Result<(), TurnError> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    send_line(
        cli,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": "bridge-provider does not support app-server server requests",
            },
        }),
    )
    .await
}

fn thread_start_params(req: &TurnRequest) -> Value {
    let mut params = json!({
        "cwd": req.cwd,
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "sandbox": "read-only",
        "ephemeral": false,
        "threadSource": "bridge-provider",
    });
    if let Some(model) = req.model.as_deref().filter(|model| !model.is_empty()) {
        params["model"] = json!(model);
    }
    params
}

fn thread_resume_params(req: &TurnRequest, thread_id: &str) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "cwd": req.cwd,
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "sandbox": "read-only",
    });
    if let Some(model) = req.model.as_deref().filter(|model| !model.is_empty()) {
        params["model"] = json!(model);
    }
    params
}

fn turn_start_params(req: &TurnRequest, thread_id: &str) -> Value {
    json!({
        "threadId": thread_id,
        "clientUserMessageId": req.run_id,
        "input": [{
            "type": "text",
            "text": req.prompt,
            "text_elements": [],
        }],
        "cwd": req.cwd,
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "sandboxPolicy": {
            "type": "readOnly",
            "networkAccess": false,
        },
    })
}

fn extract_thread_id(value: &Value) -> Option<String> {
    value
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .or_else(|| value.get("threadId").and_then(Value::as_str))
        .map(str::to_string)
}

fn extract_turn_id(value: &Value) -> Option<String> {
    value
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .or_else(|| value.get("turnId").and_then(Value::as_str))
        .map(str::to_string)
}

fn matches_turn(value: &Value, thread_id: &str, turn_id: &str) -> bool {
    let Some(params) = value.get("params") else {
        return false;
    };
    let thread_matches = params
        .get("threadId")
        .and_then(Value::as_str)
        .is_none_or(|value| value == thread_id);
    let turn_matches = params
        .get("turnId")
        .and_then(Value::as_str)
        .is_none_or(|value| value == turn_id);
    thread_matches && turn_matches
}

fn is_server_request(value: &Value) -> bool {
    value.get("id").is_some()
        && value.get("method").is_some()
        && value.get("result").is_none()
        && value.get("error").is_none()
}

fn extract_delta(value: &Value) -> Option<String> {
    let params = value.get("params")?;
    params
        .get("delta")
        .and_then(Value::as_str)
        .or_else(|| params.get("text").and_then(Value::as_str))
        .or_else(|| {
            params
                .get("delta")
                .and_then(|delta| delta.get("text"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn completed_text(value: &Value) -> Option<String> {
    let params = value.get("params")?;
    params
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| params.get("finalText").and_then(Value::as_str))
        .or_else(|| params.get("final_text").and_then(Value::as_str))
        .map(str::to_string)
}

fn completed_failure_message(value: &Value) -> Option<String> {
    let turn = value.pointer("/params/turn")?;
    if turn.get("status").and_then(Value::as_str) != Some("failed") {
        return None;
    }
    let message = turn
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("app-server turn failed");
    Some(sanitize_message(message))
}

fn turn_failure_message(value: &Value) -> String {
    let message = value
        .pointer("/params/error/message")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/params/message").and_then(Value::as_str))
        .unwrap_or("app-server turn failed");
    sanitize_message(message)
}

fn sanitize_message(message: &str) -> String {
    const MAX_LEN: usize = 256;
    let clean: String = message.chars().filter(|c| !c.is_control()).collect();
    match clean.char_indices().nth(MAX_LEN) {
        Some((index, _)) => clean[..index].to_string(),
        None => clean,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_app_server_ids() {
        assert_eq!(
            extract_thread_id(&json!({"thread": {"id": "t-1"}})).as_deref(),
            Some("t-1")
        );
        assert_eq!(
            extract_turn_id(&json!({"turn": {"id": "turn-1"}})).as_deref(),
            Some("turn-1")
        );
    }

    #[test]
    fn extracts_streaming_delta_shapes() {
        assert_eq!(
            extract_delta(&json!({"params": {"delta": "hello"}})).as_deref(),
            Some("hello")
        );
        assert_eq!(
            extract_delta(&json!({"params": {"text": "world"}})).as_deref(),
            Some("world")
        );
    }

    #[test]
    fn matches_only_the_active_turn() {
        let value = json!({
            "params": {"threadId": "t-1", "turnId": "turn-1"}
        });
        assert!(matches_turn(&value, "t-1", "turn-1"));
        assert!(!matches_turn(&value, "t-2", "turn-1"));
        assert!(!matches_turn(&value, "t-1", "turn-2"));
    }
}
