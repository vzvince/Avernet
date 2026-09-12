# Bridge Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 bcs workspace 新增独立服务 `bridge-provider`：实现 BCN Provider 2.0 webhook（SSE 下行），把 `chat.send` 等请求桥接到 cfuse 的 cc/codex 引擎执行。

**Architecture:** axum webhook → RunRegistry/SessionStore/InteractionRegistry（进程内存）→ `Engine` trait（`CfuseCc`/`CfuseCodex`，共享 `CliSession` 子进程管道）→ SSE 帧经统一 encoder 输出。引擎事件映射为 `bcs_protocol::stream::StreamEvent`，由 encoder 赋 `seq` 并编码为 Provider 2.0 SSE 帧。

**Tech Stack:** Rust（workspace edition）、axum 0.8、tokio、serde/serde_json、bcs-protocol（契约测试用 parser）、tokio-util（CancellationToken）、reqwest（仅测试）。

**Spec:** `src/bcs/docs/superpowers/specs/2026-08-31-bridge-provider-design.md`

## Global Constraints

- 禁止 `cargo fmt`（项目 CLAUDE.md）；只改动本任务需要的行。
- UTF-8 安全：禁止字节索引切片字符串，一律 `char_indices()`（项目 CLAUDE.md）。
- 生产代码不新增 `unwrap/expect/panic`（测试代码除外）；错误用 `thiserror`。
- 依赖只允许 workspace 已有共享依赖 + 必要时在 `[workspace.dependencies]` 新增 `tokio-stream`。
- 构建/测试只用 `cargo test -p bridge-provider`（本 worktree 磁盘受限，禁止全 workspace 构建）。
- 测试零真实 LLM 调用：引擎一律用 mock 可执行文件替代。
- `interaction.resolve` 的错误 ACK 形态是 `{"ok":false,"retryable":bool,"error":"<string>"}`（error 为字符串），**不要**用通用错误对象形态。
- SSE 协议约束（spec §2）：`seq` 同流单调递增、interaction 必须有 seq；terminal 后不再发帧；`agent/stream:approval` 禁止发送；单帧 ≤ 8 MiB。
- BCS 无重连/续传：SSE 写失败（无订阅者）即终止 run。

## File Structure

```
crates/adapters/bridge-provider/
├── Cargo.toml
├── src/
│   ├── lib.rs            # 模块声明 + 公共 re-export
│   ├── main.rs           # binary：加载配置、起 server、优雅退出
│   ├── config.rs         # ProviderConfig / BotConfig / EngineKind
│   ├── error.rs          # BridgeError → HTTP status + 线错误体
│   ├── sse.rs            # encode_frame / StreamEvent→帧映射 / 构造器
│   ├── idempotency.rs    # 幂等台账
│   ├── session.rs        # SessionStore（双 id 映射 + pending injects + active_run）
│   ├── interaction.rs    # InteractionRegistry（pending/resolve/兜底/失效）
│   ├── run.rs            # RunRegistry + run loop（select: 引擎事件/心跳/deadline/abort）
│   ├── webhook.rs        # axum router + 5 个 method handler + 校验链
│   └── engine/
│       ├── mod.rs        # Engine trait / TurnRequest / TurnOutcome / TurnError / build_engine
│       ├── cli.rs        # CliSession（spawn/stdin/stdout/kill，kill_on_drop）
│       ├── cfuse_cc.rs   # cfuse cc 模式：stream-json ↔ StreamEvent + 控制通道
│       └── cfuse_codex.rs# cfuse codex 模式：codex SSE ↔ StreamEvent
└── tests/
    ├── fixtures/         # mock cfuse 脚本（bash）
    ├── golden_frames.rs  # SSE 线格式 golden + bcs-protocol parser 往返
    └── e2e_webhook.rs    # 全链路：webhook ↔ mock 引擎 ↔ SSE 消费
```

---

### Task 1: Crate scaffold + 配置加载

**Files:**
- Create: `crates/adapters/bridge-provider/Cargo.toml`
- Create: `crates/adapters/bridge-provider/src/lib.rs`
- Create: `crates/adapters/bridge-provider/src/config.rs`
- Modify: `Cargo.toml`（workspace 根，members 列表 + 可能新增 tokio-stream）

**Interfaces:**
- Produces:
  - `pub enum EngineKind { CfuseCc, CfuseCodex }`（serde: `"cfuse-cc" | "cfuse-codex"`）
  - `pub struct BotConfig { provider_bot_ref: String, engine: EngineKind, model: Option<String>, cwd: PathBuf, permission_mode: Option<String>, cfuse_bin: Option<PathBuf> }`
  - `pub struct ProviderConfig { provider_id: String, listen: SocketAddr, bcs_to_provider_token: String, bot_runtime_token: Option<String>, bots: Vec<BotConfig> }`
  - `impl ProviderConfig { pub fn load(path: &Path) -> Result<Self, ConfigError>; pub fn bot(&self, provider_bot_ref: &str) -> Option<&BotConfig>; }`

- [ ] **Step 1: 注册 workspace member + 建 Cargo.toml**

根 `Cargo.toml` members 的 adapters 段加一行 `"crates/adapters/bridge-provider",`。

```toml
[package]
name        = "bridge-provider"
description = "BCN Provider 2.0 bridge to local coding engines (cfuse cc/codex)"
version.workspace      = true
edition.workspace      = true
license.workspace      = true
repository.workspace   = true
rust-version.workspace = true

[lints]
workspace = true

[dependencies]
async-trait   = { workspace = true }
axum          = { workspace = true }
bcs-protocol  = { workspace = true }
futures       = { workspace = true }
serde         = { workspace = true }
serde_json    = { workspace = true }
thiserror     = { workspace = true }
tokio         = { workspace = true }
tokio-stream  = { workspace = true }
tokio-util    = { workspace = true }
toml          = { workspace = true }
tracing       = { workspace = true }
uuid          = { workspace = true }

[dev-dependencies]
reqwest = { workspace = true }
tempfile = "3"
```

若根 `[workspace.dependencies]` 无 `tokio-stream`，加 `tokio-stream = "0.1"`。

- [ ] **Step 2: 写失败测试 `config.rs` 的 `#[cfg(test)]`**

```rust
#[test]
fn loads_provider_config_and_finds_bot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.toml");
    std::fs::write(&path, r#"
provider_id = "bridge-1"
listen = "127.0.0.1:21100"
bcs_to_provider_token = "tok-b2p"

[[bot]]
provider_bot_ref = "cc-worker"
engine = "cfuse-cc"
model = "sonnet"
cwd = "/tmp"
"#).unwrap();
    let cfg = ProviderConfig::load(&path).unwrap();
    assert_eq!(cfg.provider_id, "bridge-1");
    let bot = cfg.bot("cc-worker").unwrap();
    assert_eq!(bot.engine, EngineKind::CfuseCc);
    assert_eq!(bot.model.as_deref(), Some("sonnet"));
    assert!(cfg.bot("nope").is_none());
}

#[test]
fn rejects_unknown_engine_kind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.toml");
    std::fs::write(&path, r#"
provider_id = "bridge-1"
listen = "127.0.0.1:21100"
bcs_to_provider_token = "t"
[[bot]]
provider_bot_ref = "x"
engine = "bogus"
cwd = "/tmp"
"#).unwrap();
    assert!(ProviderConfig::load(&path).is_err());
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test -p bridge-provider config`
Expected: 编译失败（`ProviderConfig` 不存在）

- [ ] **Step 4: 实现 `config.rs`**

```rust
use std::{net::SocketAddr, path::{Path, PathBuf}};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineKind { CfuseCc, CfuseCodex }

#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    pub provider_bot_ref: String,
    pub engine: EngineKind,
    pub model: Option<String>,
    pub cwd: PathBuf,
    pub permission_mode: Option<String>,
    pub cfuse_bin: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub listen: SocketAddr,
    pub bcs_to_provider_token: String,
    pub bot_runtime_token: Option<String>,
    #[serde(rename = "bot")]
    pub bots: Vec<BotConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("read config: {0}")]
    Read(#[from] std::io::Error),
    #[error("parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

impl ProviderConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
    pub fn bot(&self, provider_bot_ref: &str) -> Option<&BotConfig> {
        self.bots.iter().find(|b| b.provider_bot_ref == provider_bot_ref)
    }
}
```

`lib.rs` 先只放 `pub mod config;`。

- [ ] **Step 5: 运行确认通过并提交**

Run: `cargo test -p bridge-provider config`
Expected: PASS

```bash
git add Cargo.toml crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): crate scaffold + provider config"
```

---

### Task 2: SSE 帧编码器（纯函数）

**Files:**
- Create: `crates/adapters/bridge-provider/src/sse.rs`
- Test: `crates/adapters/bridge-provider/tests/golden_frames.rs`

**Interfaces:**
- Consumes: `serde_json::Value`
- Produces:
  - `pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;`
  - `pub const HEARTBEAT: &str = ": heartbeat\n\n";`
  - `pub fn encode_frame(event: &str, id: Option<u64>, data: &serde_json::Value) -> Result<String, FrameError>`
  - `pub enum FrameError { FrameTooLarge(usize), Json(serde_json::Error) }`

- [ ] **Step 1: 写失败测试（对齐 spec §10 线上样本形态）**

```rust
use bridge_provider::sse::{encode_frame, HEARTBEAT, MAX_FRAME_BYTES};
use serde_json::json;

#[test]
fn encodes_chat_delta_golden() {
    let frame = encode_frame("chat", Some(605), &json!({
        "state":"delta","deltaText":"查询。","runId":"r-1","seq":605,"ts":1786276303908u64
    })).unwrap();
    let expected = "event: chat\nid: 605\ndata: {\"state\":\"delta\",\"deltaText\":\"查询。\",\"runId\":\"r-1\",\"seq\":605,\"ts\":1786276303908}\n\n";
    assert_eq!(frame, expected);
}

#[test]
fn encodes_frame_without_id() {
    let frame = encode_frame("ping", None, &json!({"ts":1})).unwrap();
    assert_eq!(frame, "event: ping\ndata: {\"ts\":1}\n\n");
}

#[test]
fn rejects_frame_over_8mib() {
    let big = "x".repeat(MAX_FRAME_BYTES);
    let err = encode_frame("chat", None, &json!({"deltaText": big})).unwrap_err();
    assert!(matches!(err, bridge_provider::sse::FrameError::FrameTooLarge(_)));
}

#[test]
fn heartbeat_is_sse_comment() {
    assert_eq!(HEARTBEAT, ": heartbeat\n\n");
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider --test golden_frames`
Expected: 编译失败（`sse` 模块不存在）

- [ ] **Step 3: 实现 `sse.rs` 编码部分**

```rust
use serde_json::Value;

pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const HEARTBEAT: &str = ": heartbeat\n\n";

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("SSE frame too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("serialize SSE data: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn encode_frame(event: &str, id: Option<u64>, data: &Value) -> Result<String, FrameError> {
    // 单行紧凑 JSON；data 内不出现裸换行（serde_json 会转义）
    let data_json = serde_json::to_string(data)?;
    let mut frame = String::with_capacity(event.len() + data_json.len() + 24);
    frame.push_str("event: ");
    frame.push_str(event);
    frame.push('\n');
    if let Some(id) = id {
        frame.push_str("id: ");
        frame.push_str(&id.to_string());
        frame.push('\n');
    }
    frame.push_str("data: ");
    frame.push_str(&data_json);
    frame.push_str("\n\n");
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::FrameTooLarge(frame.len()));
    }
    Ok(frame)
}
```

注意：`frame.len()` 是字节数（`String::len` 即字节长度），这正是 8 MiB 约束的度量单位；不存在逐字节切片问题。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider --test golden_frames`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): SSE frame encoder with 8MiB guard"
```

---

### Task 3: StreamEvent → 线帧映射（往返契约测试）

**Files:**
- Modify: `crates/adapters/bridge-provider/src/sse.rs`
- Test: `crates/adapters/bridge-provider/tests/golden_frames.rs`

**Interfaces:**
- Consumes: `bcs_protocol::stream::{StreamEvent, ChatEvent, ChatState, AgentEvent, AgentData, ToolData, ToolPhase, ThinkingData, LifecycleData, InteractionEvent, InteractionKind, InteractionPhase}`（字段均为 pub；emit 侧忽略其 `raw` 字段）
- Produces:
  - `pub fn event_to_frame(ev: &StreamEvent, seq: u64, ts: u64, run_id: &str) -> Result<String, FrameError>` — 把事件转为线帧；**调用方保证 seq 单调**；`Ping` 与 `Unknown` 由调用方过滤，此函数对二者返回 `FrameError::Unsupported`
  - 构造器（驱动侧使用，seq 恒为 `None`，由 run loop 赋）：
    - `pub fn chat_delta(run_id: &str, text: &str) -> StreamEvent`
    - `pub fn chat_final(run_id: &str, text: String) -> StreamEvent`
    - `pub fn chat_error(run_id: &str, message: &str, kind: Option<&str>) -> StreamEvent`
    - `pub fn chat_aborted(run_id: &str, stop_reason: &str) -> StreamEvent`
    - `pub fn agent_tool(run_id: &str, data: ToolData) -> StreamEvent`
    - `pub fn agent_thinking(run_id: &str, delta: Option<String>, text: Option<String>) -> StreamEvent`
    - `pub fn agent_lifecycle(run_id: &str, phase: &str, model: Option<String>) -> StreamEvent`
    - `pub fn interaction_event(run_id: &str, phase: InteractionPhase, kind: InteractionKind, interaction_id: &str, extra: Value) -> StreamEvent`

设计要点（spec §5.2）：线格式 camelCase 键（`runId/deltaText/toolCallId`）；`final` 的 `message` 是 full snapshot；`ChatEvent/AgentEvent` 不 derive Serialize，故 encoder 手工构造 `Value`（这就是"复用协议类型做语义、encoder 独占线格式"的边界）。

- [ ] **Step 1: 写失败测试——golden 帧 + BCS parser 往返**

```rust
use bcs_protocol::stream::{parse_stream_event, ChatState, StreamEvent, ToolPhase};
use bridge_provider::sse::*;
use serde_json::json;

/// 从编码帧中抽出 event 名与 data JSON（测试辅助，复制到测试文件顶部）
fn split_frame(frame: &str) -> (String, serde_json::Value) {
    let mut event = String::new();
    let mut data = String::new();
    for line in frame.lines() {
        if let Some(v) = line.strip_prefix("event: ") { event = v.to_string(); }
        if let Some(v) = line.strip_prefix("data: ") { data = v.to_string(); }
    }
    (event, serde_json::from_str(&data).unwrap())
}

#[test]
fn chat_delta_roundtrips_through_bcs_parser() {
    let frame = event_to_frame(&chat_delta("r-1", "正在分析"), 1, 100, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    assert_eq!(event, "chat");
    match parse_stream_event(&event, data) {
        StreamEvent::Chat(c) => {
            assert_eq!(c.state, ChatState::Delta);
            assert_eq!(c.delta_text.as_deref(), Some("正在分析"));
            assert_eq!(c.seq, Some(1));
        }
        other => panic!("expected chat, got {other:?}"),
    }
}

#[test]
fn chat_final_is_full_snapshot_terminal() {
    let frame = event_to_frame(&chat_final("r-1", "最终答案".to_string()), 5, 200, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    match parse_stream_event(&event, data.clone()) {
        StreamEvent::Chat(c) => {
            assert_eq!(c.state, ChatState::Final);
            assert_eq!(data["message"]["content"][0]["text"], json!("最终答案"));
        }
        other => panic!("expected final, got {other:?}"),
    }
}

#[test]
fn tool_result_roundtrips() {
    let ev = agent_tool("r-1", bcs_protocol::stream::ToolData {
        phase: ToolPhase::Result,
        name: Some("exec".into()),
        tool_call_id: Some("tc-1".into()),
        is_error: Some(false),
        exit_code: Some(0),
        duration_ms: Some(120),
        cwd: None,
        args: None,
        result: Some(json!({"content":[{"type":"text","text":"ok"}]})),
        partial_result: None,
    });
    let frame = event_to_frame(&ev, 4, 100, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    match parse_stream_event(&event, data) {
        StreamEvent::Agent(a) => match a.data {
            bcs_protocol::stream::AgentData::Tool(t) => {
                assert_eq!(t.phase, ToolPhase::Result);
                assert_eq!(t.tool_call_id.as_deref(), Some("tc-1"));
            }
            other => panic!("expected tool, got {other:?}"),
        },
        other => panic!("expected agent, got {other:?}"),
    }
}

#[test]
fn interaction_requested_exec_roundtrips() {
    let ev = interaction_event(
        "r-1",
        bcs_protocol::stream::InteractionPhase::Requested,
        bcs_protocol::stream::InteractionKind::Exec,
        "int-1",
        json!({"title":"Run command?","command":"npm run deploy",
               "options":[{"decision":"allow_once","label":"Allow once"},
                          {"decision":"deny","label":"Deny"}]}),
    );
    let frame = event_to_frame(&ev, 7, 100, "r-1").unwrap();
    let (event, data) = split_frame(&frame);
    assert_eq!(event, "interaction");
    match parse_stream_event(&event, data.clone()) {
        StreamEvent::Interaction(i) => {
            assert_eq!(i.interaction_id, "int-1");
            assert_eq!(i.kind, bcs_protocol::stream::InteractionKind::Exec);
            assert_eq!(data["options"][0]["decision"], json!("allow_once"));
        }
        other => panic!("expected interaction, got {other:?}"),
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider --test golden_frames`
Expected: 编译失败（`event_to_frame` 等不存在）

- [ ] **Step 3: 实现映射（`sse.rs` 追加）**

```rust
use bcs_protocol::stream::{
    AgentData, AgentEvent, ChatEvent, ChatState, InteractionEvent, InteractionKind,
    InteractionPhase, LifecycleData, StreamEvent, ThinkingData, ToolData,
};
use serde_json::{json, Value};

pub fn chat_delta(run_id: &str, text: &str) -> StreamEvent {
    StreamEvent::Chat(ChatEvent {
        run_id: run_id.into(), seq: None, state: ChatState::Delta, session_key: None,
        delta_text: Some(text.into()), stop_reason: None, error_message: None,
        error_kind: None, error_code: None, message: None, raw: Value::Null,
    })
}

pub fn chat_final(run_id: &str, text: String) -> StreamEvent {
    let message = json!({"role":"assistant","content":[{"type":"text","text":text}]});
    StreamEvent::Chat(ChatEvent {
        run_id: run_id.into(), seq: None, state: ChatState::Final, session_key: None,
        delta_text: None, stop_reason: Some("completed".into()), error_message: None,
        error_kind: None, error_code: None, message: Some(message), raw: Value::Null,
    })
}

pub fn chat_error(run_id: &str, message: &str, kind: Option<&str>) -> StreamEvent {
    StreamEvent::Chat(ChatEvent {
        run_id: run_id.into(), seq: None, state: ChatState::Error, session_key: None,
        delta_text: None, stop_reason: None, error_message: Some(message.into()),
        error_kind: kind.map(str::to_string), error_code: None, message: None, raw: Value::Null,
    })
}

pub fn chat_aborted(run_id: &str, stop_reason: &str) -> StreamEvent {
    StreamEvent::Chat(ChatEvent {
        run_id: run_id.into(), seq: None, state: ChatState::Aborted, session_key: None,
        delta_text: None, stop_reason: Some(stop_reason.into()), error_message: None,
        error_kind: None, error_code: None, message: None, raw: Value::Null,
    })
}

pub fn agent_tool(run_id: &str, data: ToolData) -> StreamEvent {
    StreamEvent::Agent(AgentEvent {
        run_id: run_id.into(), seq: None, ts: None, session_key: None,
        data: AgentData::Tool(data), raw: Value::Null,
    })
}

pub fn agent_thinking(run_id: &str, delta: Option<String>, text: Option<String>) -> StreamEvent {
    StreamEvent::Agent(AgentEvent {
        run_id: run_id.into(), seq: None, ts: None, session_key: None,
        data: AgentData::Thinking(ThinkingData { delta, text }), raw: Value::Null,
    })
}

pub fn agent_lifecycle(run_id: &str, phase: &str, model: Option<String>) -> StreamEvent {
    StreamEvent::Agent(AgentEvent {
        run_id: run_id.into(), seq: None, ts: None, session_key: None,
        data: AgentData::Lifecycle(LifecycleData { phase: phase.into(), model, agent_mode: None }),
        raw: Value::Null,
    })
}

pub fn interaction_event(run_id: &str, phase: InteractionPhase, kind: InteractionKind,
                         interaction_id: &str, extra: Value) -> StreamEvent {
    StreamEvent::Interaction(InteractionEvent {
        run_id: run_id.into(), seq: None, ts: None, session_key: None,
        phase, interaction_id: interaction_id.into(), kind, raw: extra,
    })
}

pub fn event_to_frame(ev: &StreamEvent, seq: u64, ts: u64, run_id: &str)
    -> Result<String, FrameError>
{
    let (event, data) = match ev {
        StreamEvent::Chat(c) => {
            let mut d = json!({"runId": run_id, "seq": seq, "ts": ts});
            let obj = d.as_object_mut().expect("freshly built object");
            match c.state {
                ChatState::Delta => {
                    obj.insert("state".into(), json!("delta"));
                    if let Some(t) = &c.delta_text { obj.insert("deltaText".into(), json!(t)); }
                }
                ChatState::Final => {
                    obj.insert("state".into(), json!("final"));
                    if let Some(m) = &c.message { obj.insert("message".into(), m.clone()); }
                    if let Some(s) = &c.stop_reason { obj.insert("stopReason".into(), json!(s)); }
                }
                ChatState::Error => {
                    obj.insert("state".into(), json!("error"));
                    if let Some(m) = &c.error_message { obj.insert("errorMessage".into(), json!(m)); }
                    if let Some(k) = &c.error_kind { obj.insert("errorKind".into(), json!(k)); }
                }
                ChatState::Aborted => {
                    obj.insert("state".into(), json!("aborted"));
                    if let Some(s) = &c.stop_reason { obj.insert("stopReason".into(), json!(s)); }
                }
            }
            ("chat", d)
        }
        StreamEvent::Agent(a) => {
            let mut d = json!({"runId": run_id, "seq": seq, "ts": ts});
            let obj = d.as_object_mut().expect("freshly built object");
            match &a.data {
                AgentData::Tool(t) => {
                    obj.insert("stream".into(), json!("tool"));
                    let v = serde_json::to_value(t)?;
                    merge(obj, v);
                }
                AgentData::Thinking(t) => {
                    obj.insert("stream".into(), json!("thinking"));
                    let v = serde_json::to_value(t)?;
                    merge(obj, v);
                }
                AgentData::Lifecycle(l) => {
                    obj.insert("stream".into(), json!("lifecycle"));
                    let v = serde_json::to_value(l)?;
                    merge(obj, v);
                }
                // Approval 属旧兼容结构，禁止输出（spec §2）；Phase 暂不发
                AgentData::Approval(_) | AgentData::Phase(_) | AgentData::Unknown { .. } => {
                    return Err(FrameError::Unsupported);
                }
            }
            ("agent", d)
        }
        StreamEvent::Interaction(i) => {
            let mut d = json!({
                "runId": run_id, "seq": seq, "ts": ts,
                "phase": match i.phase {
                    InteractionPhase::Requested => "requested",
                    InteractionPhase::Resolved => "resolved",
                },
                "interactionId": i.interaction_id,
                "kind": match i.kind {
                    InteractionKind::Exec => "exec",
                    InteractionKind::AskUser => "ask_user",
                    InteractionKind::ModeSwitch => "mode_switch",
                },
            });
            let obj = d.as_object_mut().expect("freshly built object");
            merge(obj, i.raw.clone()); // raw 承载 kind 专有三白名单字段（options/questions/…）
            ("interaction", d)
        }
        StreamEvent::Ping { .. } | StreamEvent::Unknown { .. } => {
            return Err(FrameError::Unsupported);
        }
    };
    encode_frame(event, Some(seq), &data)
}

fn merge(obj: &mut serde_json::Map<String, Value>, v: Value) {
    if let Value::Object(m) = v {
        for (k, val) in m { obj.insert(k, val); }
    }
}
```

`FrameError` 增加变体：`#[error("event kind is not emittable on the wire")] Unsupported`。

注意：`AgentEvent`/`ChatEvent` 的 `raw` 字段在 emit 侧无意义，填 `Value::Null`；`ts`/`session_key` 由 encoder 与 run loop 统一填。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider --test golden_frames`
Expected: PASS（4 个测试）

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): StreamEvent to Provider 2.0 wire mapping"
```

---

### Task 4: 错误类型 + webhook 骨架（校验链 + bot.ping）

**Files:**
- Create: `crates/adapters/bridge-provider/src/error.rs`
- Create: `crates/adapters/bridge-provider/src/webhook.rs`
- Modify: `crates/adapters/bridge-provider/src/lib.rs`
- Test: `crates/adapters/bridge-provider/tests/e2e_webhook.rs`（本任务先建骨架）

**Interfaces:**
- Produces:
  - `pub struct BridgeError { status: StatusCode, code: &'static str, message: String, retryable: bool }`，构造器：`invalid_request(msg)/unauthorized()/provider_id_mismatch()/bot_not_found(ref)/conflict()/rate_limited()/unsupported_method(method)/unavailable(msg)/timeout()`；`pub fn into_response(self) -> axum::response::Response`
  - `pub struct AppState { config: ProviderConfig, … }`（后续任务逐步加字段）
  - `pub fn router(state: Arc<AppState>) -> axum::Router`
  - 请求 DTO：
    ```rust
    pub struct DownstreamRequest {
        pub id: String,
        pub method: String,
        pub to_bot: ToBot,
        pub session_id: Option<String>,
        pub message: Option<Value>,
        pub timeout_ms: Option<u64>,
        pub params: Option<Value>,
    }
    pub struct ToBot { pub provider_id: String, pub provider_bot_ref: String }
    ```

校验链顺序（spec §5.1，任一不过即返回对应错误，不进入业务）：
1. `Authorization: Bearer <bcs_to_provider_token>` → 401
2. body `to_bot.provider_id == config.provider_id` → 403
3. method ∈ 已知集合 → 501
4. method 级参数校验 → 400

- [ ] **Step 1: 写失败测试**

```rust
// tests/e2e_webhook.rs
use axum::http::{header, StatusCode};
use serde_json::json;

mod support; // tests/support/mod.rs：spawn_app(config_toml: &str) -> String(base_url)

#[tokio::test]
async fn ping_requires_auth_and_matching_provider() {
    let url = support::spawn_app(r#"
provider_id = "bridge-1"
listen = "127.0.0.1:0"
bcs_to_provider_token = "tok-b2p"
[[bot]]
provider_bot_ref = "cc-worker"
engine = "cfuse-cc"
cwd = "/tmp"
"#).await;
    let client = reqwest::Client::new();

    // 无 token → 401
    let resp = client.post(format!("{url}/webhook"))
        .json(&json!({"type":"req","id":"1","method":"bot.ping",
                      "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], json!("unauthorized"));

    // provider_id 不匹配 → 403
    let resp = client.post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"2","method":"bot.ping",
                      "to_bot":{"provider_id":"other","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 未知 method → 501
    let resp = client.post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"3","method":"chat.explode",
                      "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);

    // ping 正常 → 200
    let resp = client.post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .json(&json!({"type":"req","id":"4","method":"bot.ping",
                      "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["ok"], json!(true));
}
```

`tests/support/mod.rs`：

```rust
use std::{net::SocketAddr, sync::Arc};
use bridge_provider::{config::ProviderConfig, webhook};

pub async fn spawn_app(toml_text: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.toml");
    std::fs::write(&path, toml_text).unwrap();
    let config = ProviderConfig::load(&path).unwrap();
    // tempdir 不能 drop：泄漏到测试生命周期结束即可（测试进程退出清理）
    std::mem::forget(dir);
    let mut cfg = config;
    cfg.listen = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let app = webhook::router(Arc::new(bridge_provider::AppState::new(cfg)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider --test e2e_webhook`
Expected: 编译失败

- [ ] **Step 3: 实现 `error.rs` + `webhook.rs` 骨架**

`error.rs`：

```rust
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
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({
            "ok": false,
            "error": { "code": self.code, "message": self.message, "retryable": self.retryable }
        }))).into_response()
    }
}
```

`webhook.rs` 骨架：

```rust
use std::sync::Arc;
use axum::{extract::State, http::{HeaderMap, StatusCode}, response::Response, routing::post, Json, Router};
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

pub struct AppState { pub config: ProviderConfig }
impl AppState { pub fn new(config: ProviderConfig) -> Self { Self { config } } }

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

use axum::response::IntoResponse;
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider --test e2e_webhook`
Expected: PASS（chat.* 方法本任务不测试）

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): webhook skeleton with auth chain + bot.ping"
```

---

### Task 5: 幂等台账

**Files:**
- Create: `crates/adapters/bridge-provider/src/idempotency.rs`
- Modify: `crates/adapters/bridge-provider/src/webhook.rs`（挂入 AppState，chat.inject/chat.abort 使用）

**Interfaces:**
- Produces:
  - `pub enum IdemDecision { Proceed, Replay(serde_json::Value), Conflict }`
  - `pub struct IdempotencyLedger { … }`
  - `impl IdempotencyLedger { pub fn new() -> Self; pub fn begin(&self, id: &str, fingerprint: &str) -> IdemDecision; pub fn complete(&self, id: &str, response: serde_json::Value); }`
  - `pub fn fingerprint(body: &serde_json::Value) -> String`（对 `DownstreamRequest` 的关键字段做稳定序列化：method/to_bot/session_id/message/params；排除易变字段）

语义（spec §5.5）：同 id 异 fingerprint → `Conflict`（409）；同 id 同 fingerprint 且已完成 → `Replay`（直接返回上次响应）；同 id 同 fingerprint 进行中 → `Replay({"ok":true})` 幂等应答（inject/abort 的场景不要求重入执行）。`chat.send` 不走本台账（走 RunRegistry，Task 10）。

- [ ] **Step 1: 写失败测试（`idempotency.rs` 内 `#[cfg(test)]`）**

```rust
#[test]
fn dedupes_same_id_same_body_and_conflicts_different_body() {
    let ledger = IdempotencyLedger::new();
    assert!(matches!(ledger.begin("id-1", "fp-a"), IdemDecision::Proceed));
    ledger.complete("id-1", serde_json::json!({"ok": true}));
    match ledger.begin("id-1", "fp-a") {
        IdemDecision::Replay(v) => assert_eq!(v["ok"], serde_json::json!(true)),
        _ => panic!("expected replay"),
    }
    assert!(matches!(ledger.begin("id-1", "fp-b"), IdemDecision::Conflict));
}

#[test]
fn in_progress_same_body_replays_ok_ack() {
    let ledger = IdempotencyLedger::new();
    assert!(matches!(ledger.begin("id-2", "fp-a"), IdemDecision::Proceed));
    match ledger.begin("id-2", "fp-a") {
        IdemDecision::Replay(v) => assert_eq!(v["ok"], serde_json::json!(true)),
        _ => panic!("expected replay"),
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider idempotency`
Expected: 编译失败

- [ ] **Step 3: 实现**

```rust
use std::{collections::HashMap, sync::Mutex};

pub enum IdemDecision { Proceed, Replay(serde_json::Value), Conflict }

enum Entry { InProgress { fingerprint: String }, Completed { fingerprint: String, response: serde_json::Value } }

#[derive(Default)]
pub struct IdempotencyLedger { map: Mutex<HashMap<String, Entry>> }

impl IdempotencyLedger {
    pub fn new() -> Self { Self::default() }

    pub fn begin(&self, id: &str, fingerprint: &str) -> IdemDecision {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        match map.get(id) {
            Some(Entry::InProgress { fingerprint: f }) if f == fingerprint =>
                IdemDecision::Replay(serde_json::json!({"ok": true})),
            Some(Entry::Completed { fingerprint: f, response }) if f == fingerprint =>
                IdemDecision::Replay(response.clone()),
            Some(_) => IdemDecision::Conflict,
            None => {
                map.insert(id.to_string(), Entry::InProgress { fingerprint: fingerprint.to_string() });
                IdemDecision::Proceed
            }
        }
    }

    pub fn complete(&self, id: &str, response: serde_json::Value) {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(Entry::InProgress { fingerprint }) = map.get(id) {
            let fingerprint = fingerprint.clone();
            map.insert(id.to_string(), Entry::Completed { fingerprint, response });
        }
    }
}

pub fn fingerprint(parts: &[&str]) -> String {
    // 稳定拼接；调用方传入已选定的关键字段，避免引入哈希依赖
    parts.join("\u{1f}")
}
```

- [ ] **Step 4: 运行确认通过并提交**

Run: `cargo test -p bridge-provider idempotency`

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): idempotency ledger"
```

---

### Task 6: SessionStore（双 id 映射 + pending injects + active_run）

**Files:**
- Create: `crates/adapters/bridge-provider/src/session.rs`
- Modify: `crates/adapters/bridge-provider/src/webhook.rs`（AppState 加 `sessions: SessionStore`）

**Interfaces:**
- Produces:
  - `pub struct InjectedMessage { pub run_id: String, pub from_name: Option<String>, pub text: String }`
  - `pub struct SessionMapping { pub engine_session_id: Option<String>, pub pending_injects: Vec<InjectedMessage>, pub active_run: Option<String> }`
  - `pub struct SessionStore { … }`，方法：
    - `pub async fn mapping(&self, bot: &str, bcs_session: &str) -> SessionMapping`（克隆快照；不存在返回默认）
    - `pub async fn set_engine_session_id(&self, bot: &str, bcs_session: &str, engine_session_id: &str)`
    - `pub async fn add_inject(&self, bot: &str, bcs_session: &str, msg: InjectedMessage)`
    - `pub async fn take_pending_injects(&self, bot: &str, bcs_session: &str) -> Vec<InjectedMessage>`
    - `pub async fn try_start_run(&self, bot: &str, bcs_session: &str, run_id: &str) -> Result<(), SessionBusy>`
    - `pub async fn finish_run(&self, bot: &str, bcs_session: &str, run_id: &str)`
    - `pub async fn active_run(&self, bot: &str, bcs_session: &str) -> Option<String>`

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn dual_id_mapping_and_run_exclusion() {
    let store = SessionStore::new();
    let m = store.mapping("bot-a", "s-1").await;
    assert!(m.engine_session_id.is_none());

    store.set_engine_session_id("bot-a", "s-1", "engine-sess-9").await;
    assert_eq!(store.mapping("bot-a", "s-1").await.engine_session_id.as_deref(),
               Some("engine-sess-9"));
    // 另一个 bcs session 不受影响
    assert!(store.mapping("bot-a", "s-2").await.engine_session_id.is_none());

    store.try_start_run("bot-a", "s-1", "run-1").await.unwrap();
    assert!(store.try_start_run("bot-a", "s-1", "run-2").await.is_err());
    store.finish_run("bot-a", "s-1", "run-1").await;
    store.try_start_run("bot-a", "s-1", "run-2").await.unwrap();
}

#[tokio::test]
async fn pending_injects_fifo_drain() {
    let store = SessionStore::new();
    store.add_inject("b", "s", InjectedMessage{ run_id: "i1".into(), from_name: None, text: "m1".into() }).await;
    store.add_inject("b", "s", InjectedMessage{ run_id: "i2".into(), from_name: Some("张三".into()), text: "m2".into() }).await;
    let drained = store.take_pending_injects("b", "s").await;
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].text, "m1");
    assert!(store.take_pending_injects("b", "s").await.is_empty());
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider session`
Expected: 编译失败

- [ ] **Step 3: 实现 `session.rs`**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct InjectedMessage { pub run_id: String, pub from_name: Option<String>, pub text: String }

#[derive(Debug, Clone, Default)]
pub struct SessionMapping {
    pub engine_session_id: Option<String>,
    pub pending_injects: Vec<InjectedMessage>,
    pub active_run: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("session already has an active run")]
pub struct SessionBusy;

type Key = (String, String); // (provider_bot_ref, bcs_session_id)

#[derive(Clone, Default)]
pub struct SessionStore { map: Arc<RwLock<HashMap<Key, SessionMapping>>> }

impl SessionStore {
    pub fn new() -> Self { Self::default() }

    pub async fn mapping(&self, bot: &str, s: &str) -> SessionMapping {
        self.map.read().await.get(&(bot.into(), s.into())).cloned().unwrap_or_default()
    }

    pub async fn set_engine_session_id(&self, bot: &str, s: &str, engine_id: &str) {
        self.map.write().await.entry((bot.into(), s.into()))
            .or_default().engine_session_id = Some(engine_id.into());
    }

    pub async fn add_inject(&self, bot: &str, s: &str, msg: InjectedMessage) {
        self.map.write().await.entry((bot.into(), s.into()))
            .or_default().pending_injects.push(msg);
    }

    pub async fn take_pending_injects(&self, bot: &str, s: &str) -> Vec<InjectedMessage> {
        let mut map = self.map.write().await;
        match map.get_mut(&(bot.into(), s.into())) {
            Some(m) => std::mem::take(&mut m.pending_injects),
            None => Vec::new(),
        }
    }

    pub async fn try_start_run(&self, bot: &str, s: &str, run_id: &str) -> Result<(), SessionBusy> {
        let mut map = self.map.write().await;
        let m = map.entry((bot.into(), s.into())).or_default();
        if m.active_run.is_some() { return Err(SessionBusy); }
        m.active_run = Some(run_id.into());
        Ok(())
    }

    pub async fn finish_run(&self, bot: &str, s: &str, run_id: &str) {
        let mut map = self.map.write().await;
        if let Some(m) = map.get_mut(&(bot.into(), s.into())) {
            if m.active_run.as_deref() == Some(run_id) { m.active_run = None; }
        }
    }

    pub async fn active_run(&self, bot: &str, s: &str) -> Option<String> {
        self.map.read().await.get(&(bot.into(), s.into()))
            .and_then(|m| m.active_run.clone())
    }
}
```

`AppState` 增加 `pub sessions: SessionStore`，`AppState::new` 里初始化。

- [ ] **Step 4: 运行确认通过并提交**

Run: `cargo test -p bridge-provider session`

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): session store with dual-id mapping"
```

---

### Task 7: CliSession（子进程管道）

**Files:**
- Create: `crates/adapters/bridge-provider/src/engine/mod.rs`
- Create: `crates/adapters/bridge-provider/src/engine/cli.rs`
- Create: `crates/adapters/bridge-provider/tests/fixtures/mock_engine.sh`

**Interfaces:**
- Produces:
  - `pub struct CliSession { … }`
  - `impl CliSession { pub async fn spawn(bin: &Path, args: &[String], cwd: &Path, env: &[(String, String)]) -> std::io::Result<Self>; pub async fn write_line(&mut self, line: &str) -> std::io::Result<()>; pub async fn next_line(&mut self) -> std::io::Result<Option<String>>; pub async fn kill(&mut self); }`
  - spawn 必须 `kill_on_drop(true)`（进程随 bridge 退出/句柄释放被回收，防 zombie）

- [ ] **Step 1: 写 mock 引擎脚本 + 失败测试**

`tests/fixtures/mock_engine.sh`（echo 服务：读 stdin 行、原样写回，再逐行吐两个事件并等 EOF）：

```bash
#!/usr/bin/env bash
# mock cfuse：按行读 stdin；每读一行回显 "ack:<line>"；收到 "quit" 时输出终态并退出
while IFS= read -r line; do
  if [ "$line" = "quit" ]; then
    printf '{"type":"result","subtype":"success","result":"done","session_id":"sess-1"}\n'
    exit 0
  fi
  printf 'ack:%s\n' "$line"
done
```

测试（`cli.rs` 内 `#[cfg(test)]`）：

```rust
#[tokio::test]
async fn cli_session_echo_and_kill() {
    let mut cli = CliSession::spawn(
        std::path::Path::new("bash"),
        &["tests/fixtures/mock_engine.sh".to_string()],
        std::path::Path::new("."),
        &[],
    ).await.unwrap();
    cli.write_line("hello").await.unwrap();
    let line = cli.next_line().await.unwrap().unwrap();
    assert_eq!(line, "ack:hello");
    cli.kill().await;
}
```

（注：集成测试 cwd 是 crate 根；若失败用 `env!("CARGO_MANIFEST_DIR")` 拼绝对路径。）

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider engine::cli`
Expected: 编译失败

- [ ] **Step 3: 实现 `engine/cli.rs`**

```rust
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

pub struct CliSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl CliSession {
    pub async fn spawn(bin: &Path, args: &[String], cwd: &Path, env: &[(String, String)])
        -> std::io::Result<Self>
    {
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        for (k, v) in env { cmd.env(k, v); }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io_err("stdin not piped"))?;
        let stdout = child.stdout.take().ok_or_else(|| io_err("stdout not piped"))?;
        let mut stderr = child.stderr.take().ok_or_else(|| io_err("stderr not piped"))?;
        tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => tracing::debug!(target: "bridge_provider::engine", stderr = line.trim_end()),
                }
            }
        });
        Ok(Self { child, stdin, stdout: BufReader::new(stdout) })
    }

    pub async fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await
    }

    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).await?;
        if n == 0 { return Ok(None); }
        Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
    }

    pub async fn kill(&mut self) {
        if let Err(e) = self.child.start_kill() {
            tracing::debug!(target: "bridge_provider::engine", "kill failed: {e}");
        }
        let _ = self.child.wait().await;
    }
}

fn io_err(msg: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, msg)
}
```

`engine/mod.rs` 先只放 `pub mod cli;`。

- [ ] **Step 4: 运行确认通过并提交**

Run: `cargo test -p bridge-provider engine::cli`

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): CliSession subprocess plumbing"
```

---

### Task 8: Engine trait + 工厂

**Files:**
- Modify: `crates/adapters/bridge-provider/src/engine/mod.rs`

**Interfaces:**
- Consumes: `config::{EngineKind, BotConfig}`、`session::SessionStore`、`interaction::InteractionRegistry`（Task 12 才创建；本任务先把参数类型定义为 `crate::interaction::InteractionRegistry` 的前置引用——为避免循环依赖，本任务先定义 trait 对象里不带 interaction；Task 12 再扩展 `TurnRequest`）
- Produces:
  ```rust
  pub struct TurnRequest {
      pub run_id: String,               // = BCS 下游 body id；帧 runId 用它
      pub prompt: String,
      pub engine_session_id: Option<String>,
      pub cwd: PathBuf,
      pub model: Option<String>,
      pub cfuse_bin: PathBuf,
      pub permission_mode: Option<String>,
  }
  pub struct TurnOutcome { pub engine_session_id: Option<String>, pub final_text: Option<String> }
  pub enum TurnError { Spawn(std::io::Error), EngineExited(String), Aborted, Protocol(String) }

  #[async_trait::async_trait]
  pub trait Engine: Send + Sync {
      fn kind(&self) -> EngineKind;
      async fn run_turn(&self, req: TurnRequest,
                        events: tokio::sync::mpsc::Sender<StreamEvent>,
                        abort: tokio_util::sync::CancellationToken)
          -> Result<TurnOutcome, TurnError>;
  }

  pub fn build_engine(bot: &BotConfig) -> std::sync::Arc<dyn Engine>
  ```

- [ ] **Step 1: 写失败测试（fake engine 编译锚点）**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bcs_protocol::stream::StreamEvent;

    struct FakeEngine;
    #[async_trait::async_trait]
    impl Engine for FakeEngine {
        fn kind(&self) -> EngineKind { EngineKind::CfuseCc }
        async fn run_turn(&self, req: TurnRequest,
                          events: tokio::sync::mpsc::Sender<StreamEvent>,
                          _abort: tokio_util::sync::CancellationToken)
            -> Result<TurnOutcome, TurnError> {
            let _ = events.send(crate::sse::chat_delta(&req.run_id, "fake")).await;
            Ok(TurnOutcome { engine_session_id: Some("e-1".into()), final_text: Some("done".into()) })
        }
    }

    #[tokio::test]
    async fn fake_engine_emits_delta() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let engine = FakeEngine;
        let req = TurnRequest {
            run_id: "r-1".into(), prompt: "hi".into(), engine_session_id: None,
            cwd: ".".into(), model: None, cfuse_bin: "cfuse".into(), permission_mode: None,
        };
        let outcome = engine.run_turn(req, tx, tokio_util::sync::CancellationToken::new()).await.unwrap();
        assert_eq!(outcome.engine_session_id.as_deref(), Some("e-1"));
        assert!(rx.recv().await.is_some());
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider engine`
Expected: 编译失败（trait 未定义）

- [ ] **Step 3: 实现 trait/类型/工厂骨架**

`engine/mod.rs` 写入上述 `TurnRequest/TurnOutcome/TurnError/Engine`。`build_engine` 本任务先返回一个内部 `StubEngine`（`run_turn` 立即返回 `Err(TurnError::EngineExited("engine not wired".into()))`），Task 9/10 完成后替换为 `CfuseCc::new(bin)` / `CfuseCodex::new(bin)`——**禁止用 `unimplemented!`**（生产代码不 panic）。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider engine`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): Engine trait and turn types"
```

（本任务故意薄：类型对齐是目的。）

---

### Task 9: CfuseCc 驱动（stream-json ↔ StreamEvent）

**Files:**
- Create: `crates/adapters/bridge-provider/src/engine/cfuse_cc.rs`
- Create: `crates/adapters/bridge-provider/tests/fixtures/cc_turn.ndjson`（录制的协议形状样例）
- Modify: `crates/adapters/bridge-provider/src/engine/mod.rs`（`build_engine` 接上）

**Interfaces:**
- Consumes: `engine/mod.rs` 的 `Engine/TurnRequest/TurnOutcome/TurnError`、`cli::CliSession`、`sse` 构造器
- Produces:
  - `pub struct CfuseCc { bin: PathBuf }`，`impl Engine for CfuseCc`
  - `pub(crate) fn map_cc_line(line: &str, run_id: &str) -> CcMap`（纯函数，便于测试）：
    `enum CcMap { Events(Vec<StreamEvent>), SessionId(String), Final(String), Ignore, Malformed }`

引擎调用形态（对齐 aix-relay `codefuse_direct_args`，spec §4.2）：

```text
cfuse --cc --output-format stream-json --verbose --input-format stream-json
      --include-partial-messages
      [--permission-mode <mode>] [--resume <engine_session_id>] [--model <model>]
```

启动后立刻向 stdin 写一条 user 消息（claude stream-json 输入格式）：

```json
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"<prompt>"}]}}
```

事件映射表（cc stream-json → StreamEvent）：

| cc 事件 | 映射 |
| --- | --- |
| `{"type":"system","subtype":"init","session_id":…}` | `CcMap::SessionId` |
| `{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":…}}}` | `chat_delta` |
| `{"type":"assistant","message":{"content":[{"type":"tool_use","id","name","input"}]}}` | `agent_tool(Start, name, toolCallId=id, args=input)` |
| `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id","content"}]}}` | `agent_tool(Result, toolCallId=tool_use_id, result=…)` |
| `{"type":"result","subtype":"success","result":…}` | `CcMap::Final(text)` |
| `{"type":"result","subtype":"error*"}` / 非零退出 | `TurnError::EngineExited` |
| `{"type":"control_request","request":{"subtype":"can_use_tool",…}}` | Task 12 接线；本任务先映射为 `agent_thinking`（占位行为会产生一条可观测 thinking 事件，但绝不发 `stream:approval`） |

- [ ] **Step 1: 录制 fixture + 写失败测试**

`tests/fixtures/cc_turn.ndjson`（逐行）：

```json
{"type":"system","subtype":"init","session_id":"cc-sess-1"}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"正在"}}}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"分析"}}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}}
{"type":"result","subtype":"success","result":"完成了","session_id":"cc-sess-1"}
```

测试：

```rust
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
            CcMap::Ignore | CcMap::Malformed => {}
        }
    }
    assert_eq!(session_id.as_deref(), Some("cc-sess-1"));
    assert_eq!(deltas, "正在分析");
    assert_eq!(tools, 2);
    assert_eq!(final_text.as_deref(), Some("完成了"));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider cfuse_cc`
Expected: 编译失败（`map_cc_line` 不存在）

- [ ] **Step 3: 实现 `map_cc_line` + `CfuseCc::run_turn`**

实现要点：`map_cc_line` 按上表逐类解析（`serde_json::from_str::<Value>` 后按 `type`/`subtype` 分派；无法识别 → `CcMap::Ignore`，JSON 非法 → `CcMap::Malformed`）。`run_turn`：`CliSession::spawn` → 写 user 消息行 → 关键循环如下。EOF 无 final → `TurnError::EngineExited`；abort → `cli.kill()`。`TurnError` 增加变体 `Io(#[from] std::io::Error)`。`build_engine` 在此接上：`EngineKind::CfuseCc => Arc::new(CfuseCc::new(bot.cfuse_bin.clone().unwrap_or_else(|| "cfuse".into())))`。

`run_turn` 关键循环：

```rust
loop {
    tokio::select! {
        _ = abort.cancelled() => { cli.kill().await; return Err(TurnError::Aborted); }
        line = cli.next_line() => {
            let Some(line) = line.map_err(TurnError::Io)? else {
                return Err(TurnError::EngineExited("stdout EOF before result".into()));
            };
            match map_cc_line(&line, &req.run_id) {
                CcMap::SessionId(s) => engine_session_id = Some(s),
                CcMap::Events(evs) => for ev in evs {
                    if events.send(ev).await.is_err() { cli.kill().await; return Err(TurnError::Aborted); }
                },
                CcMap::Final(text) => return Ok(TurnOutcome { engine_session_id, final_text: Some(text) }),
                CcMap::Ignore | CcMap::Malformed => {}
            }
        }
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider cfuse_cc`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): CfuseCc driver mapping claude stream-json"
```

---

### Task 10: CfuseCodex 驱动（codex SSE ↔ StreamEvent）

**Files:**
- Create: `crates/adapters/bridge-provider/src/engine/cfuse_codex.rs`
- Create: `crates/adapters/bridge-provider/tests/fixtures/codex_turn.sse`
- Modify: `crates/adapters/bridge-provider/src/engine/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct CfuseCodex { bin: PathBuf }`，`impl Engine`
  - `pub(crate) fn map_codex_block(event: &str, data: &str, run_id: &str) -> CodexMap`：`enum CodexMap { Events(Vec<StreamEvent>), Final(String), Failed(String), Ignore }`

codex 输出为 SSE 帧（`event:/data:` 空行分隔），CliSession 需按块读：本任务给 `cli.rs` 加 `pub async fn next_sse_block(&mut self) -> std::io::Result<Option<(String, String)>>`（聚合到空行）。

映射表（对齐 aix-relay `codefuse_codex.rs` 测试样本）：

| codex 事件 | 映射 |
| --- | --- |
| `response.output_text.delta` `{"delta":…}` | `chat_delta` |
| `response.completed` | `CodexMap::Final(累计文本)` |
| `response.failed` / `error` | `CodexMap::Failed(脱敏 message)` |

**实现前置调研步骤（必做）**：读 `~/workspace/aix-engine-workspace/crates/relay/src/runtime/codefuse_codex.rs` 的 spawn 参数构造与会话 resume 方式（搜索 `Command`/`resume`/`session`），把真实 cfuse codex 调用参数与本驱动对齐；若 cfuse codex 模式不支持 `--resume` 等价物，则 `engine_session_id` 恒为 `None` 并在代码注释注明限制（spec 允许：会话上下文由引擎 transcript 保证的前提不成立时，回退为"每次新会话 + pending injects 前置"）。

> **执行期修正（controller ruling，已实测验证）**：`cfuse --codex` 是 codex CLI 透传；真实输出形态是 `codex exec --json` 的 **JSONL**（`thread.started`/`turn.started`/`item.completed{agent_message,reasoning}`/`turn.completed`），**不是** SSE。resume 用 `codex exec resume <thread_id> [prompt]`（engine session id = thread_id）。映射：`thread.started`→SessionId；`agent_message`→chat_delta；`reasoning`→agent_thinking；`turn.completed`→Final（累计文本）；`turn.failed`/`error`→Failed。上方 SSE 映射表与 fixture 形态以本修正为准。

- [ ] **Step 1: 录制 fixture + 写失败测试**

`tests/fixtures/codex_turn.sse`：

```text
event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"正在"}

event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"排查"}

event: response.completed
data: {"type":"response.completed"}

```

测试（`cfuse_codex.rs` 内 `#[cfg(test)]`）：

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider cfuse_codex`
Expected: 编译失败

- [ ] **Step 3: 实现 `next_sse_block` + mapper + `run_turn`**

`cli.rs` 增加：

```rust
/// 读取一个 SSE 块（以空行分隔），返回 (event, data)。EOF 且无残留 → Ok(None)。
pub async fn next_sse_block(&mut self) -> std::io::Result<Option<(String, String)>> {
    let mut event = String::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut saw_any = false;
    loop {
        match self.next_line().await? {
            None => return Ok(saw_any.then(|| (event, data_lines.join("\n")))),
            Some(line) if line.is_empty() => {
                return Ok(if saw_any { Some((event, data_lines.join("\n"))) } else { None });
            }
            Some(line) => {
                saw_any = true;
                if let Some(v) = line.strip_prefix("event: ") { event = v.to_string(); }
                if let Some(v) = line.strip_prefix("data: ") { data_lines.push(v.to_string()); }
            }
        }
    }
}
```

`run_turn`：prompt 经 argv 或 stdin 传入（以前置调研结论为准）；逐 block map → `events.send`；`Final` → 返回 `TurnOutcome`；EOF 无 terminal → `TurnError::EngineExited`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider cfuse_codex`

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): CfuseCodex driver mapping codex SSE"
```

---

### Task 11: RunRegistry + run loop + chat.send 端到端

**Files:**
- Create: `crates/adapters/bridge-provider/src/run.rs`
- Modify: `crates/adapters/bridge-provider/src/webhook.rs`（chat.send handler）
- Test: `crates/adapters/bridge-provider/tests/e2e_webhook.rs`

**Interfaces:**
- Consumes: 此前全部任务
- Produces:
  - `pub struct RunRegistry { … }`
    - `pub fn begin(&self, run_id: &str) -> RunHandle`（创建 buffer/broadcast/abort token；同 id 已存在 → 返回既有 handle 用于重挂判断）
    - `pub fn get(&self, run_id: &str) -> Option<RunHandle>`
    - `pub fn finish(&self, run_id: &str)`（标记 terminal，buffer 保留进 grace TTL，由 lazy sweep 清理）
  - `pub struct RunHandle { pub abort: CancellationToken, tx: broadcast::Sender<String>, buffer: Arc<RwLock<Vec<String>>>, terminal: Arc<AtomicBool> }`
  - `pub fn spawn_run(state: Arc<AppState>, req: DownstreamRequest, bot: BotConfig) -> impl Stream<Item = String>`：驱动整个 turn 并把帧推入 broadcast+buffer

run loop（spec §6.2/§6.4，核心 select）：

```rust
let mut seq: u64 = 0;
let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms.saturating_sub(30_000)));
tokio::pin!(deadline);
loop {
    tokio::select! {
        _ = &mut deadline => {
            push(chat_error(&run_id, "run deadline exceeded", Some("deadline")));
            break;
        }
        _ = heartbeat.tick() => { push_raw(HEARTBEAT); }
        ev = events_rx.recv() => {
            match ev {
                Some(StreamEvent::Chat(c)) if c.state == ChatState::Final => { push_ev; break; }
                Some(ev) => push_ev,
                None => { push(chat_error(&run_id, "engine exited without terminal", Some("runtime_error"))); break; }
            }
        }
    }
}
// push 时：seq += 1；event_to_frame(ev, seq, now_ms, run_id) → buffer.push + tx.send；
// tx.send 返回 Err（无订阅者）= BCS 断连 → abort 引擎、收尾退出（spec 修正项：写失败即杀）
```

handler 流程（chat.send）：

1. 校验 `X-BCN-Protocol-Version: 2.0`（否则 400）、`message` 存在、`session_id` 存在（400）
2. `config.bot(ref)` → 404
3. `sessions.try_start_run` → 冲突 429
4. 幂等：RunRegistry 同 id 活跃 handle → 重挂（replay buffer + subscribe broadcast）；同 id terminal → 单帧重放终态
5. 正常：立即构造 SSE 响应流（`Body::from_stream`），spawn run task

响应构造（自管理帧文本，不经 axum Event 格式化——单一路径可测）：

```rust
let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
// attach: 先回放 buffer，再转发 broadcast
let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
    .map(|s| Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(s)));
Response::builder()
    .header(header::CONTENT_TYPE, "text/event-stream; charset=utf-8")
    .header(header::CACHE_CONTROL, "no-cache")
    .body(Body::from_stream(stream))
```

转发 task：回放 buffer 快照 → 循环 `broadcast::Receiver::recv()` → 写入 mpsc；`Err(Lagged)` 记 warning 继续（BCS 侧按 seq gap 容忍）；terminal 帧后退出。

本任务同时把 `tests/support/mod.rs` 扩展出两个 helper（此前任务未用到，故放在这里定义，避免 dead_code lint）：

```rust
/// 用指定 mock 脚本作为 cfuse binary 起服务。
pub async fn spawn_app_with_mock(script: &str, engine: &str) -> String {
    let bin = format!("{}/tests/fixtures/{script}", env!("CARGO_MANIFEST_DIR"));
    spawn_app(&format!(r#"
provider_id = "bridge-1"
listen = "127.0.0.1:0"
bcs_to_provider_token = "tok-b2p"
[[bot]]
provider_bot_ref = "worker-1"
engine = "{engine}"
cwd = "/tmp"
cfuse_bin = "{bin}"
"#)).await
}

/// 从 SSE 文本抽取 data 里的 seq 序列。
pub fn extract_seqs(sse_text: &str) -> Vec<u64> {
    sse_text.lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .filter_map(|v| v["seq"].as_u64())
        .collect()
}
```

另两个接线约定：

- `AppState::new(config)` 内部构造全部 store（`SessionStore/RunRegistry/InteractionRegistry/IdempotencyLedger`），测试侧的调用签名不变。
- `webhook.rs` 的 `dispatch` 从本任务起改为 `async fn`（chat.send 要构造流式响应）；`handle_webhook` 相应 `.await`。

- [ ] **Step 1: 写失败测试（端到端：mock cc 引擎跑完整 turn）**

用 `tests/fixtures/mock_cc.sh`（读一行 stdin，逐行吐 `cc_turn.ndjson` 内容）作为 `cfuse_bin`：

```bash
#!/usr/bin/env bash
IFS= read -r _first
cat "$(dirname "$0")/cc_turn.ndjson"
```

`mock_cc_slow.sh`（429 测试用）：

```bash
#!/usr/bin/env bash
IFS= read -r _first
sleep 30
printf '{"type":"result","subtype":"success","result":"done","session_id":"sess-1"}\n'
```

测试：

```rust
#[tokio::test]
async fn chat_send_streams_sse_to_final() {
    let url = support::spawn_app_with_mock("mock_cc.sh", "cfuse-cc").await;
    let resp = reqwest::Client::new().post(format!("{url}/webhook"))
        .bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&json!({"type":"req","id":"run-1","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"cc-worker"},
            "message":{"role":"user","content":[{"type":"text","text":"你好"}]}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers()["content-type"].to_str().unwrap().starts_with("text/event-stream"));
    let text = resp.text().await.unwrap();
    assert!(text.contains("event: agent"));            // tool 事件
    assert!(text.contains("\"state\":\"delta\""));
    assert!(text.contains("\"deltaText\":\"正在\""));
    assert!(text.contains("\"state\":\"final\""));
    assert!(text.contains("完成了"));
    // seq 单调
    let seqs = support::extract_seqs(&text);
    assert!(seqs.windows(2).all(|w| w[0] < w[1]));
}

#[tokio::test]
async fn concurrent_send_same_session_gets_429() {
    // mock_cc_slow.sh：读 stdin 后 sleep 30 再吐结果，保证第一个 run 仍在执行
    let url = support::spawn_app_with_mock("mock_cc_slow.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let body = |id: &str| serde_json::json!({"type":"req","id":id,"method":"chat.send",
        "session_id":"s-1",
        "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
        "message":{"role":"user","content":[{"type":"text","text":"hi"}]}});

    let first = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0").json(&body("run-a")).send().await.unwrap();
    assert_eq!(first.status(), 200); // SSE 流已建立（后台持有）

    let second = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0").json(&body("run-b")).send().await.unwrap();
    assert_eq!(second.status(), 429);
    let err: serde_json::Value = second.json().await.unwrap();
    assert_eq!(err["error"]["code"], serde_json::json!("rate_limited"));
    assert_eq!(err["error"]["retryable"], serde_json::json!(true));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider --test e2e_webhook chat_send`
Expected: FAIL（chat.send 仍是 503 占位）

- [ ] **Step 3: 实现 RunRegistry + run loop + handler**

按本任务上方的循环骨架与 handler 流程实现。`chat.send` handler 取代 Task 4 的占位分支；幂等/429/404/400 校验顺序见流程 1–5。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider`

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): run loop and chat.send SSE end-to-end"
```

---

### Task 12: InteractionRegistry + interaction.resolve + 驱动接线

**Files:**
- Create: `crates/adapters/bridge-provider/src/interaction.rs`
- Modify: `crates/adapters/bridge-provider/src/engine/mod.rs`（`TurnRequest` 加 `pub interactions: InteractionRegistry`）
- Modify: `crates/adapters/bridge-provider/src/engine/cfuse_cc.rs`（control_request 接线）
- Modify: `crates/adapters/bridge-provider/src/webhook.rs`（interaction.resolve handler）
- Create: `crates/adapters/bridge-provider/tests/fixtures/mock_cc_approval.sh`

**Interfaces:**
- Produces:
  ```rust
  pub struct InteractionRegistry { … } // Clone，内部 Arc<Mutex<…>>
  pub struct PendingInteraction {
      pub run_id: String,
      pub kind: InteractionKind,
      pub engine_request_id: String,     // 引擎原生 id（cc 的 control request_id），不回泄
      pub idempotency_key: Option<String>,
      resolver: Option<oneshot::Sender<Value>>,
  }
  pub enum ResolveOutcome { Delivered, Duplicate, Unknown }
  impl InteractionRegistry {
      pub fn register(&self, run_id: &str, kind: InteractionKind, engine_request_id: String)
          -> (String /*interaction_id*/, oneshot::Receiver<Value>);
      pub fn resolve(&self, interaction_id: &str, key: &str, resolution: Value) -> ResolveOutcome;
      pub fn invalidate_run(&self, run_id: &str, fallback: Value); // abort/deadline 时兜底释放
  }
  ```
  interaction_id 生成：`format!("int-{}", uuid::Uuid::new_v4().simple())`。
- webhook `interaction.resolve`（spec §5.1；注意 ACK 错误形态为字符串 error）：
  - 参数：`params.{interactionId, idempotencyKey, kind, decision|action+answers}`
  - `Delivered` → `{"ok":true}`；`Duplicate` → `{"ok":true}`；`Unknown` → `{"ok":false,"retryable":false,"error":"unknown interaction"}`
  - 幂等：同 `idempotencyKey` 重复 → `Duplicate` 直接成功，不重复回写引擎

本任务给 `CcMap` 增加变体（Task 9 的占位分支随之删除）：

```rust
enum CcMap { /* …已有…, */ ControlRequest { request_id: String, tool_name: String, input: Value } }
```

cc 驱动接线（`map_cc_line` 的 control_request 分支改为真正挂起）：

```rust
// run_turn 内
CcMap::ControlRequest { request_id, tool_name, input } => {
    let kind = if tool_name == "AskUserQuestion" { InteractionKind::AskUser } else { InteractionKind::Exec };
    let (iid, resolution_rx) = interactions.register(&req.run_id, kind, request_id.clone());
    let requested = build_requested_extra(&tool_name, &input); // exec: command/options；ask_user: questions
    let _ = events.send(interaction_event(&req.run_id, InteractionPhase::Requested, kind, &iid, requested)).await;
    let resolution = tokio::select! {
        _ = abort.cancelled() => { json!({"decision":"deny"}) }
        r = resolution_rx => r.unwrap_or_else(|_| json!({"decision":"deny"})),
    };
    let behavior = if resolution["decision"].as_str() == Some("deny") { "deny" } else { "allow" };
    cli.write_line(&json!({
        "type":"control_response",
        "response":{"request_id": request_id,
                    "response":{"behavior": behavior,
                                "updatedInput": (behavior == "allow").then(|| input.clone())}}
    }).to_string()).await.map_err(TurnError::Io)?;
    let _ = events.send(interaction_event(&req.run_id, InteractionPhase::Resolved, kind, &iid,
        json!({"decision": resolution["decision"]}))).await;
}
```

- AskUserQuestion 的 `input.questions[]` → BCN ask_user `questions[]`：`header/question/multiSelect/options[].{label→label,label→value}`（cc 无独立 value，用 label 回填；对齐 baas fallback 策略）。**含 secret 标记的问题拒绝转换**（spec §5.2）：该 control request 直接回 deny 并记 warning。
- exec 的 options 固定合成：`[{"decision":"allow_once","label":"Allow once"},{"decision":"deny","label":"Deny"}]`（对齐协议推荐值）。
- abort/deadline 时 run loop 调 `interactions.invalidate_run(run_id, fallback)` 释放挂起的 oneshot（driver 收到 fallback 后向引擎写 deny）。

mock 引擎 `mock_cc_approval.sh`：读 user 消息 → 吐 `control_request(can_use_tool)` → 等 stdin 的 `control_response` → 按 behavior 吐 result。

- [ ] **Step 1: 写失败测试（registry 单测 + e2e：interaction 全流程）**

registry 单测（`interaction.rs` 内）：

```rust
#[tokio::test]
async fn resolve_delivers_and_duplicate_key_replays() {
    let reg = InteractionRegistry::new();
    let (iid, rx) = reg.register("run-1", InteractionKind::Exec, "engine-req-1".into());
    assert!(matches!(
        reg.resolve(&iid, "key-1", serde_json::json!({"decision":"allow_once"})),
        ResolveOutcome::Delivered));
    assert_eq!(rx.await.unwrap()["decision"], serde_json::json!("allow_once"));
    // 同 key 重复 → Duplicate（不再投递）
    assert!(matches!(
        reg.resolve(&iid, "key-1", serde_json::json!({"decision":"allow_once"})),
        ResolveOutcome::Duplicate));
    // 未知 id → Unknown
    assert!(matches!(
        reg.resolve("int-nope", "key-2", serde_json::json!({"decision":"deny"})),
        ResolveOutcome::Unknown));
}
```

e2e（`tests/e2e_webhook.rs`）——`mock_cc_approval.sh`：读 user 消息后吐
`control_request`（`can_use_tool` Bash），`read` 等待 `control_response`，按
`behavior` 吐 result：

```rust
#[tokio::test]
async fn interaction_roundtrip_over_sse_and_resolve_webhook() {
    let url = support::spawn_app_with_mock("mock_cc_approval.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&serde_json::json!({"type":"req","id":"run-1","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"执行一下"}]}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    let mut acc = String::new();
    // 读到 interaction/requested 帧为止
    let iid = loop {
        let chunk = stream.next().await.unwrap().unwrap();
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("\"phase\":\"requested\"") {
            break support::extract_first_interaction_id(&acc);
        }
    };
    // BCS 回程：interaction.resolve
    let ack = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"resolve-1","method":"interaction.resolve",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "params":{"bcsRunId":"run-1","runId":"run-1","interactionId":iid,
                      "kind":"exec","idempotencyKey":"key-1","decision":"allow_once"}}))
        .send().await.unwrap();
    assert_eq!(ack.json::<serde_json::Value>().await.unwrap()["ok"], serde_json::json!(true));
    // 幂等重放同 key
    let dup = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"resolve-2","method":"interaction.resolve",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "params":{"bcsRunId":"run-1","runId":"run-1","interactionId":iid,
                      "kind":"exec","idempotencyKey":"key-1","decision":"allow_once"}}))
        .send().await.unwrap();
    assert_eq!(dup.json::<serde_json::Value>().await.unwrap()["ok"], serde_json::json!(true));
    // 未知 interactionId → 字符串形态 error
    let unknown = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"resolve-3","method":"interaction.resolve",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "params":{"bcsRunId":"run-1","runId":"run-1","interactionId":"int-nope",
                      "kind":"exec","idempotencyKey":"key-9","decision":"deny"}}))
        .send().await.unwrap();
    let body: serde_json::Value = unknown.json().await.unwrap();
    assert_eq!(body["ok"], serde_json::json!(false));
    assert!(body["error"].is_string());  // 注意：此方法的 error 是字符串（spec §5.1）
    // 流继续：resolved → chat/final
    while let Some(chunk) = stream.next().await {
        acc.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        if acc.contains("\"state\":\"final\"") { break; }
    }
    assert!(acc.contains("\"phase\":\"resolved\""));
    assert!(acc.contains("\"state\":\"final\""));
}
```

`tests/support/mod.rs` 增加：

```rust
pub fn extract_first_interaction_id(sse_text: &str) -> String {
    sse_text.lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .find(|v| v["interactionId"].is_string())
        .and_then(|v| v["interactionId"].as_str().map(str::to_string))
        .expect("interaction requested frame present")
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider interaction`
Expected: 编译失败（`InteractionRegistry` 不存在）

- [ ] **Step 3: 实现 registry + resolve handler + 驱动接线**

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider interaction`

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): HITL interaction bridging via control channel"
```

---

### Task 13: chat.inject + cc TranscriptSink + codex 降级

**Files:**
- Modify: `crates/adapters/bridge-provider/src/webhook.rs`（chat.inject handler）
- Create: `crates/adapters/bridge-provider/src/engine/transcript.rs`
- Modify: `crates/adapters/bridge-provider/src/run.rs`（prompt 组装时消费 pending injects）

**Interfaces:**
- Produces:
  - `pub trait TranscriptSink: Send + Sync { fn append_user_message(&self, cwd: &Path, engine_session_id: &str, msg: &InjectedMessage) -> Result<(), TranscriptError>; }`
  - `pub struct ClaudeJsonlSink;`（cc 用；`~/.claude/projects/<encoded-cwd>/<session>.jsonl`；幂等：条目带 `bridgeInjectId = run_id`，append 前扫尾部去重；仿 aix-relay `ClaudeJsonlSink` 但只做最小集：leaf uuid 链接可省——新消息 parentUuid 取文件末行 uuid，找不到则省略）
  - codex：`None` sink → injects 留在 `pending_injects`，下次 chat.send 时前置注入 prompt：

    ```text
    [from:张三] 注入的消息一
    注入的消息二

    <本次 message 文本>
    ```

- chat.inject handler 流程：幂等台账（Task 5）→ `sessions.add_inject` → sink 成功则从 pending 移除（已落引擎 transcript）→ `{"ok":true}`。

- [ ] **Step 1: 写失败测试**

```rust
// engine/transcript.rs 内 #[cfg(test)]
#[test]
fn claude_jsonl_sink_appends_idempotently() {
    let dir = tempfile::tempdir().unwrap();
    // Claude 项目目录布局：<root>/<encoded-cwd>/<session>.jsonl；encoded-cwd = 路径 '/'→'-'
    let projects = dir.path().join("projects");
    let sess_dir = projects.join("-tmp-work");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let sess_file = sess_dir.join("sess-1.jsonl");
    std::fs::write(&sess_file, "{\"type\":\"assistant\",\"uuid\":\"u1\",\"message\":{}}\n").unwrap();

    let sink = ClaudeJsonlSink::with_projects_root(projects.clone());
    let msg = InjectedMessage { run_id: "inj-1".into(), from_name: Some("张三".into()), text: "观察".into() };
    sink.append_user_message(Path::new("/tmp/work"), "sess-1", &msg).unwrap();
    sink.append_user_message(Path::new("/tmp/work"), "sess-1", &msg).unwrap(); // 幂等

    let content = std::fs::read_to_string(&sess_file).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2); // 只新增一条
    let appended: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(appended["type"], serde_json::json!("user"));
    assert_eq!(appended["parentUuid"], serde_json::json!("u1"));
    assert_eq!(appended["bridgeInjectId"], serde_json::json!("inj-1"));
    assert_eq!(appended["message"]["content"][0]["text"], serde_json::json!("[from:张三] 观察"));
}
```

`ClaudeJsonlSink::with_projects_root` 是测试构造器；生产 `ClaudeJsonlSink::default_home()`
解析 `$HOME/.claude/projects`。

```rust
// tests/e2e_webhook.rs
#[tokio::test]
async fn inject_then_send_prepends_for_codex() {
    // mock_codex.sh：把 argv 里的 prompt 作为 delta 回显
    let url = support::spawn_app_with_mock("mock_codex.sh", "cfuse-codex").await;
    let client = reqwest::Client::new();
    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"inj-1","method":"chat.inject",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"观察上下文"}]},
            "from":{"kind":"bot","name":"观察者"}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["ok"], serde_json::json!(true));

    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&serde_json::json!({"type":"req","id":"run-9","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"正式问题"}]}}))
        .send().await.unwrap();
    let text = resp.text().await.unwrap();
    assert!(text.contains("观察上下文"));  // 注入被前置进 prompt
    assert!(text.contains("正式问题"));
}
```

`mock_codex.sh`（回显 prompt 的 codex 假引擎）——注意：**执行期修正后 codex 输出是 JSONL**（`codex exec --json` 形态），不是 SSE：

```bash
#!/usr/bin/env bash
prompt="$*"
printf '{"type":"thread.started","thread_id":"mock-thread-1"}\n'
printf '{"type":"turn.started"}\n'
# prompt 里的双引号/反斜杠需转义后再嵌入 JSON；mock 场景的 prompt 不含特殊字符，直接拼接
printf '{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"%s"}}\n' "$prompt"
printf '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}\n'
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider transcript inject`
Expected: 编译失败（`transcript` 模块不存在）

- [ ] **Step 3: 实现 transcript.rs + inject handler + prompt 前置组装**

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider`

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): chat.inject with engine transcript sink"
```

---

### Task 14: chat.abort

**Files:**
- Modify: `crates/adapters/bridge-provider/src/webhook.rs`、`run.rs`、`session.rs`

**Interfaces:**
- handler 流程（spec §5.3 响应形态）：
  1. `sessions.active_run(bot, session_id)` 有值 → `runs.get(run_id).abort.cancel()` → run loop 收到 abort → driver `cli.kill()` → 发 `chat_aborted` 终态 → `{"ok":true,"aborted":true,"aborted_run_ids":[run_id]}`
  2. 无活跃但 RunRegistry 有该 session 的 terminal run → 410 `run_terminated`（对同 terminal run 重复 abort 稳定同答；幂等台账保证同 id 重放）
  3. 无任何记录 → `{"ok":true,"aborted":false,"aborted_run_ids":[]}`
  4. abort 命中时先 `interactions.invalidate_run(run_id, deny-fallback)` 释放挂起的 interaction
- `RunRegistry` 需要 `run_session: HashMap<run_id, (bot, session)>` 反查索引
- `RunRegistry` 增加 `pub async fn abort_all(&self, reason: &str)`（Task 15 优雅退出用）：遍历活跃 run 逐一 `abort.cancel()`

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn abort_active_run_emits_aborted_terminal() {
    let url = support::spawn_app_with_mock("mock_cc_slow.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let send = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&serde_json::json!({"type":"req","id":"run-1","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"慢任务"}]}}));
    // 后台持有 SSE 响应体
    let sse = tokio::spawn(async move { send.send().await.unwrap().text().await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await; // 等 run 起跑

    let resp = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"abort-1","method":"chat.abort",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
        .send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["aborted"], serde_json::json!(true));
    assert_eq!(body["aborted_run_ids"], serde_json::json!(["run-1"]));

    let sse_text = tokio::time::timeout(std::time::Duration::from_secs(5), sse).await.unwrap().unwrap();
    assert!(sse_text.contains("\"state\":\"aborted\""));

    // 对同一 terminal run 重复 abort → 410 run_terminated（稳定）
    let again = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"abort-2","method":"chat.abort",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
        .send().await.unwrap();
    assert_eq!(again.status(), 410);
    assert_eq!(again.json::<serde_json::Value>().await.unwrap()["error"]["code"],
               serde_json::json!("run_terminated"));

    // 无任何记录的 session → aborted:false
    let none = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"abort-3","method":"chat.abort",
            "session_id":"s-unknown",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
        .send().await.unwrap();
    let none_body: serde_json::Value = none.json().await.unwrap();
    assert_eq!(none_body["aborted"], serde_json::json!(false));
    assert_eq!(none_body["aborted_run_ids"], serde_json::json!([]));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider --test e2e_webhook abort`
Expected: FAIL（chat.abort 返回 501/未实现）

- [ ] **Step 3: 实现 abort handler + run 反查索引 + invalidate_run 接线**

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider`

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): chat.abort with terminal-state matrix"
```

---

### Task 15: main.rs + 优雅退出 + HTTP/2 验证

**Files:**
- Create: `crates/adapters/bridge-provider/src/main.rs`

**内容：**
- 从 `BRIDGE_CONFIG`（默认 `bridge.toml`）加载配置；`tracing_subscriber` 初始化（env-filter）
- axum server 绑定 `config.listen`；`tokio::signal` SIGTERM/SIGINT → 停接新连接（hyper graceful）→ 遍历 RunRegistry 全部 `abort.cancel()` → 退出
- 手动验证步骤（不进 CI）：`curl --http2-prior-knowledge -N` 打 webhook，确认 h2c SSE 可用（协议要求生产 HTTP/2；axum/hyper auto builder 支持 h2c 先验）

- [ ] **Step 1: 写 smoke 测试**

```rust
// tests/e2e_webhook.rs
#[tokio::test]
async fn binary_starts_and_serves_ping() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("bridge.toml");
    std::fs::write(&cfg_path, r#"
provider_id = "bridge-1"
listen = "127.0.0.1:21999"
bcs_to_provider_token = "tok-b2p"
[[bot]]
provider_bot_ref = "worker-1"
engine = "cfuse-cc"
cwd = "/tmp"
"#).unwrap();
    let bin = env!("CARGO_BIN_EXE_bridge-provider");
    let mut child = std::process::Command::new(bin)
        .env("BRIDGE_CONFIG", &cfg_path)
        .spawn().unwrap();
    // 轮询直到端口就绪（最多 5s）
    let client = reqwest::Client::new();
    let mut ok = false;
    for _ in 0..50 {
        let resp = client.post("http://127.0.0.1:21999/webhook")
            .bearer_auth("tok-b2p")
            .json(&serde_json::json!({"type":"req","id":"p1","method":"bot.ping",
                "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"}}))
            .send().await;
        if let Ok(r) = resp {
            if r.status() == 200 { ok = true; break; }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // 优雅退出：SIGTERM 应让进程退出
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = tokio::time::timeout(std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || child.wait())).await
        .expect("process exits within 5s").unwrap().unwrap();
    assert!(status.success());
    assert!(ok, "ping should succeed while running");
}
```

（需要 `libc` dev-dependency：在 crate 的 `[dev-dependencies]` 加 `libc = "0.2"`——若根 workspace 已有则改 `{ workspace = true }`。）

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p bridge-provider --test e2e_webhook binary`
Expected: FAIL（binary 不存在）

- [ ] **Step 3: 实现 main.rs**

```rust
use std::{path::PathBuf, sync::Arc};
use bridge_provider::{config::ProviderConfig, webhook, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config_path: PathBuf = std::env::var("BRIDGE_CONFIG")
        .map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("bridge.toml"));
    let config = ProviderConfig::load(&config_path)?;
    let listen = config.listen;
    let state = Arc::new(AppState::new(config));
    let app = webhook::router(state.clone());
    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!(%listen, "bridge-provider listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            // 先停新连接；再中止全部活跃 run（aborted 终态），子进程随之回收
            state.runs.abort_all("shutdown").await;
        })
        .await?;
    Ok(())
}
```

（`anyhow` 已在 workspace 依赖；`AppState::new` 此时已含 `runs: RunRegistry`，`runs.abort_all(reason)` 在 Task 14 实现。）

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p bridge-provider --test e2e_webhook`

- [ ] **Step 5: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "feat(bridge-provider): binary entrypoint with graceful shutdown"
```

---

### Task 16: 协议回归 e2e（mock BCS 客户端语义）

**Files:**
- Test: `crates/adapters/bridge-provider/tests/e2e_webhook.rs`（追加）

**测试清单（对 spec §5/§6）：**

- [ ] **Step 1: 幂等重挂 + 终态重放 + 409/400**

```rust
#[tokio::test]
async fn duplicate_send_reattaches_with_replay() {
    // mock_cc_slow.sh：run 进行中时第二个同 id 请求到达
    let url = support::spawn_app_with_mock("mock_cc_slow.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({"type":"req","id":"run-dup","method":"chat.send",
        "session_id":"s-1",
        "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
        "message":{"role":"user","content":[{"type":"text","text":"hi"}]}});
    let first = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0").json(&body).send().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // 同 id 同 body 重发 → 200 且新流自 seq 1 重放（BCS 按 seq 去重，重放无害）
    let second = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0").json(&body).send().await.unwrap();
    assert_eq!(second.status(), 200);
    let second_text = second.text().await.unwrap();
    let seqs = support::extract_seqs(&second_text);
    assert_eq!(seqs.first(), Some(&1));
    drop(first); // 让第一个连接断开，不阻塞测试结束
}

#[tokio::test]
async fn same_id_different_body_conflicts() {
    let url = support::spawn_app_with_mock("mock_cc_slow.sh", "cfuse-cc").await;
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({"type":"req","id":"run-x","method":"chat.send",
        "session_id":"s-1",
        "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
        "message":{"role":"user","content":[{"type":"text","text":"hi"}]}});
    let _first = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0").json(&body).send().await.unwrap();
    body["message"]["content"][0]["text"] = serde_json::json!("changed");
    let second = client.post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0").json(&body).send().await.unwrap();
    assert_eq!(second.status(), 409);
}

#[tokio::test]
async fn missing_protocol_2_header_rejected() {
    let url = support::spawn_app_with_mock("mock_cc.sh", "cfuse-cc").await;
    let resp = reqwest::Client::new().post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .json(&serde_json::json!({"type":"req","id":"r","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"hi"}]}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}
```

- [ ] **Step 2: UTF-8 与超大帧回归**

```rust
#[tokio::test]
async fn utf8_chinese_deltas_stay_intact() {
    // mock_cc_utf8.sh：逐行吐 40 条中文 delta（每条一个完整 JSON 事件行）
    let url = support::spawn_app_with_mock("mock_cc_utf8.sh", "cfuse-cc").await;
    let resp = reqwest::Client::new().post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&serde_json::json!({"type":"req","id":"run-u","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"hi"}]}}))
        .send().await.unwrap();
    let text = resp.text().await.unwrap();  // text() 要求全程合法 UTF-8
    assert!(text.contains("中文增量"));
    // 每个 data 行都是合法 JSON（无半个字符截断）
    for line in text.lines().filter_map(|l| l.strip_prefix("data: ")) {
        serde_json::from_str::<serde_json::Value>(line).expect("valid json frame");
    }
}

#[tokio::test]
async fn oversize_single_frame_becomes_chat_error() {
    // mock_cc_big.sh：吐一条 >8MiB 的 text_delta
    let url = support::spawn_app_with_mock("mock_cc_big.sh", "cfuse-cc").await;
    let resp = reqwest::Client::new().post(format!("{url}/webhook")).bearer_auth("tok-b2p")
        .header("X-BCN-Protocol-Version", "2.0")
        .json(&serde_json::json!({"type":"req","id":"run-big","method":"chat.send",
            "session_id":"s-1",
            "to_bot":{"provider_id":"bridge-1","provider_bot_ref":"worker-1"},
            "message":{"role":"user","content":[{"type":"text","text":"hi"}]}}))
        .send().await.unwrap();
    let text = resp.text().await.unwrap();
    assert!(text.contains("\"state\":\"error\""));      // 超限 → error 终态
    assert!(!text.contains("\"state\":\"final\""));
    assert!(text.len() < 9 * 1024 * 1024);              // 没有超限帧被发出
}
```

`mock_cc_utf8.sh` / `mock_cc_big.sh` 均为逐行 printf 的 bash fixture（big 用 `head -c 9000000 /dev/zero | tr '\0' 'x'` 生成单条 delta 文本）。

- [ ] **Step 3: 运行全部失败 → 修复实现 → 通过**

Run: `cargo test -p bridge-provider`

- [ ] **Step 4: 提交**

```bash
git add crates/adapters/bridge-provider
git commit -m "test(bridge-provider): protocol regression e2e"
```

---

## Self-Review 记录

**Spec 覆盖：** §4 组件（webhook/Engine/CliSession/EventMapper/SseEncoder/SessionStore/RunRegistry/InteractionRegistry/TranscriptSink/CallbackClient）——CallbackClient 属 JSON-callback fallback，v1 恒走 SSE，未单列任务（非目标，spec §3 已注明 SSE always）；其余一一对应 Task 1–14。§5 线协议 → Task 2/3/4/11/12/13/14/16。§6 生命周期 → Task 11/12/14/15。§7 错误 → Task 4/11/14。§8 测试 → Task 16 + 各任务内嵌。

**已知留白（实现期验证，非 placeholder）：** cfuse codex 的 resume 参数与权限请求事件形态（Task 10 前置调研步骤）；AskUserQuestion → ask_user 的字段细节（Task 12，以真实 cfuse cc 输出校准 fixture）。
