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
//! | `{"type":"control_request","request":{"subtype":"can_use_tool",…}}` | `CcMap::ControlRequest { request_id, tool_name, input }` → 驱动接线 HITL 交互（register→requested→await resolution→control_response→resolved） |

use std::path::PathBuf;

use bcs_protocol::stream::{InteractionKind, InteractionPhase, StreamEvent, ToolData, ToolPhase};
use serde_json::{json, Value};

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
/// `Ignore` 标记“JSON 合法但未识别”；`Malformed` 标记“非法 JSON”；
/// `ControlRequest` 携带 `can_use_tool` 的 request_id/tool_name/input，由
/// `run_turn` 接线为 HITL 交互（Task 12）。
#[derive(Debug)]
pub(crate) enum CcMap {
    Events(Vec<StreamEvent>),
    SessionId(String),
    Final(String),
    Failed(String),
    Ignore,
    Malformed,
    /// Engine permission request (`can_use_tool`). Fields surfaced to the driver
    /// for the interaction roundtrip; `request_id` is engine-native and stays
    /// inside the driver (never sent to BCS as an interactionId).
    ControlRequest {
        request_id: String,
        tool_name: String,
        input: Value,
    },
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

/// `control_request` 映射：`can_use_tool` 提取为 [`CcMap::ControlRequest`]，
/// 由 `run_turn` 接线 HITL 交互（register→requested→await→control_response
/// →resolved）。`can_use_tool` 以外的子类型忽略；绝不发 `stream:"approval"`
/// （spec §2 禁止旧 approval 用于新接入）。
fn map_control_request(v: &Value, _run_id: &str) -> CcMap {
    let req = match v.get("request") {
        Some(r) => r,
        None => return CcMap::Ignore,
    };
    let subtype = req.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
    if subtype != "can_use_tool" {
        return CcMap::Ignore;
    }
    let request_id = req.get("request_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let tool_name = req.get("tool_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let input = req.get("input").cloned().unwrap_or(Value::Null);
    CcMap::ControlRequest { request_id, tool_name, input }
}

/// Build the kind-specific `extra` payload for the `interaction/requested` SSE
/// event (per spec §5.2).
///
/// - exec (`tool_name != "AskUserQuestion"`): `{title, command, options}` —
///   `title` = tool_name, `command` from `input.command` (else null), and the
///   fixed options `[allow_once, deny]`.
/// - ask_user (`AskUserQuestion`): `{questions}` from `input.questions[]` —
///   questionId = `header` fallback `question_N`, options `label → {label,
///   value=label}` (cc has no separate value; baas-fallback parity).
fn build_requested_extra(tool_name: &str, input: &Value) -> Value {
    if tool_name == "AskUserQuestion" {
        json!({ "questions": map_ask_user_questions(input) })
    } else {
        let command = input.get("command").cloned().unwrap_or(Value::Null);
        json!({
            "title": tool_name,
            "command": command,
            "options": [
                { "decision": "allow_once", "label": "Allow once" },
                { "decision": "deny", "label": "Deny" },
            ],
        })
    }
}

/// Map cc `AskUserQuestion` `input.questions[]` to BCN questions. See
/// [`build_requested_extra`]: `questionId` 取 `header`，缺省回落到
/// `question_N`（1-based 索引）；`options[].value` 以 `label` 回填（cc 无独立
/// value，对齐 baas fallback 策略）；`multiSelect` 透传。**注意**：含
/// `secret`/`isSecret` 标记的问题在本函数之前已被 [`ask_user_has_secret`]
/// 拦截并应答 deny，因此不会走到本映射。
fn map_ask_user_questions(input: &Value) -> Value {
    let Some(questions) = input.get("questions").and_then(|q| q.as_array()) else {
        return json!([]);
    };
    let out: Vec<Value> = questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let question_id = q
                .get("header")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("question_{}", i + 1));
            let question = q.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let options = q
                .get("options")
                .and_then(|o| o.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|o| {
                            let label = o.get("label").and_then(|v| v.as_str()).unwrap_or("");
                            json!({ "label": label, "value": label })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut obj = json!({
                "questionId": question_id,
                "question": question,
                "options": options,
            });
            if let Some(ms) = q.get("multiSelect").and_then(|v| v.as_bool()) {
                obj["multiSelect"] = json!(ms);
            }
            obj
        })
        .collect();
    json!(out)
}

/// Refuse conversion when any `AskUserQuestion` question carries a `secret`
/// (spec §5.2): BCS interaction cannot ferry secret answers — the driver
/// answers the engine `deny` directly and emits no interaction. Checks both
/// the question-level `secret`/`isSecret` boolean and (defensively) a
/// whole-call `isSecret` flag.
fn ask_user_has_secret(input: &Value) -> bool {
    if input.get("secret").and_then(|v| v.as_bool()) == Some(true)
        || input.get("isSecret").and_then(|v| v.as_bool()) == Some(true)
    {
        return true;
    }
    let Some(questions) = input.get("questions").and_then(|q| q.as_array()) else {
        return false;
    };
    questions.iter().any(|q| {
        q.get("secret").and_then(|v| v.as_bool()) == Some(true)
            || q.get("isSecret").and_then(|v| v.as_bool()) == Some(true)
    })
}

/// Map a BCS resolution value to the cc control_response `behavior`
/// (`"allow"`/`"deny"`).
///
/// v1 limitation (spec §5.2): cc has no answers channel, so an ask_user
/// resolution collapses to allow/deny only — exec `decision:"deny"` and
/// ask_user `action:"cancel"` map to `deny`; a non-deny exec decision maps to
/// `allow`; ask_user `action:"answer"` maps to `allow` only when `answers` is a
/// non-empty array (cancel and missing/empty answers map to `deny`). The
/// answers payload itself is dropped (cc v1 cannot consume it).
fn resolution_to_behavior(resolution: &Value) -> &'static str {
    if let Some(d) = resolution["decision"].as_str() {
        return if d == "deny" { "deny" } else { "allow" };
    }
    match resolution["action"].as_str() {
        Some("answer") => {
            let has_answers = resolution["answers"]
                .as_array()
                .map_or(false, |a| !a.is_empty());
            if has_answers { "allow" } else { "deny" }
        }
        _ => "deny",
    }
}

/// Drive one `can_use_tool` control request as a HITL interaction (Task 12):
///
/// 1. (ask_user only) refuse secret-marked questions — answer the engine `deny`
///    immediately and log; no interaction is emitted (spec §5.2).
/// 2. register a pending interaction with the registry → mint interactionId.
/// 3. emit `interaction/requested` (exec: title/command/options; ask_user:
///    questions).
/// 4. await the BCS resolution (or abort → deny fallback).
/// 5. write the cc `control_response` back to the engine's control channel
///    (`behavior` allow/deny; `updatedInput` only on allow).
/// 6. emit `interaction/resolved`.
///
/// Returns `Ok(())` so the `run_turn` loop continues to the next line; an IO
/// failure writing the control_response is returned as `TurnError::Io`, and a
/// failed `requested` send (BCS disconnect) aborts the engine.
async fn handle_control_request(
    cli: &mut CliSession,
    events: &tokio::sync::mpsc::Sender<StreamEvent>,
    abort: &tokio_util::sync::CancellationToken,
    req: &TurnRequest,
    request_id: String,
    tool_name: String,
    input: Value,
) -> Result<(), TurnError> {
    let kind = if tool_name == "AskUserQuestion" {
        InteractionKind::AskUser
    } else {
        InteractionKind::Exec
    };
    // 1. Secret-marked ask_user questions are refused (spec §5.2): the engine
    //    gets deny and no interaction is emitted to BCS.
    if kind == InteractionKind::AskUser && ask_user_has_secret(&input) {
        tracing::warn!(
            target: "bridge_provider",
            request_id = %request_id, tool = %tool_name,
            "AskUserQuestion with secret-marked question refused; answering deny"
        );
        let deny = json!({
            "type": "control_response",
            "response": { "request_id": request_id,
                          "response": { "behavior": "deny", "updatedInput": null } }
        });
        cli.write_line(&deny.to_string()).await.map_err(TurnError::Io)?;
        return Ok(());
    }
    // 2-3. Register + emit requested.
    let (iid, resolution_rx) = req.interactions.register(&req.run_id, kind, request_id.clone());
    let requested = build_requested_extra(&tool_name, &input);
    if events
        .send(sse::interaction_event(
            &req.run_id,
            InteractionPhase::Requested,
            kind,
            &iid,
            requested,
        ))
        .await
        .is_err()
    {
        cli.kill().await;
        return Err(TurnError::Aborted);
    }
    // 4. Await the BCS resolution; abort or a dropped sender recovers to deny
    //    so the driver never blocks on a dead interaction.
    let resolution = tokio::select! {
        _ = abort.cancelled() => json!({ "decision": "deny" }),
        r = resolution_rx => r.unwrap_or_else(|_| json!({ "decision": "deny" })),
    };
    // 5. Write the control_response with the mapped behavior.
    let behavior = resolution_to_behavior(&resolution);
    let updated_input = if behavior == "allow" { Some(input.clone()) } else { None };
    let response = json!({
        "type": "control_response",
        "response": { "request_id": request_id,
                      "response": { "behavior": behavior,
                                    "updatedInput": updated_input } }
    });
    cli.write_line(&response.to_string()).await.map_err(TurnError::Io)?;
    // 6. Emit resolved (best-effort — BCS may have disconnected post-request).
    // v1 limitation: ask_user resolutions carry no `decision` key, so the
    // resolved event's `decision` is null for ask_user (consistent with the
    // no-answers-channel collapse above).
    let _ = events
        .send(sse::interaction_event(
            &req.run_id,
            InteractionPhase::Resolved,
            kind,
            &iid,
            json!({ "decision": resolution["decision"].clone() }),
        ))
        .await;
    Ok(())
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
                        CcMap::ControlRequest { request_id, tool_name, input } => {
                            handle_control_request(
                                &mut cli,
                                &events,
                                &abort,
                                &req,
                                request_id,
                                tool_name,
                                input,
                            )
                            .await?;
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
                CcMap::ControlRequest { .. } => {}
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
    fn control_request_can_use_tool_maps_to_control_request() {
        // 不再发占位 thinking（Task 9）——Task 12 接线为 ControlRequest 载体，
        // 由 run_turn 驱动 HITL 交互；绝不发 `stream:"approval"`（spec §2）。
        let line = r#"{"type":"control_request","request":{"subtype":"can_use_tool","request_id":"req-7","tool_name":"Bash","input":{"command":"rm -rf /"}}}"#;
        match map_cc_line(line, "r-1") {
            CcMap::ControlRequest { request_id, tool_name, input } => {
                assert_eq!(request_id, "req-7");
                assert_eq!(tool_name, "Bash");
                assert_eq!(input["command"], json!("rm -rf /"));
            }
            other => panic!("expected ControlRequest, got {other:?}"),
        }
    }

    #[test]
    fn build_requested_extra_exec_carries_title_command_and_fixed_options() {
        let extra = build_requested_extra("Bash", &json!({"command":"ls -la"}));
        assert_eq!(extra["title"], json!("Bash"));
        assert_eq!(extra["command"], json!("ls -la"));
        assert_eq!(extra["options"][0]["decision"], json!("allow_once"));
        assert_eq!(extra["options"][0]["label"], json!("Allow once"));
        assert_eq!(extra["options"][1]["decision"], json!("deny"));
        assert_eq!(extra["options"][1]["label"], json!("Deny"));
    }

    #[test]
    fn build_requested_extra_exec_defaults_command_to_null_when_missing() {
        let extra = build_requested_extra("Read", &json!({"path":"/a"}));
        assert_eq!(extra["title"], json!("Read"));
        assert!(extra["command"].is_null(), "non-Bash tools may omit command");
    }

    #[test]
    fn build_requested_extra_ask_user_maps_questions_label_to_value() {
        let input = json!({
            "questions": [
                { "header": "lang", "question": "Pick a language", "multiSelect": false,
                  "options": [ {"label":"Rust"}, {"label":"Go"} ] },
                { "question": "Free text?", "options": [ {"label":"yes"} ] },
            ]
        });
        let extra = build_requested_extra("AskUserQuestion", &input);
        let qs = &extra["questions"];
        assert_eq!(qs[0]["questionId"], json!("lang"), "header → questionId");
        assert_eq!(qs[0]["question"], json!("Pick a language"));
        assert_eq!(qs[0]["multiSelect"], json!(false));
        assert_eq!(qs[0]["options"][0], json!({"label":"Rust","value":"Rust"}));
        // 缺 header → question_N（1-based）
        assert_eq!(qs[1]["questionId"], json!("question_2"));
        assert_eq!(qs[1]["options"][0], json!({"label":"yes","value":"yes"}));
    }

    #[test]
    fn ask_user_question_with_secret_at_question_is_refused() {
        let input = json!({"questions":[{"secret":true,"question":"pwd"}]});
        assert!(ask_user_has_secret(&input));
    }

    #[test]
    fn ask_user_question_with_is_secret_at_call_level_is_refused() {
        let input = json!({"isSecret":true,"questions":[{"question":"pwd"}]});
        assert!(ask_user_has_secret(&input));
    }

    #[test]
    fn ask_user_question_without_secret_is_not_refused() {
        let input = json!({"questions":[{"question":"name","options":[{"label":"a"}]}]});
        assert!(!ask_user_has_secret(&input));
    }

    #[test]
    fn resolution_to_behavior_maps_exec_decisions_and_ask_user_actions() {
        // exec: 显式 deny → deny；其它 decision → allow
        assert_eq!(resolution_to_behavior(&json!({"decision":"deny"})), "deny");
        assert_eq!(resolution_to_behavior(&json!({"decision":"allow_once"})), "allow");
        // ask_user: answer + 非空 answers → allow；cancel → deny；缺 answers → deny
        assert_eq!(
            resolution_to_behavior(&json!({"action":"answer","answers":[{"q":"a"}]})),
            "allow"
        );
        assert_eq!(resolution_to_behavior(&json!({"action":"answer"})), "deny", "empty answers → deny");
        assert_eq!(resolution_to_behavior(&json!({"action":"cancel"})), "deny");
        assert_eq!(resolution_to_behavior(&json!(null)), "deny", "unknown shape → deny");
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
