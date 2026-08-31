//! `CfuseCc` driver: maps claude `stream-json` NDJSON lines to engine-neutral
//! [`StreamEvent`]s and drives one downstream turn over a [`CliSession`].
//!
//! 调用形态（对齐 aix-relay `codefuse_direct_args`，spec §4.2）：
//!
//! ```text
//! cfuse --cc --output-format stream-json --verbose --input-format stream-json
//!       --include-partial-messages
//!       [--permission-mode <mode>] [--resume <engine_session_id>] [--model <model>]
//! ```
//!
//! 启动后立刻向 stdin 写一条 user 消息（claude stream-json 输入格式）。
//!
//! 事件映射表（cc stream-json → StreamEvent）：
//!
//! | cc 事件 | 映射 |
//! | --- | --- |
//! | `{"type":"system","subtype":"init","session_id":…}` | `CcMap::SessionId` |
//! | `{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":…}}}` | `chat_delta` |
//! | `{"type":"assistant","message":{"content":[{"type":"tool_use","id","name","input"}]}}` | `agent_tool(Start, name, toolCallId=id, args=input)` |
//! | `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id","content"}]}}` | `agent_tool(Result, toolCallId=tool_use_id, result=…)` |
//! | `{"type":"result","subtype":"success","result":…}` | `CcMap::Final(text)` |
//! | `{"type":"result","subtype":_}` 非成功 | `CcMap::Failed(note)` → `TurnError::EngineExited` |
//! | `{"type":"control_request","request":{"subtype":"can_use_tool",…}}` | `agent_thinking`（占位；Task 12 接线真实 approval，本任务绝不发 `stream:"approval"`） |

use std::path::PathBuf;

use bcs_protocol::stream::{StreamEvent, ToolData, ToolPhase};
use serde_json::Value;

use crate::engine::cli::CliSession;
use crate::engine::{Engine, EngineKind, TurnError, TurnOutcome, TurnRequest};
use crate::sse;

/// `cfuse --cc` (claude stream-json) 引擎驱动。
pub struct CfuseCc {
    bin: PathBuf,
}

impl CfuseCc {
    pub fn new(bin: PathBuf) -> Self {
        Self { bin }
    }
}

/// 单行 claude `stream-json` NDJSON 的映射结果。
///
/// `SessionId` 携带 `system/init` 的引擎内 session id；
/// `Final` 携带成功 `result` 的最终助手文本；
/// `Failed` 携带非成功 `result` 的经净化退出原因（调用方上抛为
/// `TurnError::EngineExited`）；`Events` 是该行产出的 [`StreamEvent`]；
/// `Ignore` 标记“JSON 合法但未识别”；`Malformed` 标记“非法 JSON”。
#[derive(Debug)]
pub(crate) enum CcMap {
    Events(Vec<StreamEvent>),
    SessionId(String),
    Final(String),
    Failed(String),
    Ignore,
    Malformed,
}

/// 把一行 claude `stream-json` NDJSON 映射为引擎中立的 [`CcMap`]。
///
/// 纯函数（无 IO），便于用录制 fixture 做单元测试；按上方映射表逐类分派。
pub(crate) fn map_cc_line(line: &str, run_id: &str) -> CcMap {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return CcMap::Malformed,
    };
    let ty = match value.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return CcMap::Ignore,
    };
    match ty {
        "system" => map_system(&value),
        "stream_event" => map_stream_event(&value, run_id),
        "assistant" => map_assistant(&value, run_id),
        "user" => map_user(&value, run_id),
        "result" => map_result(&value),
        "control_request" => map_control_request(&value, run_id),
        _ => CcMap::Ignore,
    }
}

fn map_system(v: &Value) -> CcMap {
    let subtype = v.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
    if subtype != "init" {
        return CcMap::Ignore;
    }
    match v.get("session_id").and_then(|x| x.as_str()) {
        Some(s) if !s.is_empty() => CcMap::SessionId(s.to_string()),
        _ => CcMap::Ignore,
    }
}

fn map_stream_event(v: &Value, run_id: &str) -> CcMap {
    let event = match v.get("event") {
        Some(e) => e,
        None => return CcMap::Ignore,
    };
    let etype = event.get("type").and_then(|x| x.as_str()).unwrap_or("");
    if etype != "content_block_delta" {
        return CcMap::Ignore;
    }
    let delta = match event.get("delta") {
        Some(d) => d,
        None => return CcMap::Ignore,
    };
    let dtype = delta.get("type").and_then(|x| x.as_str()).unwrap_or("");
    if dtype != "text_delta" {
        return CcMap::Ignore;
    }
    match delta.get("text").and_then(|x| x.as_str()) {
        Some(text) => CcMap::Events(vec![sse::chat_delta(run_id, text)]),
        None => CcMap::Ignore,
    }
}

fn map_assistant(v: &Value, run_id: &str) -> CcMap {
    let content = match v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(arr) => arr,
        None => return CcMap::Ignore,
    };
    let mut events = Vec::new();
    for item in content {
        let itype = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if itype != "tool_use" {
            continue;
        }
        let id = item.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
        let input = item.get("input").cloned().unwrap_or(Value::Null);
        events.push(sse::agent_tool(
            run_id,
            ToolData {
                phase: ToolPhase::Start,
                name: Some(name.to_string()),
                tool_call_id: Some(id.to_string()),
                is_error: None,
                exit_code: None,
                duration_ms: None,
                cwd: None,
                args: Some(input),
                result: None,
                partial_result: None,
            },
        ));
    }
    if events.is_empty() {
        CcMap::Ignore
    } else {
        CcMap::Events(events)
    }
}

fn map_user(v: &Value, run_id: &str) -> CcMap {
    let content = match v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(arr) => arr,
        None => return CcMap::Ignore,
    };
    let mut events = Vec::new();
    for item in content {
        let itype = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if itype != "tool_result" {
            continue;
        }
        let tool_use_id = item.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("");
        let content_val = item.get("content").cloned().unwrap_or(Value::Null);
        events.push(sse::agent_tool(
            run_id,
            ToolData {
                phase: ToolPhase::Result,
                name: None,
                tool_call_id: Some(tool_use_id.to_string()),
                is_error: None,
                exit_code: None,
                duration_ms: None,
                cwd: None,
                args: None,
                result: Some(content_val),
                partial_result: None,
            },
        ));
    }
    if events.is_empty() {
        CcMap::Ignore
    } else {
        CcMap::Events(events)
    }
}

fn map_result(v: &Value) -> CcMap {
    let subtype = v.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
    if subtype == "success" {
        let text = v.get("result").and_then(|x| x.as_str()).unwrap_or("").to_string();
        return CcMap::Final(text);
    }
    // 非成功 result：上抛为引擎退出。净化——只保留引擎错误类别标识符，
    // 绝不把原始 payload（可能含敏感细节）灌进错误消息。
    CcMap::Failed(sanitize_result_note(subtype))
}

/// 只保留引擎错误类别（有限标识符：字母数字/下划线/连字符），剥掉其它。
fn sanitize_result_note(subtype: &str) -> String {
    let clean: String = subtype
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if clean.is_empty() {
        "engine returned non-success result".to_string()
    } else {
        format!("engine result subtype: {clean}")
    }
}

/// `control_request` 占位映射：Task 12 接线真实 approval 交互流。
///
/// 本阶段产出一个可观测的 `agent_thinking` 事件（带 can_use_tool/tool_name
/// 标记），保证该轮可见；绝不发 `stream:"approval"`（spec §2 禁止旧 approval
/// 用于新接入）。
fn map_control_request(v: &Value, run_id: &str) -> CcMap {
    let req = match v.get("request") {
        Some(r) => r,
        None => return CcMap::Ignore,
    };
    let subtype = req.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
    if subtype != "can_use_tool" {
        return CcMap::Ignore;
    }
    let tool_name = req.get("tool_name").and_then(|x| x.as_str()).unwrap_or("tool");
    let note = format!("can_use_tool: {tool_name}");
    CcMap::Events(vec![sse::agent_thinking(run_id, Some(note), None)])
}

#[async_trait::async_trait]
impl Engine for CfuseCc {
    fn kind(&self) -> EngineKind {
        EngineKind::CfuseCc
    }

    async fn run_turn(
        &self,
        req: TurnRequest,
        events: tokio::sync::mpsc::Sender<StreamEvent>,
        abort: tokio_util::sync::CancellationToken,
    ) -> Result<TurnOutcome, TurnError> {
        let mut args: Vec<String> = vec![
            "--cc".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--include-partial-messages".into(),
        ];
        if let Some(mode) = &req.permission_mode {
            args.push("--permission-mode".into());
            args.push(mode.clone());
        }
        if let Some(sid) = &req.engine_session_id {
            args.push("--resume".into());
            args.push(sid.clone());
        }
        if let Some(model) = &req.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        let mut cli = CliSession::spawn(&self.bin, &args, &req.cwd, &[])
            .await
            .map_err(TurnError::Spawn)?;

        // 启动后立刻向 stdin 写一条 user 消息（claude stream-json 输入格式）。
        let user_msg = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": req.prompt }]
            }
        });
        let user_line = serde_json::to_string(&user_msg)
            .map_err(|e| TurnError::Protocol(format!("encode user message: {e}")))?;
        cli.write_line(&user_line).await.map_err(TurnError::Io)?;

        let mut engine_session_id = req.engine_session_id.clone();
        loop {
            tokio::select! {
                _ = abort.cancelled() => {
                    cli.kill().await;
                    return Err(TurnError::Aborted);
                }
                line = cli.next_line() => {
                    let Some(line) = line.map_err(TurnError::Io)? else {
                        return Err(TurnError::EngineExited("stdout EOF before result".into()));
                    };
                    match map_cc_line(&line, &req.run_id) {
                        CcMap::SessionId(s) => engine_session_id = Some(s),
                        CcMap::Events(evs) => for ev in evs {
                            if events.send(ev).await.is_err() {
                                cli.kill().await;
                                return Err(TurnError::Aborted);
                            }
                        },
                        CcMap::Final(text) => {
                            return Ok(TurnOutcome { engine_session_id, final_text: Some(text) });
                        }
                        CcMap::Failed(note) => {
                            cli.kill().await;
                            return Err(TurnError::EngineExited(note));
                        }
                        CcMap::Ignore | CcMap::Malformed => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcs_protocol::stream::{ChatState, StreamEvent};

    #[test]
    fn maps_cc_ndjson_turn() {
        let lines: Vec<String> = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cc_turn.ndjson"))
            .unwrap().lines().map(str::to_string).collect();
        let mut session_id = None;
        let mut deltas = String::new();
        let mut tools = 0;
        let mut final_text = None;
        for line in &lines {
            match map_cc_line(line, "r-1") {
                CcMap::SessionId(s) => session_id = Some(s),
                CcMap::Events(events) => for ev in events {
                    match ev {
                        StreamEvent::Chat(c) if c.state == ChatState::Delta =>
                            deltas.push_str(&c.delta_text.unwrap()),
                        StreamEvent::Agent(_) => tools += 1,
                        _ => {}
                    }
                },
                CcMap::Final(text) => final_text = Some(text),
                CcMap::Failed(_) => {}
                CcMap::Ignore | CcMap::Malformed => {}
            }
        }
        assert_eq!(session_id.as_deref(), Some("cc-sess-1"));
        assert_eq!(deltas, "正在分析");
        assert_eq!(tools, 2);
        assert_eq!(final_text.as_deref(), Some("完成了"));
    }

    #[test]
    fn malformed_json_is_malformed() {
        assert!(matches!(map_cc_line("not json", "r-1"), CcMap::Malformed));
        assert!(matches!(map_cc_line("{", "r-1"), CcMap::Malformed));
    }

    #[test]
    fn unrecognized_type_is_ignore() {
        let line = r#"{"type":"mystery","payload":42}"#;
        assert!(matches!(map_cc_line(line, "r-1"), CcMap::Ignore));
    }

    #[test]
    fn json_without_type_is_ignore() {
        let line = r#"{"hello":"world"}"#;
        assert!(matches!(map_cc_line(line, "r-1"), CcMap::Ignore));
    }

    #[test]
    fn non_success_result_is_failed_with_sanitized_note() {
        // error subtype 标识符被保留（引擎错误类别，非用户数据）。
        let line = r#"{"type":"result","subtype":"error_max_cycles","result":"secret detail"}"#;
        match map_cc_line(line, "r-1") {
            CcMap::Failed(note) => {
                assert!(note.contains("error_max_cycles"), "note: {note}");
                // 不携带原始 result payload。
                assert!(!note.contains("secret detail"), "note leaked payload: {note}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn empty_result_text_success_maps_to_empty_final() {
        let line = r#"{"type":"result","subtype":"success","result":""}"#;
        match map_cc_line(line, "r-1") {
            CcMap::Final(text) => assert_eq!(text, ""),
            other => panic!("expected Final, got {other:?}"),
        }
    }

    #[test]
    fn control_request_can_use_tool_emits_thinking_not_approval() {
        let line = r#"{"type":"control_request","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"rm -rf /"}}}"#;
        match map_cc_line(line, "r-1") {
            CcMap::Events(evs) => {
                assert_eq!(evs.len(), 1, "exactly one placeholder thinking event");
                match &evs[0] {
                    StreamEvent::Agent(a) => match &a.data {
                        bcs_protocol::stream::AgentData::Thinking(t) => {
                            let delta = t.delta.as_deref().unwrap_or("");
                            assert!(delta.contains("can_use_tool"), "delta: {delta}");
                            assert!(delta.contains("Bash"), "delta: {delta}");
                        }
                        other => panic!("expected Thinking, got {other:?}"),
                    },
                    other => panic!("expected Agent(Thinking), got {other:?}"),
                }
            }
            other => panic!("expected Events, got {other:?}"),
        }
    }

    #[test]
    fn non_can_use_tool_control_request_is_ignore() {
        let line = r#"{"type":"control_request","request":{"subtype":"other"}}"#;
        assert!(matches!(map_cc_line(line, "r-1"), CcMap::Ignore));
    }

    #[test]
    fn assistant_tool_use_carries_id_name_and_input() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_9","name":"Read","input":{"path":"/a"}}]}}"#;
        match map_cc_line(line, "r-9") {
            CcMap::Events(evs) => {
                assert_eq!(evs.len(), 1);
                match &evs[0] {
                    StreamEvent::Agent(a) => match &a.data {
                        bcs_protocol::stream::AgentData::Tool(t) => {
                            assert_eq!(t.phase, ToolPhase::Start);
                            assert_eq!(t.name.as_deref(), Some("Read"));
                            assert_eq!(t.tool_call_id.as_deref(), Some("toolu_9"));
                            assert_eq!(t.args.as_ref().unwrap()["path"], serde_json::json!("/a"));
                        }
                        other => panic!("expected Tool, got {other:?}"),
                    },
                    other => panic!("expected Agent(Tool), got {other:?}"),
                }
            }
            other => panic!("expected Events, got {other:?}"),
        }
    }

    #[test]
    fn user_tool_result_carries_tool_use_id_and_content_zero_loss() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_9","content":[{"type":"text","text":"ok"}]}]}}"#;
        match map_cc_line(line, "r-9") {
            CcMap::Events(evs) => {
                assert_eq!(evs.len(), 1);
                match &evs[0] {
                    StreamEvent::Agent(a) => match &a.data {
                        bcs_protocol::stream::AgentData::Tool(t) => {
                            assert_eq!(t.phase, ToolPhase::Result);
                            assert_eq!(t.tool_call_id.as_deref(), Some("toolu_9"));
                            assert_eq!(t.result.as_ref().unwrap()[0]["text"], serde_json::json!("ok"));
                        }
                        other => panic!("expected Tool, got {other:?}"),
                    },
                    other => panic!("expected Agent(Tool), got {other:?}"),
                }
            }
            other => panic!("expected Events, got {other:?}"),
        }
    }

    #[test]
    fn assistant_without_tool_use_is_ignore() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#;
        // 文本块由 stream_event 的 deltas 承载；assistant 非工具块不重复映射。
        assert!(matches!(map_cc_line(line, "r-1"), CcMap::Ignore));
    }

    #[test]
    fn system_non_init_is_ignore() {
        let line = r#"{"type":"system","subtype":"other","session_id":"s"}"#;
        assert!(matches!(map_cc_line(line, "r-1"), CcMap::Ignore));
    }

    #[test]
    fn stream_event_non_text_delta_is_ignore() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{"}}}"#;
        assert!(matches!(map_cc_line(line, "r-1"), CcMap::Ignore));
    }
}
