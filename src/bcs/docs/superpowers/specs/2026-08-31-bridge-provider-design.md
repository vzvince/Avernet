# Bridge Provider 设计：BCN Provider 2.0 × cfuse(cc/codex) 引擎桥接

日期：2026-08-31
状态：待评审

## 1. 背景与目标

BCS 已支持下行调用模式，并定义了标准的 Provider 2.0 SSE 协议
（`docs/bcs-provider-2.0-sse-protocol.md`）。本设计实现一个 **bridge-provider**：
一个独立的 Rust 服务，作为 BCN Provider 2.0 与 BCS 进行 gateway 通信，
把下游 `chat.send` 等请求桥接到本地编码引擎执行。

引擎支持优先级：

1. **v1（本设计范围）**：cfuse 的 cc 模式（`--cc` / claude-code）与 codex 模式
   （`--codex`）。
2. **后续**：原生 claude（`claude-code-agent-sdk`）与原生 codex。

非本设计的另一部分：Provider/Bot 在 BCS 侧的注册引导（已有 BCS API，本服务
以配置方式消费注册结果）。

## 2. 既有契约（北向固定，不可协商）

北向协议就是 BCN Provider 2.0，权威定义见：

- `src/bcs/docs/bcs-provider-2.0-sse-protocol.md`（SSE 帧、interaction、resolve 回程）
- `docs/bot-provider-integration.md`（仓根；webhook 方法集、token 模型、错误表）

要点摘录（本服务必须满足）：

- BCS `POST <webhook_url>`，`Authorization: Bearer <bcs_to_provider_token>`，
  `X-BCN-Protocol-Version: 2.0`，`X-BCN-Message-Id`（仅追踪）、
  `X-BCN-Timestamp`。
- `chat.send` 且 `Accept: text/event-stream, application/json` 时，应答
  `Content-Type: text/event-stream` 则 run 绑定 SSE；JSON ack 则走
  `/bot/events` callback。一个 run 不可混用两种 transport。
- SSE 帧 `event: agent|chat|ping|interaction`，`data` 为单行紧凑 JSON；
  `data.seq` 在同一 SSE 内跨事件类型单调递增（interaction 必填 seq）；
  `data.runId` 必填（除 ping）但不作 BCS 路由依据；terminal 为
  `chat` 的 `final|error|aborted`。
- 传输约束：生产 HTTP/2；单帧 ≤ 8 MiB；BCS 侧 15 分钟无字节判 idle timeout；
  **BCS 不支持 Last-Event-ID/自动重连/断点续传**；EOF 无 terminal 时 BCS 合成
  `chat/error` 并将 run 置为 terminal。
- `event: agent + stream: approval` 为旧兼容结构，**禁止发送**；HITL 必须走顶层
  `event: interaction`。
- `interaction.resolve` 由 BCS 以独立 POST 打到同一 webhook
  （`X-BCN-Transport: callback`，`Accept: application/json`），ACK 成功为
  `{"ok":true}`；失败为 `{"ok":false,"retryable":bool,"error":"..."}`
  （注意：此方法的 error 是字符串，与通用错误对象不同）。
- 通用错误：`{"ok":false,"error":{"code","message","retryable","retry_after_ms?"}}`，
  code→HTTP 映射见 §5.4。

## 3. 关键决策记录

| 决策点 | 结论 | 理由 / 被否决项 |
| --- | --- | --- |
| 技术栈 | Rust，bcs workspace 内独立 crate + binary | 复用 `bcs-protocol` SSE 类型；与 BCS 侧 `bcs-provider-http` 对应 |
| 总体架构 | 自包含 cfuse-subprocess provider（方案 A） | 不引入对 aix-relay 的运行时依赖；aix-relay 仅作参考实现 |
| A2A/ACP 调研 | 不采用 | ACP 已并入 A2A（Linux Foundation）；二者都不能替代北向（BCS 只讲 Provider 2.0）；A2A 可作为远期南向出口，非 v1 |
| 引擎抽象命名 | trait `Engine` + `enum EngineKind` | 否决：`AgentProvider`（BCN 已占用 Provider=本桥）、`AgentBridge`（bridge=整体组件名）、`AgentBackend`（与本仓 `src/backend` 服务冲突）、`AgentEngine`（agent≈engine 同义重复）、`EngineDriver`（driver=BCS 群组角色）；`bcs-protocol` 已有 `EngineType`（bot 平台类型），故枚举名用 `EngineKind` |
| v1 方法集 | `chat.send`(SSE)、`chat.inject`、`chat.abort`、`interaction.resolve`、`bot.ping` | 用户明确要求交互必须支持；`chat.history` 不做 |
| 交互（HITL） | 必须支持；引擎以交互模式运行 | 不做 yolo/自动批准；v1 支持 `exec` + `ask_user`，`mode_switch` 不合成（协议标记为可选能力，cfuse 无双向模式切换语义） |
| inject 处理 | SQLite 持久队列，下一次 send 按序合并 | CC/Codex 统一路径；删除原生 transcript sink，不读写引擎历史文件 |
| BCS 断连 | **写失败即杀 run**（修正项） | 协议文档 §1.3 明确无重连/续传，EOF 后 BCS 已合成 terminal error，保活无意义。引擎 transcript 仍在，BCS 后续新 run 可 resume |
| 同 session 并发 chat.send | `429 rate_limited`（retryable） | 引擎会话天然串行；v1 不排队 |
| 幂等重挂 | 每 run 内存 frame buffer，重放自 seq 1 + live follow | 仅服务"首个响应丢失后 BCS 同 id 重试"窗口；BCS 按 seq 去重，重放无害 |
| session 标识 | 双 id 模型：`bcs_session_id` ↔ `engine_session_id` | 映射归 session store；Engine 只见 engine session id；BCN id 永不传给 `--resume` |
| SSE 事件类型 | 复用 `bcs_protocol::stream` 类型 | 编译期保证发出帧可被 BCS 解析；不另造 `RuntimeEvent` 式平行类型 |

## 4. 架构与组件

### 4.1 crate 位置

`src/bcs/crates/adapters/bridge-provider/`（library + binary `bridge-provider`）。

它是被 BCS 调用的外部独立进程，不在 BCS inbound `application→core→port`
分层之内；放在 `adapters/` 下与 BCS 侧对应物 `adapters/http/bcs-provider-http`
并列。依赖面保持小：axum、tokio、serde/serde_json、reqwest（仅 callback
fallback 需要）、`bcs-protocol`（SSE 流类型）。

### 4.2 组件

```text
POST /webhook ──→ WebhookServer ──→ Dispatcher(按 method)
                      │                ├─ chat.send ──→ RunRegistry ──→ Engine(经 CliSession)
                      │                ├─ chat.inject ─→ SessionStore (SQLite queue)
                      │                ├─ chat.abort ──→ RunRegistry.abort
                      │                ├─ interaction.resolve ─→ InteractionRegistry
                      │                └─ bot.ping ──→ 引擎可用性探针
                      └─ 校验: token / provider_id / method / 幂等台账
Engine stdout ──→ EventMapper ──→ SseEncoder(seq/buffer) ──→ SSE response
```

1. **WebhookServer**（axum）：唯一入口 `POST /webhook`。校验顺序：
   `Authorization`（401）→ `to_bot.provider_id` 匹配本桥（403）→ method 已知
   （501）→ 幂等台账（同 id 异 body → 409）。`X-BCN-Message-Id` 只记日志。
2. **Engine trait + EngineKind**（模块 `engine`）：

   ```rust
   pub trait Engine: Send + Sync {
       fn kind(&self) -> EngineKind;
       async fn run_turn(&self, req: TurnRequest,
                         events: mpsc::Sender<StreamEvent>,   // bcs_protocol::stream
                         abort: CancellationToken) -> Result<TurnOutcome, TurnError>;
   }
   pub enum EngineKind { CfuseCc, CfuseCodex, /* 后续 */ ClaudeCode, CodexCli }
   ```

   v1 实现 `engine::CfuseCc` / `engine::CfuseCodex`，二者共享具体类型
   **`CliSession`**（tokio 子进程、stdin 喂入、stdout 分帧、kill/abort、
   Unix zombie reap——参考 aix-relay `runtime/claude.rs` 的 waitpid 模式），
   子进程机制不进 trait。
3. **EventMapper**：引擎原生事件 → `bcs_protocol::stream::StreamEvent`。
   cfuse cc：stream-json（`--input-format stream-json --output-format
   stream-json --verbose --include-partial-messages --permission-prompt-tool stdio`），assistant/text 增量 →
   `chat/delta`，tool_use/tool_result → `agent/tool`，`can_use_tool` 控制消息 →
   `interaction/requested(exec)`，AskUserQuestion → `interaction/requested(ask_user)`；
   cfuse codex：Codex app-server JSON-RPC over stdio（
   `item/agentMessage/delta` → `chat/delta`，
   `turn/completed/agent/turn_failed` → terminal；其它 app-server
   notifications 按 thread/turn 关联过滤）。精确字段映射以实现期对真实
   cfuse 输出的契约测试为准。

   共用工具事件构造器对 `phase:result` 的完整输出做通用去壳：仅当输出是
   恰好只含一个 `result` 字段的对象，或解析后满足此条件的 JSON 字符串，
   才用该字段的值替换输出。只去一层，内层值保留原类型，不继续解析字符串
   或递归去壳；不检查工具名称或 BCS 业务字段。带其他字段的对象、普通文本、
   非法 JSON 和内容块数组保持原样，`args` / `partialResult` 不参与转换。
   SSE 协议的外层 `result` 字段保留，可承载任意 JSON 值；原始引擎日志
   仍记录转换前的内容。依赖单字段 `result` 包装的消费方应读取去壳后的值。

   cc 权限控制使用消息顶层的 `request_id` 关联请求与回复；
   `request` 内只读取 `subtype:can_use_tool`、`tool_name` 和 `input`。
   缺少有效 ID、工具名或 input 对象时结束 run 并报告错误，避免产生无法
   回传的待审批记录。回复必须包含 `response.subtype:"success"` 和原始
   `response.request_id`；允许时发送 `behavior:"allow"` 与原始
   `updatedInput`，拒绝时发送 `behavior:"deny"` 与 `message`。
   普通拒绝、取消后的拒绝以及 secret 问题的拒绝共用此回复格式。
   `permission_denials` 和工具失败文本是已结束调用的结果，不合成为待审批事件。
   使用 cfuse 2.6.40 / Claude 2.1.199 实测，此权限通道可直接从 user 消息
   开始，无需为此额外增加 `initialize` 握手。
4. **SseEncoder**：`StreamEvent` → `event:/id:/data:` 文本帧；赋 `seq`
   （per-run 单调，自 1 起，跨 chat/agent/interaction 共享），SSE `id:` 镜像
   `seq`；帧 ≤ 8 MiB；UTF-8 安全切分（`char_indices`，禁止字节切片——
   CLAUDE.md 硬性要求）。
5. **SessionStore**（SQLite 持久状态 + 进程内 run 占用）：键
   `(provider_id, provider_bot_ref, bcs_session_id)`。持久保存引擎类型、规范化
   cwd 与 `engine_session_id`，运行占用仅在内存，重启后不恢复。
   首次捕获 CC `system/init` 或 Codex app-server `thread/start|resume` 的
   session/thread ID 后，通过内部 `SessionObserver` 等待提交成功再继续处理；
   不依赖本轮成功完成。已有会话的 engine/cwd 或原生 session ID 改变时报错，
   不静默覆盖历史映射。数据库打开、迁移和写入错误必须向调用方传播。
6. **RunRegistry**：活跃 run 与 grace 期内 terminal run；每 run 持有
   `buffer: Vec<Frame>`（重放用）、`live_tx: broadcast::Sender<Frame>`、
   `abort: CancellationToken`、`pending_interactions`。
7. **InteractionRegistry**：`interactionId → oneshot::Sender<Resolution>`；
   公开 `interactionId` 由本桥铸造（run 内唯一不复用），引擎内部请求 id 不外泄；
   同时保存 resolve 回写引擎所需的 engine-native 关联信息（对齐协议文档 §11
   "Provider 在 requested 时保存 engine-native correlation"）。
8. **持久 inject 队列**：接收消息与幂等回执在同一事务提交后才 ACK。
   消息在下一次 send 中按接收顺序前置合并，状态为
   `pending → inflight(run_id) → delivered`。失败/取消后回到 pending；
   重启恢复未确认的 inflight，等待下次 send，不自动启动引擎。
   原 `TranscriptSink` 及 CC JSONL 追加逻辑已删除。
9. **CallbackClient**（仅 JSON-ack fallback 路径用）：`POST /bot/events`。
   SSE 绑定的 run 禁止走它（BCS 会 409）。
10. **ProviderConfig**：静态配置（见 §4.3），含 token 与 bot→引擎绑定。

### 4.3 配置形态（示意）

```toml
provider_id = "bridge-provider-1"
bcs_to_provider_token = { env = "BRIDGE_B2P_TOKEN" }
bot_runtime_token = { env = "BRIDGE_BOT_RUNTIME_TOKEN" }  # callback fallback 用
listen = "0.0.0.0:21100"
# state_path = "~/.bcn-bridge/bridge-state.sqlite3"  # 可省略，默认使用用户目录

[[bot]]
provider_bot_ref = "cc-worker"
engine = "cfuse-cc"          # EngineKind
model = "sonnet"             # 可选，-m 透传
cwd = "/data/work/cc"
permission_mode = "default"  # 交互模式；禁止 yolo/bypass

[[bot]]
provider_bot_ref = "codex-worker"
engine = "cfuse-codex"
cwd = "/data/work/codex"
```

`state_path` 可省略，默认 `~/.bcn-bridge/bridge-state.sqlite3`，启动时自动创建目录。
显式 `~/` 路径也展开到运行 Bridge 的用户主目录；普通相对路径仍以配置文件
目录为基准，绝对路径原样使用。无法定位主目录时要求显式配置路径，不回退到
临时目录。数据库与 `.lock`/SQLite sidecar 属于本地运行数据，不提交到仓库；
部署时应使用持久卷，并保留同一运行账号下的引擎原生历史。启动时持有数据库
专用的 OS 文件锁，禁止两个 Bridge 实例同时使用同一个状态库。SQLite 使用
WAL 和 FULL synchronous；日常操作在线程池执行，不阻塞 Tokio 的网络任务。

首次从旧版升级不会自动推断旧内存映射，也不会合并已分散的历史；新建的映射
从此持久化。需要延续升级前某个已知原生会话时，必须先明确对应关系再迁移，
不得按目录里“最新的文件”猜测。

## 5. 线协议面（本桥实现侧）

### 5.1 方法表

| method | transport | 行为 |
| --- | --- | --- |
| `chat.send` | SSE | 解析 bot→engine → 会话映射（resume 或新建）→ spawn → 流式转发 → terminal 关流 |
| `chat.inject` | JSON | 消息和去重回执提交 SQLite 后 ACK，**不触发引擎** → `200 {"ok":true}`；落盘失败 → 503 |
| `chat.abort` | JSON | 按 `session_id` 反查活跃 run → 取消 → 其流上发 `aborted` 终态；响应形态见 §5.3 |
| `interaction.resolve` | JSON | 按 `interactionId` 查 pending → 幂等（同 key 同 resolution 直接成功）→ 回写引擎控制通道 → 引擎应用后在原 SSE 发 `interaction/resolved` → ACK `{"ok":true}`；引擎暂不可写 → `{"ok":false,"retryable":true,"error":"..."}` |
| `bot.ping` | JSON | `200 {"ok":true}` + 引擎 binary 可用性 |

### 5.2 chat.send 发出的帧

- 首帧建议 `agent/lifecycle start`；末两帧 `agent/lifecycle end` + `chat/final`
  （对齐线上真实样本形态，协议文档 §10）。
- `chat/delta`：`{"runId","seq","ts","state":"delta","deltaText":"…"}`。
- `chat/final`：`state:"final"` + `message{role:"assistant",content:[{type:"text",text:<full snapshot>}],timestamp}` + 可选 `stopReason`。
- `chat/error`：`state:"error"` + `errorMessage`（脱敏）+ 可选 `errorKind`。
- `chat/aborted`：`state:"aborted","stopReason":"user_cancelled"`。
- `agent/tool`：`phase:"start|update|result"`，`name/toolCallId` 关联，
  result 携带 `result/isError/exitCode/durationMs/cwd`（有则给）。
- `agent/thinking`：`delta` + 可选累计 `text`。
- `interaction/requested`：`runId/seq/ts/phase/interactionId/kind` 必填，
  字段白名单按协议 §5/§6（exec: command+options[decision,label]；ask_user:
  questions 1–4、questionId/question 必填、自由文本题省略 options 与
  allowOther；secret 类问题**拒绝转换**，不降级明文）。
- `interaction/resolved`：仅在引擎真正应用决议后发送；exec 回显 decision；
  ask_user 最小回显可只带 phase/interactionId/kind。
- 心跳：默认 SSE comment（`: heartbeat`）每 20–30s；需要业务可观测性时才用
  `event: ping`。二者均不含 seq。

### 5.3 chat.abort 响应形态（协议固定）

| 会话状态 | HTTP | Body |
| --- | --- | --- |
| 有 RUNNING/PENDING run | 200 | `{"ok":true,"aborted":true,"aborted_run_ids":["…"]}` |
| 仅 terminal 记录 | 410 | `{"ok":false,"error":{"code":"run_terminated",…}}`（重复 abort 稳定同答） |
| 无记录 | 200 | `{"ok":true,"aborted":false,"aborted_run_ids":[]}` |

### 5.4 错误表（应答前失败）

| code | HTTP | retryable | 场景 |
| --- | --- | --- | --- |
| `invalid_request` | 400 | false | header/body 非法 |
| `unauthorized` | 401 | false | token 错 |
| `provider_id_mismatch` | 403 | false | provider_id 不匹配 |
| `bot_not_found` | 404 | false | provider_bot_ref 未配置 |
| `conflict` | 409 | false | 同幂等键不同 body |
| `run_terminated` | 410 | false | abort 已终结 run |
| `rate_limited` | 429 | true | 同 session 已有活跃 run |
| `unsupported_method` | 501 | false | 未知 method |
| `unavailable` | 503 | true | 引擎 binary 缺失 / spawn 失败 |
| `timeout` | 504 | true | 依赖超时 |

应答后（流已开）的失败一律以 `chat/error` 终态帧表达，不再改 HTTP 状态。

### 5.5 幂等

| 场景 | 键 | 行为 |
| --- | --- | --- |
| `chat.send` | body `id` | 活跃 run 同 body → 迁移/重挂流（buffer 自 seq 1 重放 + live follow）；terminal → 重放终态帧的单帧 SSE；异 body → 409 |
| `chat.inject` | `(provider_id, body.id)` | 完整 message/from 与 bot/session 一致则成功，不重复入队；有差异则 409；回执跨重启保留 |
| `chat.abort` | body `id` | 重复 abort 同 terminal run 稳定 410 |
| `interaction.resolve` | `params.idempotencyKey` | 同键同 resolution → 直接 ACK 成功，不重复回写引擎；引擎侧重复投递需容忍（协议 §8） |

## 6. Run 生命周期与数据流

### 6.1 状态机

```text
Accepted → Starting → Streaming ⇄ AwaitingInteraction → Terminal → Evicted
```

### 6.2 chat.send 主流程

1. 校验 + 幂等认领 → 按 `provider_bot_ref` 取 `EngineKind`/model/cwd →
   查 SessionMapping（有 `engine_session_id` 则 resume，无则新会话）→
   在 SQLite 事务中认领待发 inject，按序合并到当前输入；消息不删除。
2. 立即 `200 + Content-Type: text/event-stream` 应答（远早于 125s 响应头
   deadline），run task 持有该响应流。
3. spawn 引擎（cfuse cc stream-json 或 codex app-server JSON-RPC I/O，交互权限模式）。
4. 从引擎流捕获 engine session id → **立即持久化映射**（run 中途失败也保留下次
   resume 能力）。
5. 引擎事件 → EventMapper → SseEncoder（赋 seq）→ 写 SSE + 入 run buffer。
6. 成功完成时，先提交本轮 inject 为 delivered，再发 final；CC
   `is_error:true` 即使 subtype 为 success 也按失败处理，不能确认消费。
   失败/取消后释放本轮 inject，随后 terminal（final/error/aborted）→ 关流、标记 Terminal、使该 run 全部
   Pending/Accepted interaction 失效、buffer 进 grace TTL（默认 10 min）后驱逐。

投递采用 **至少一次** 语义：如果引擎已接收消息而 Bridge 尚未提交 delivered
就崩溃，下一次 send 可能再次附带这些消息。状态库与引擎之间没有共同事务，
不保证“恰好一次”，也不通过读写原生历史来猜测消费状态。运行期间新收到的
inject 不加入已经认领的批次，留给下一轮；已 delivered 的回执继续用于去重。

### 6.3 Interaction 子流程

1. 引擎发权限/提问请求（cc：`can_use_tool` 控制消息；codex：权限请求事件）。
2. EventMapper 铸造公开 `interactionId`，InteractionRegistry 存 oneshot +
   engine-native 关联，发 `interaction/requested`，run 挂起。
   cfuse cc 的 exec 选项由 Bridge 生成，固定为 `allow-once` / `deny`，
   对齐既有 BaaS→BCN 决策值。回程仅 `allow-once` 映射为引擎 `allow`，
   其余值映射为 `deny`，不提供 `allow_once` 等别名或持久授权。
   BCN 按 requested 中的选项精确校验 decision；旧版 Bridge 已发出的
   `allow_once` 卡片不会随升级变更，需要重新发起任务生成新卡片。
3. BCS 经 InteractionService 路由给 Human；之后独立 POST
   `interaction.resolve` 到本 webhook。
4. 校验 + 幂等 → 触发 resolver → Driver 经引擎控制通道写回决议 → 引擎应用后
   EventMapper 发 `interaction/resolved` → ACK。
5. run deadline 先到：按 kind 安全兜底（exec→deny、ask_user→cancel）写回引擎并
   记录 warning；run 终态时所有未决 interaction 失效（协议 §9）。

### 6.4 abort / deadline / 心跳

- abort：CancellationToken → cc 走控制通道 interrupt（SDK 语义）→ 宽限后
  SIGTERM→SIGKILL → 发 `aborted` 终态。挂起中的 interaction 先按兜底决议释放。
- deadline：以 BCS `timeout_ms` 为上限，bridge 提前 ~30s 自发 `chat/error`
  （`errorKind:"deadline"`）关流，不让 BCS 掐连接。
- 心跳：comment heartbeat 20–30s；不超过 15min idle 与 run deadline。

### 6.5 失败矩阵

| 故障 | 检测 | 行为 |
| --- | --- | --- |
| 引擎崩溃（无结果退出） | stdout EOF / 非零 exit | `chat/error` 终态（脱敏），关流 |
| 引擎僵死无输出 | run deadline | 杀进程 + `chat/error` 终态 |
| BCS 断连（写失败） | SSE write error | **立即杀 run**（协议无重连续传；BCS 已合成 terminal error） |
| 重复 chat.send（同 id 同 body） | 幂等台账 | 见 §5.5（重挂/重放） |
| 同 session 并发第二个 chat.send | SessionMapping.active_run 占用 | `429 rate_limited`（retryable） |
| bridge 进程重启 | — | run/交互/SSE buffer 不恢复；映射和 inject 回执保留，新 send 凭持久映射 `thread/resume` 或 `--resume` 恢复；未确认 inject 重投 |
| interaction.resolve 指向未知 id | 查 registry | `{"ok":false,"retryable":false,"error":"unknown interaction"}` |
| 单帧 > 8 MiB 风险 | encoder 侧检查 | 截断/降级为 error 帧（脱敏），不产生超限帧 |

## 7. 错误处理细则

- **脱敏**：发往 BCS 的 `errorMessage` 不含本地路径/token/命令原文；细节只进
  结构化日志（带 run_id、provider_bot_ref、bcs_session_id）。interaction 业务
  payload（command/questions/answers）不写 INFO/WARN 日志（对齐协议 §9 的日志
  约束）。
- **panic 隔离**：每 run 独立 task，panic 捕获 → `chat/error` 终态，不拖垮
  webhook。
- **优雅退出**：SIGTERM → 停接新 run → 活跃流发 `aborted` 终态 → 杀子进程并
  reap。
- **引擎 stderr**：按行进日志（带关联标签），永不进入协议帧。

## 8. 测试策略（零真实 LLM 调用）

1. **Golden-frame 契约测试**：encoder 输出逐字节对齐协议文档 §10 的真实样本
   形态与 BCS 侧 `bcs-provider-http` 的 fixture；并用
   `bcs_protocol::stream::parse_stream_event` 做往返解析断言。
2. **Mock 引擎二进制**（仿 aix-testkit）：脚本化假 `cfuse`，按剧本吐
   stream-json（含 permission 请求）→ 覆盖 chat.send→final、interaction
   requested→resolve→resolved、abort（含挂起在 interaction 上的 abort）、
   inject、幂等重挂、429 竞争、错误映射表。
3. **Mock BCS 客户端**：测试 client 按 Provider 2.0 发 POST、消费 SSE，
   模拟 BCS 的 seq 去重验证重放安全性。
4. **单元测试**：SseEncoder（多行 data、seq、`id:` 镜像、8 MiB 检查）、
   双 id 映射（首 turn 捕获→后续 resume）、InteractionRegistry
   （resolve/超时兜底/abort 竞态/幂等键）、幂等台账。
5. **UTF-8 专项**：中文 delta 跨帧切分必须走 `char_indices` 安全边界。
6. **持久化恢复**：真实 Bridge 进程 + SQLite + 本地 mock 引擎，覆盖成功/失败/取消/中断后的 resume、inject FIFO/去重/重投，以及写入错误和单实例锁。
7. **轻依赖**：`cargo test -p bridge-provider` 可在本 worktree 独立构建
   （磁盘受限，不做全 workspace 构建）。

## 9. 非目标与未来工作

- 原生 claude（claude-code-agent-sdk）/ 原生 codex：`EngineKind` 已预留
  `ClaudeCode`/`CodexCli`，v2 实现。
- `chat.history`：不做。
- `mode_switch` interaction：不合成（协议允许 Provider 不声明该能力）。
- A2A 出口层（让非 BCS 网络调用本桥引擎）：远期可选。
- chat.send 排队（替代 429）、运行/交互/SSE buffer 的跨进程恢复：按运行需要再做。
- inject 回执清理策略：当前保留 delivered 回执确保跨重启去重，需部署侧关注状态库增长。
- cfuse `proxy` 模式（HTTP 常驻）替代 per-turn 子进程：实现期若并发/冷启动
  成为瓶颈再评估。

## 10. 参考资料

- `src/bcs/docs/bcs-provider-2.0-sse-protocol.md` — Provider 2.0 SSE 权威协议
- `docs/bot-provider-integration.md`（仓根）— Provider 集成契约（token/方法/错误）
- `src/bcs/crates/adapters/http/bcs-provider-http/` — BCS 侧传输实现与合约测试
- aix-engine-workspace `crates/relay/` — 引擎驱动参考（`runtime/mod.rs` 的
  provider 抽象与 `RuntimeEvent`、`sessions/inject.rs` 的 inject/transcript
  模式、`interactions/` 的 pending/resolve 模式、`runtime/claude.rs` 的
  SDK interrupt/zombie reap）。仅参考，不产生依赖。
- `src/baas/docs/2026-08-19-baas-bcn-interaction-sse-design.md` — 引擎事件 →
  BCN interaction 的白名单转换与容错参考


## Upstream V2 extension (2026-09-18)

Gateway remains the default mode and retains this HTTP/SSE wire contract. Shared
runtime buffers now contain structured events; GatewayEncoder formats SSE at
delivery time. The new optional upstream mode uses outgoing Bot WebSocket V2
connections and its own encoder. See [upstream V2 design and configuration](2026-09-18-bridge-upstream-v2-design.md) for disconnect handling, durable reconnect
credentials and V2 authorization limits. Local SQLite schema v2 preserves prior
session mappings and inject receipts but requires a backup or separate state path
when rolling back to the v1 binary.
