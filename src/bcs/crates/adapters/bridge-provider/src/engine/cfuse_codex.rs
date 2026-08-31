//! `CfuseCodex` driver: maps codex `exec --json` JSONL lines (one JSON object per
//! line) to engine-neutral [`StreamEvent`]s and drives one downstream turn over
//! a [`crate::engine::cli::CliSession`].
//!
//! 调用形态（对齐 aix-relay probe 实测：`cfuse --codex` 透传到 `codex exec`）：
//!
//! ```text
//! // 首轮（无 engine_session_id）：
//! cfuse --codex exec --json --skip-git-repo-check -C <cwd> [-m <model>] <prompt>
//! // 续轮（resume 已捕获的 codex thread）：
//! cfuse --codex exec resume <engine_session_id> --json [-m <model>] <prompt>
//! ```
//!
//! `thread.started` 携带 `thread_id`——引擎内会话 id，续轮经 `exec resume <sid>`
//! 恢复。`CliSession` 已用 `current_dir(cwd)` 推进到工作目录；`-C <cwd>` 与首轮
//! probe 形一致。prompt 作为 argv 位置参数传入；spawn 后立即 `close_stdin()`
//! （`codex exec` 会把 piped stdin 当额外输入读，必须立刻 EOF）。
//!
//! 事件映射表（codex JSONL → [`CodexMap`] / [`StreamEvent`]）：
//!
//! | codex 行 | 映射 |
//! | --- | --- |
//! | `{"type":"thread.started","thread_id":…}` | `CodexMap::SessionId(thread_id)` |
//! | `{"type":"turn.started"}` | `CodexMap::Ignore` |
//! | `{"type":"item.completed","item":{"type":"agent_message","text":…}}` | `chat_delta`（并供 run_turn 累计终态文本） |
//! | `{"type":"item.completed","item":{"type":"reasoning","text":…}}` | `agent_thinking`（delta = item.text） |
//! | `{"type":"item.completed","item":{"type":其它}}` | `CodexMap::Ignore` |
//! | `{"type":"turn.completed"[,"text":…]}` | `CodexMap::Final(累计文本)`（text 缺失 → run_turn 用累计 deltas 兜底） |
//! | `{"type":"turn.failed"[,"error":{"message":…}]}` / `{"type":"error",…}` | `CodexMap::Failed(脱敏 message)` |
//! | 其余（含非 JSON 行、未知 type） | `CodexMap::Ignore` |

use std::path::PathBuf;

use bcs_protocol::stream::StreamEvent;
use serde_json::Value;

use crate::engine::cli::CliSession;
use crate::engine::{Engine, EngineKind, TurnError, TurnOutcome, TurnRequest};
use crate::sse;

/// `cfuse --codex` (codex `exec --json` JSONL) 引擎驱动。
pub struct CfuseCodex {
    bin: PathBuf,
}

impl CfuseCodex {
    pub fn new(bin: PathBuf) -> Self {
        Self { bin }
    }
}

/// 单行 codex `exec --json` JSONL 的映射结果。
///
/// `SessionId` 携带 `thread.started` 的 `thread_id`（续轮用于 `exec resume`）；
/// `Events` 是该行产出的 [`StreamEvent`]；`Final` 携带 `turn.completed` 的文本
/// （事件缺文本则空串，`run_turn` 用累计 deltas 兜底）；`Failed` 携带
/// `turn.failed`/`error` 经脱敏的退出原因（调用方上抛为 [`TurnError::EngineExited`]）；
/// `Ignore` 标记“未识别或无产出”。
#[derive(Debug)]
pub(crate) enum CodexMap {
    Events(Vec<StreamEvent>),
    SessionId(String),
    Final(String),
    Failed(String),
    Ignore,
}

/// 把一行 codex `exec --json` JSONL 映射为引擎中立的 [`CodexMap`]。
///
/// 纯函数（无 IO），便于用录制 fixture 做单元测试；按上方映射表逐类分派。
pub(crate) fn map_codex_line(line: &str, run_id: &str) -> CodexMap {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return CodexMap::Ignore,
    };
    let ty = match value.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return CodexMap::Ignore,
    };
    match ty {
        "thread.started" => map_thread_started(&value),
        "turn.started" => CodexMap::Ignore,
        "item.completed" => map_item_completed(&value, run_id),
        "turn.completed" => CodexMap::Final(extract_turn_completed_text(&value)),
        "turn.failed" | "error" => map_failed(&value),
        _ => CodexMap::Ignore,
    }
}

fn map_thread_started(v: &Value) -> CodexMap {
    match v.get("thread_id").and_then(|x| x.as_str()) {
        Some(s) if !s.is_empty() => CodexMap::SessionId(s.to_string()),
        _ => CodexMap::Ignore,
    }
}

fn map_item_completed(v: &Value, run_id: &str) -> CodexMap {
    let item = match v.get("item") {
        Some(i) => i,
        None => return CodexMap::Ignore,
    };
    let itype = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match itype {
        "agent_message" => match item.get("text").and_then(|x| x.as_str()) {
            Some(text) => CodexMap::Events(vec![sse::chat_delta(run_id, text)]),
            None => CodexMap::Ignore,
        },
        "reasoning" => match item.get("text").and_then(|x| x.as_str()) {
            Some(text) => CodexMap::Events(vec![sse::agent_thinking(
                run_id,
                Some(text.to_string()),
                None,
            )]),
            None => CodexMap::Ignore,
        },
        // Task 12 接线工具/approval；本任务仅映射 agent_message/reasoning。
        _ => CodexMap::Ignore,
    }
}

fn extract_turn_completed_text(v: &Value) -> String {
    // `turn.completed` 实测不带 text（probed: {"type":"turn.completed","usage":{...}}）；
    // 防御性抽取 `text`/`output` 字符串，缺失则空串，由 run_turn 用累计 deltas 兜底。
    v.get("text")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("output").and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string()
}

fn map_failed(v: &Value) -> CodexMap {
    // 优先 `error.message`，其次顶层 `message`；均缺失则固定脱敏回退（不回退
    // 原始 JSON 行——可能携带敏感细节，仅截断剥控制字符不足以彻底净化）。
    let msg = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .or_else(|| v.get("message").and_then(|m| m.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| "engine turn failed".to_string());
    CodexMap::Failed(sanitize_message(&msg))
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
        // `cfuse --codex` 透传到 `codex exec`。首轮用 `exec` + cwd/git flags；
        // 续轮用 `exec resume <sid>` 恢复 codex thread（thread_id 由上轮
        // `thread.started` 捕获）。`--json` 两者都加；`-m` 经核实属 `codex exec`
        // 通用 flag。`--permission-mode` codex exec 不接受，故不拼。
        let mut args: Vec<String> = vec!["--codex".into(), "exec".into()];
        if let Some(sid) = &req.engine_session_id {
            args.push("resume".into());
            args.push(sid.clone());
        }
        args.push("--json".into());
        if req.engine_session_id.is_none() {
            args.push("--skip-git-repo-check".into());
            args.push("-C".into());
            args.push(req.cwd.to_string_lossy().into_owned());
        }
        if let Some(model) = &req.model {
            args.push("-m".into());
            args.push(model.clone());
        }
        args.push(req.prompt.clone());

        let mut cli = CliSession::spawn(&self.bin, &args, &req.cwd, &[])
            .await
            .map_err(TurnError::Spawn)?;
        // prompt 已在 argv；codex exec 会把 piped stdin 当额外输入读，
        // 故立即关闭 stdin 让子进程拿到 EOF，只用 argv prompt。
        cli.close_stdin();

        let mut engine_session_id = req.engine_session_id.clone();
        let mut deltas = String::new();
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
                    match map_codex_line(&line, &req.run_id) {
                        CodexMap::SessionId(sid) => engine_session_id = Some(sid),
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
                            // 累计 deltas 形成最终文本；turn.completed 若携带
                            // text 则优先，否则用 deltas 兜底（agent_message 增量）。
                            let final_text = if text.is_empty() { deltas } else { text };
                            return Ok(TurnOutcome {
                                engine_session_id,
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
    use bcs_protocol::stream::{AgentData, StreamEvent};

    #[test]
    fn maps_codex_jsonl_turn() {
        let text = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/codex_turn.jsonl")).unwrap();
        let mut session_id = None;
        let mut deltas = String::new();
        let mut thinking = 0;
        let mut final_text = None;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match map_codex_line(line, "r-1") {
                CodexMap::SessionId(s) => session_id = Some(s),
                CodexMap::Events(evs) => for ev in evs {
                    match ev {
                        StreamEvent::Chat(c) => deltas.push_str(&c.delta_text.unwrap_or_default()),
                        StreamEvent::Agent(a) => match a.data {
                            AgentData::Thinking(t) => {
                                thinking += 1;
                                assert!(t.delta.is_some(), "reasoning thinking must carry delta");
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                },
                CodexMap::Final(t) => final_text = Some(t),
                CodexMap::Failed(_) | CodexMap::Ignore => {}
            }
        }
        assert_eq!(session_id.as_deref(), Some("codex-thread-1"));
        assert_eq!(deltas, "正在排查");
        assert_eq!(thinking, 1);
        // Fixture turn.completed 无 text → Final("")；run_turn 兜底 deltas。
        assert_eq!(final_text.as_deref(), Some(""));
    }

    #[test]
    fn maps_codex_failure_turn_failed() {
        match map_codex_line(r#"{"type":"turn.failed","error":{"message":"boom"}}"#, "r-1") {
            CodexMap::Failed(msg) => assert!(msg.contains("boom")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn maps_codex_failure_top_level_error() {
        match map_codex_line(r#"{"type":"error","error":{"message":"upstream busy"}}"#, "r-1") {
            CodexMap::Failed(msg) => assert!(msg.contains("upstream busy")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn thread_started_captures_thread_id() {
        match map_codex_line(r#"{"type":"thread.started","thread_id":"t-9"}"#, "r-1") {
            CodexMap::SessionId(s) => assert_eq!(s, "t-9"),
            other => panic!("expected SessionId, got {other:?}"),
        }
    }

    #[test]
    fn thread_started_without_thread_id_is_ignore() {
        assert!(matches!(
            map_codex_line(r#"{"type":"thread.started"}"#, "r-1"),
            CodexMap::Ignore
        ));
    }

    #[test]
    fn turn_started_is_ignore() {
        assert!(matches!(map_codex_line(r#"{"type":"turn.started"}"#, "r-1"), CodexMap::Ignore));
    }

    #[test]
    fn item_completed_agent_message_emits_chat_delta() {
        let line = r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"hi"}}"#;
        match map_codex_line(line, "r-9") {
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
    fn item_completed_reasoning_emits_thinking() {
        let line = r#"{"type":"item.completed","item":{"id":"item_2","type":"reasoning","text":"pondering"}}"#;
        match map_codex_line(line, "r-9") {
            CodexMap::Events(evs) => {
                assert_eq!(evs.len(), 1);
                match &evs[0] {
                    StreamEvent::Agent(a) => match &a.data {
                        AgentData::Thinking(t) => assert_eq!(t.delta.as_deref(), Some("pondering")),
                        other => panic!("expected Thinking, got {other:?}"),
                    },
                    other => panic!("expected Agent, got {other:?}"),
                }
            }
            other => panic!("expected Events, got {other:?}"),
        }
    }

    #[test]
    fn item_completed_other_item_type_is_ignore() {
        // 工具/其它 item 类型：Task 12 接线；本任务仅 agent_message/reasoning 映射，余者 Ignore。
        for line in [
            r#"{"type":"item.completed","item":{"id":"i","type":"tool_call","text":"rm -rf /"}}"#,
            r#"{"type":"item.completed","item":{"id":"i","type":"file_edit"}}"#,
            r#"{"type":"item.completed"}"#,
        ] {
            assert!(matches!(map_codex_line(line, "r-1"), CodexMap::Ignore), "line: {line}");
        }
    }

    #[test]
    fn item_completed_without_text_is_ignore() {
        assert!(matches!(
            map_codex_line(
                r#"{"type":"item.completed","item":{"id":"i","type":"agent_message"}}"#,
                "r-1"
            ),
            CodexMap::Ignore
        ));
    }

    #[test]
    fn turn_completed_without_text_is_empty_final() {
        match map_codex_line(r#"{"type":"turn.completed","usage":{"input_tokens":1}}"#, "r-1") {
            CodexMap::Final(text) => assert_eq!(text, ""),
            other => panic!("expected Final, got {other:?}"),
        }
    }

    #[test]
    fn turn_completed_with_text_field_carries_it() {
        // 防御性：若 codex 在 turn.completed 里带 text，则 Final 携带它。
        match map_codex_line(r#"{"type":"turn.completed","text":"done"}"#, "r-1") {
            CodexMap::Final(text) => assert_eq!(text, "done"),
            other => panic!("expected Final, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_ignore() {
        assert!(matches!(map_codex_line(r#"{"type":"response.created"}"#, "r-1"), CodexMap::Ignore));
        assert!(matches!(map_codex_line(r#"{"type":"foo"}"#, "r-1"), CodexMap::Ignore));
    }

    #[test]
    fn json_without_type_is_ignore() {
        assert!(matches!(map_codex_line(r#"{"hello":"world"}"#, "r-1"), CodexMap::Ignore));
    }

    #[test]
    fn malformed_json_is_ignore() {
        assert!(matches!(map_codex_line("not json", "r-1"), CodexMap::Ignore));
        assert!(matches!(map_codex_line("{", "r-1"), CodexMap::Ignore));
    }

    #[test]
    fn turn_failed_without_message_uses_fixed_fallback() {
        // 无 message 字段时不回退原始 JSON 行（避免泄露），给固定脱敏回退。
        match map_codex_line(r#"{"type":"turn.failed"}"#, "r-1") {
            CodexMap::Failed(msg) => assert!(msg.contains("failed"), "msg: {msg}"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn failed_with_control_chars_and_long_message_is_sanitized_and_bounded() {
        // 脱敏：剥控制字符，截断到 256 字符上限（UTF-8 安全）。
        let long = format!("{}{}", "boom", "\n".repeat(10));
        let data = format!(
            r#"{{"type":"turn.failed","error":{{"message":{}}}}}"#,
            serde_json::to_string(&long).unwrap()
        );
        match map_codex_line(&data, "r-1") {
            CodexMap::Failed(msg) => {
                assert!(msg.contains("boom"));
                assert!(!msg.contains('\n'), "control chars must be stripped: {msg:?}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        let huge = "x".repeat(1024);
        let data = format!(
            r#"{{"type":"turn.failed","error":{{"message":{}}}}}"#,
            serde_json::to_string(&huge).unwrap()
        );
        match map_codex_line(&data, "r-1") {
            CodexMap::Failed(msg) => assert_eq!(msg.chars().count(), 256),
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
