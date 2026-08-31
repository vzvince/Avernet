//! `CfuseCodex` driver: maps codex SSE blocks (OpenAI Responses-API event
//! frames) to engine-neutral [`StreamEvent`]s and drives one downstream turn
//! over a [`crate::engine::cli::CliSession`].
//!
//! 调用形态（spec §4.2；cfuse 统一 CLI 的 codex 模式）：
//!
//! ```text
//! cfuse --codex --output-format sse --include-partial-messages
//!       [--permission-mode <mode>] [--model <model>]
//! ```
//!
//! 启动后向 stdin 写一条 codex Responses 输入帧（user message），随后按 SSE 块
//! 读取 stdout（[`CliSession::next_sse_block`]）。
//!
//! 会话 resume：aix-relay 的 codex runtime 跑的是 `codex app-server`（JSON-RPC），
//! resume 通过每轮 JSON-RPC 的 `session_id` 参数传递，**并非** `--resume` CLI
//! flag；而本驱动面向的是 `cfuse --codex` SSE 流式 CLI，其 resume 支持未经核实。
//! 因此 `engine_session_id` 恒为 `None`，后续上下文依赖调用方在下次 `chat.send`
//! 前置 pending injects（spec 允许的降级方案）。
//!
//! 事件映射表（codex SSE → [`CodexMap`] / [`StreamEvent`]）：
//!
//! | codex 事件 | 映射 |
//! | --- | --- |
//! | `response.output_text.delta` `{"delta":…}` | `chat_delta` |
//! | `response.completed` | `CodexMap::Final(累计文本)` |
//! | `response.failed` / `error` | `CodexMap::Failed(脱敏 message)` |
//! | 其余 | `CodexMap::Ignore` |

use std::path::PathBuf;

use bcs_protocol::stream::StreamEvent;
use serde_json::Value;

use crate::engine::cli::CliSession;
use crate::engine::{Engine, EngineKind, TurnError, TurnOutcome, TurnRequest};
use crate::sse;

/// `cfuse --codex` (codex Responses SSE) 引擎驱动。
pub struct CfuseCodex {
    bin: PathBuf,
}

impl CfuseCodex {
    pub fn new(bin: PathBuf) -> Self {
        Self { bin }
    }
}

/// 单个 codex SSE 块的映射结果。
///
/// `Events` 是该块产出的 [`StreamEvent`]；`Final` 携带 `response.completed` 的
/// 累计文本（若事件未携带则为空串，由 `run_turn` 用累计 deltas 兜底）；`Failed`
/// 携带 `response.failed`/`error` 经脱敏的退出原因（调用方上抛为
/// [`TurnError::EngineExited`]）；`Ignore` 标记“未识别或无产出”。
#[derive(Debug)]
pub(crate) enum CodexMap {
    Events(Vec<StreamEvent>),
    Final(String),
    Failed(String),
    Ignore,
}

/// 把一个 codex SSE 块（`event`/`data`）映射为引擎中立的 [`CodexMap`]。
///
/// 纯函数（无 IO），便于用录制 fixture 做单元测试；按上方映射表分派。
pub(crate) fn map_codex_block(event: &str, data: &str, run_id: &str) -> CodexMap {
    match event {
        "response.output_text.delta" => map_delta(data, run_id),
        "response.completed" => map_completed(data),
        "response.failed" | "error" => map_failed(data),
        _ => CodexMap::Ignore,
    }
}

fn map_delta(data: &str, run_id: &str) -> CodexMap {
    let value: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return CodexMap::Ignore,
    };
    match value.get("delta").and_then(|d| d.as_str()) {
        Some(text) => CodexMap::Events(vec![sse::chat_delta(run_id, text)]),
        None => CodexMap::Ignore,
    }
}

fn map_completed(data: &str) -> CodexMap {
    // 累计文本：`response.completed` 若携带 `response.output[].content[].text`
    // 则提取；fixture 无 output → 空串，`run_turn` 用累计 deltas 兜底。
    let text = serde_json::from_str::<Value>(data)
        .ok()
        .as_ref()
        .and_then(extract_completed_text)
        .unwrap_or_default();
    CodexMap::Final(text)
}

fn extract_completed_text(v: &Value) -> Option<String> {
    let response = v.get("response").unwrap_or(v);
    let output = response.get("output").and_then(|o| o.as_array())?;
    let mut text = String::new();
    for item in output {
        if item.get("type").and_then(|t| t.as_str()) != Some("message") {
            continue;
        }
        if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
            for part in content {
                if part.get("type").and_then(|t| t.as_str()) == Some("output_text") {
                    if let Some(t) = part.get("text").and_then(|x| x.as_str()) {
                        text.push_str(t);
                    }
                }
            }
        }
    }
    if text.is_empty() { None } else { Some(text) }
}

fn map_failed(data: &str) -> CodexMap {
    let raw = serde_json::from_str::<Value>(data)
        .ok()
        .as_ref()
        .and_then(extract_error_message)
        .unwrap_or_default();
    // JSON 无 message 字段时回退到脱敏后的原始 data（截断 + 剥控制字符）。
    let msg = if raw.is_empty() { data } else { &raw };
    CodexMap::Failed(sanitize_message(msg))
}

/// 从 `response.failed`/`error` 帧里抽取人类可读 message：
/// `{"error":{"message":…}}` 或顶层 `{"message":…}`。
fn extract_error_message(v: &Value) -> Option<String> {
    let err = v.get("error").unwrap_or(v);
    err.get("message").and_then(|m| m.as_str()).map(str::to_string)
}

/// 脱敏：剥控制字符（防日志注入），按字符截断到上限（UTF-8 安全，使用
/// `char_indices` 定位边界），不携带其它字段。message 本身是引擎错误描述，
/// 保留以保证下游可读；其它字段（code/raw payload）可能含敏感细节，不输出。
fn sanitize_message(msg: &str) -> String {
    const MAX_LEN: usize = 256;
    let clean: String = msg.chars().filter(|c| !c.is_control()).collect();
    match clean.char_indices().nth(MAX_LEN) {
        Some((idx, _)) => clean[..idx].to_string(),
        None => clean,
    }
}

#[async_trait::async_trait]
impl Engine for CfuseCodex {
    fn kind(&self) -> EngineKind {
        EngineKind::CfuseCodex
    }

    async fn run_turn(
        &self,
        req: TurnRequest,
        events: tokio::sync::mpsc::Sender<StreamEvent>,
        abort: tokio_util::sync::CancellationToken,
    ) -> Result<TurnOutcome, TurnError> {
        let mut args: Vec<String> = vec![
            "--codex".into(),
            "--output-format".into(),
            "sse".into(),
            "--include-partial-messages".into(),
        ];
        if let Some(mode) = &req.permission_mode {
            args.push("--permission-mode".into());
            args.push(mode.clone());
        }
        if let Some(model) = &req.model {
            args.push("--model".into());
            args.push(model.clone());
        }
        // cfuse codex SSE 模式无经核实的 --resume 等价物（见模块文档），故
        // `engine_session_id` 恒为 None：不拼 `--resume`，后续上下文由调用方
        // 前置 pending injects 保证（spec 允许的降级方案）。

        let mut cli = CliSession::spawn(&self.bin, &args, &req.cwd, &[])
            .await
            .map_err(TurnError::Spawn)?;

        // 启动后向 stdin 写一条 codex Responses user 输入帧。
        let envelope = serde_json::json!({
            "model": req.model,
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": req.prompt }]
            }],
            "stream": true,
        });
        let line = serde_json::to_string(&envelope)
            .map_err(|e| TurnError::Protocol(format!("encode codex input: {e}")))?;
        cli.write_line(&line).await.map_err(TurnError::Io)?;

        let mut deltas = String::new();
        loop {
            tokio::select! {
                _ = abort.cancelled() => {
                    cli.kill().await;
                    return Err(TurnError::Aborted);
                }
                block = cli.next_sse_block() => {
                    let Some((event, data)) = block.map_err(TurnError::Io)? else {
                        return Err(TurnError::EngineExited("stdout EOF before result".into()));
                    };
                    match map_codex_block(&event, &data, &req.run_id) {
                        CodexMap::Events(evs) => {
                            for ev in evs {
                                if let StreamEvent::Chat(c) = &ev {
                                    if let Some(t) = &c.delta_text {
                                        deltas.push_str(t);
                                    }
                                }
                                if events.send(ev).await.is_err() {
                                    cli.kill().await;
                                    return Err(TurnError::Aborted);
                                }
                            }
                        }
                        CodexMap::Final(text) => {
                            // 累计 deltas 形成最终文本；completed 事件若携带
                            // 完整 output 则优先，否则用 deltas 兜底。
                            let final_text = if text.is_empty() { deltas } else { text };
                            return Ok(TurnOutcome {
                                engine_session_id: None,
                                final_text: Some(final_text),
                            });
                        }
                        CodexMap::Failed(note) => {
                            cli.kill().await;
                            return Err(TurnError::EngineExited(note));
                        }
                        CodexMap::Ignore => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_codex_sse_turn() {
        let text = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/codex_turn.sse")).unwrap();
        let mut deltas = String::new();
        let mut final_seen = false;
        for block in text.split("\n\n").filter(|b| !b.trim().is_empty()) {
            let mut event = String::new();
            let mut data = String::new();
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event: ") { event = v.to_string(); }
                if let Some(v) = line.strip_prefix("data: ") { data = v.to_string(); }
            }
            match map_codex_block(&event, &data, "r-1") {
                CodexMap::Events(evs) => for ev in evs {
                    if let StreamEvent::Chat(c) = ev { deltas.push_str(&c.delta_text.unwrap_or_default()); }
                },
                CodexMap::Final(_) => final_seen = true,
                CodexMap::Failed(_) | CodexMap::Ignore => {}
            }
        }
        assert_eq!(deltas, "正在排查");
        assert!(final_seen);
    }

    #[test]
    fn maps_codex_failure() {
        match map_codex_block("response.failed",
            "{\"type\":\"response.failed\",\"error\":{\"message\":\"boom\"}}", "r-1") {
            CodexMap::Failed(msg) => assert!(msg.contains("boom")),
            _ => panic!("expected Failed"),
        }
    }

    #[test]
    fn error_event_maps_to_failed() {
        // 表中 `error` 事件同样映射 Failed。
        match map_codex_block("error", "{\"error\":{\"message\":\"upstream busy\"}}", "r-1") {
            CodexMap::Failed(msg) => assert!(msg.contains("upstream busy")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn delta_emits_chat_delta_event() {
        match map_codex_block(
            "response.output_text.delta",
            r#"{"type":"response.output_text.delta","delta":"hi"}"#,
            "r-9",
        ) {
            CodexMap::Events(evs) => {
                assert_eq!(evs.len(), 1);
                match &evs[0] {
                    StreamEvent::Chat(c) => {
                        assert_eq!(c.delta_text.as_deref(), Some("hi"));
                        assert_eq!(c.run_id, "r-9");
                    }
                    other => panic!("expected Chat, got {other:?}"),
                }
            }
            other => panic!("expected Events, got {other:?}"),
        }
    }

    #[test]
    fn completed_without_output_is_empty_final() {
        match map_codex_block(
            "response.completed",
            r#"{"type":"response.completed"}"#,
            "r-1",
        ) {
            CodexMap::Final(text) => assert_eq!(text, ""),
            other => panic!("expected Final, got {other:?}"),
        }
    }

    #[test]
    fn completed_with_output_text_extracts_accumulated_text() {
        let data = r#"{"type":"response.completed","response":{"output":[{"type":"message","content":[{"type":"output_text","text":"done"}]}]}}"#;
        match map_codex_block("response.completed", data, "r-1") {
            CodexMap::Final(text) => assert_eq!(text, "done"),
            other => panic!("expected Final, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_is_ignore() {
        assert!(matches!(map_codex_block("response.created", "{}", "r-1"), CodexMap::Ignore));
        assert!(matches!(map_codex_block("", "{}", "r-1"), CodexMap::Ignore));
    }

    #[test]
    fn delta_without_delta_field_is_ignore() {
        assert!(matches!(
            map_codex_block(
                "response.output_text.delta",
                r#"{"type":"response.output_text.delta"}"#,
                "r-1"
            ),
            CodexMap::Ignore
        ));
    }

    #[test]
    fn malformed_delta_json_is_ignore() {
        assert!(matches!(
            map_codex_block("response.output_text.delta", "{not json", "r-1"),
            CodexMap::Ignore
        ));
    }

    #[test]
    fn failed_with_control_chars_and_long_message_is_sanitized_and_bounded() {
        // 脱敏：剥控制字符，截断到 256 字符上限（UTF-8 安全）。
        let long = format!("{}{}", "boom", "\n".repeat(10));
        let data = format!(r#"{{"type":"response.failed","error":{{"message":{}}}}}"#,
            serde_json::to_string(&long).unwrap());
        match map_codex_block("response.failed", &data, "r-1") {
            CodexMap::Failed(msg) => {
                assert!(msg.contains("boom"));
                assert!(!msg.contains('\n'), "control chars must be stripped: {msg:?}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        let huge = "x".repeat(1024);
        let data = format!(r#"{{"type":"response.failed","error":{{"message":{}}}}}"#,
            serde_json::to_string(&huge).unwrap());
        match map_codex_block("response.failed", &data, "r-1") {
            CodexMap::Failed(msg) => assert_eq!(msg.chars().count(), 256),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn failed_without_message_field_falls_back_to_sanitized_raw_data() {
        match map_codex_block("response.failed", "{not even json", "r-1") {
            CodexMap::Failed(msg) => assert!(msg.contains("not even json")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn sanitize_message_truncation_is_utf8_safe_on_multibyte() {
        // UTF-8 安全：按字符边界截断，多字节字符不被切断。
        let s = "你".repeat(300);
        let out = sanitize_message(&s);
        assert_eq!(out.chars().count(), 256);
        assert!(out.chars().all(|c| c == '你'));
    }
}
