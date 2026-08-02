# BCN Response 格式与 Error Code 规范

更新时间：2026-07-06

本文定义 BCN API 的目标返回格式、错误码分配规则、以及按模块划分的错误码与错误消息。该规范面向 BCN REST API 的新输出格式，用来替换当前 BCS 代码中多种历史返回格式。

参考来源：

- Backend API 统一响应定义：`src/backend/docs/api/openapi/_shared.yaml`
- Backend Error Code 规则：`src/backend/docs/api/overview/error-codes.md`
- Backend API 文档规划：`src/backend/docs/api/DOC-PLAN.md`
- BCS 当前实现：`src/bcs/crates/service-api`、`src/bcs/crates/adapters/http/bcs-http`
- TeamClaw API Server Yuque 文档：按 API Server 的统一返回与错误语义方向对齐；本文中的具体 BCS error code 以当前代码实现为准

## 1. 适用范围

本规范适用于 BCN REST API：

- 对外 OpenAPI：例如 `/openapi/bcn/v1/...`
- 内部 REST API：如果后续仍保留 internal API，也应使用相同 response envelope

不直接适用于：

- WebSocket 帧协议，例如 `/ws/bot`、Workbench 前端 WS。WebSocket 仍保持 `req/res/event` 帧结构，但帧内错误建议复用本文的 `code` 与 `message`。
- Provider 下行 SSE 的事件帧。SSE 事件内如包含错误，也建议复用本文的 `code` 与 `message`。

## 2. 统一 Response Envelope

所有 REST API 返回 JSON envelope：

```json
{
  "code": 200000,
  "message": "OK",
  "data": {},
  "request_id": "req_7f3b8c2a"
}
```

字段定义：

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `code` | integer | 是 | 6 位业务码。前三位与 HTTP status 保持一致，后三位为 BCN 子码。 |
| `message` | string | 是 | 稳定、可读的英文消息。成功时通常为 `OK`；失败时为标准错误消息。 |
| `data` | object / array / string / number / boolean / null | 否 | 成功时为接口实际数据；失败时固定为 `null`。 |
| `request_id` | string | 是 | 请求追踪 ID，应与响应头 `X-Trace-Id` 一致。若上游传入可接受的 request id，可透传或派生。 |

约束：

- HTTP status 必须与 `code` 的前三位一致，例如 HTTP 404 对应 `404xxx`。
- 失败响应中不再返回历史字段 `success`、`error`、`status`、`params`、`error_code`。
- `message` 不承载调试堆栈或内部依赖细节；内部详情进入日志，并通过 `request_id` 关联。
- 参数化错误可在 `message` 中保留安全对象标识，例如 `bot not found: bot_abc`；不返回 token、secret、cookie、内部连接串。

## 3. 成功返回

通用成功码：

| Code | HTTP | Message | 使用场景 |
| --- | --- | --- | --- |
| `200000` | 200 | `OK` | 查询、普通同步操作成功 |
| `201000` | 201 | `OK` | 创建资源成功 |
| `202000` | 202 | `Accepted` | 异步任务已接受，未区分模块时使用 |
| `204000` | 204 | `No Content` | 无响应体的删除/取消类操作；如 API Server 统一要求有 body，则改用 HTTP 200 + `200000` + `data: null` |

BCN 模块化成功码：

| Code | HTTP | Message | 使用场景 |
| --- | --- | --- | --- |
| `202101` | 202 | `chat run accepted` | `POST /bots/{id}/chat-async` 创建异步 chat run |
| `202501` | 202 | `state machine run accepted` | 状态机/协作运行启动成功 |

分页响应建议统一为：

```json
{
  "code": 200000,
  "message": "OK",
  "data": {
    "items": [],
    "total": 0,
    "limit": 20,
    "offset": 0
  },
  "request_id": "req_7f3b8c2a"
}
```

异步任务创建响应示例：

```json
{
  "code": 202101,
  "message": "chat run accepted",
  "data": {
    "run_id": "run_abc",
    "bot_uuid": "bot_abc",
    "session_id": "session_abc",
    "status": "queued",
    "expires_at_ms": 1783339200000
  },
  "request_id": "req_7f3b8c2a"
}
```

错误响应示例：

```json
{
  "code": 404101,
  "message": "bot not found",
  "data": null,
  "request_id": "req_7f3b8c2a"
}
```

## 4. Error Code 分配规则

`code` 为 6 位整数：

```text
<HTTP status 3 digits><BCN subcode 3 digits>
```

前三位：

- `400`：请求参数、状态、格式错误
- `401`：未认证或 token/cookie 无效
- `403`：已认证但无权限
- `404`：资源不存在
- `409`：资源状态冲突、重复绑定、重复请求
- `410`：资源曾存在但已终止/过期
- `429`：限流或容量限制
- `500`：服务端内部错误
- `503`：依赖或目标服务暂不可用

后三位按模块分配：

| 子码范围 | 模块 |
| --- | --- |
| `000` | 通用默认码 |
| `100-149` | Bot 管理、Bot 目录、Bot A2A |
| `150-179` | 好友关系、Actor 目录 |
| `200-249` | 群组管理、群组提案、群组内部控制 |
| `300-349` | Session、session 服务调用、chat run 查询/取消 |
| `400-449` | Provider 管理、Provider 内部/灰度/回调 |
| `500-549` | 协作模板、状态机运行 |
| `600-649` | 邀请、注册、OAuth |
| `900-949` | 系统/运维、Secret、通用依赖错误 |

## 5. 通用错误码

| Code | HTTP | Message | 当前代码来源/说明 |
| --- | --- | --- | --- |
| `400000` | 400 | `invalid request` | 通用 Bad Request；当前多处为 `InvalidOperation` 或 route-level bad request |
| `401000` | 401 | `authentication required` | 通用未认证；对应 human cookie、agent token、provider token、bot runtime token 缺失或无效 |
| `403000` | 403 | `permission denied` | 通用无权限；对应 `Forbidden` |
| `404000` | 404 | `not found` | 通用资源不存在 |
| `409000` | 409 | `resource conflict` | 通用冲突 |
| `410000` | 410 | `resource gone` | 资源已过期或已终止 |
| `429000` | 429 | `quota exceeded` | 通用限流/容量限制 |
| `500000` | 500 | `internal error` | 通用内部错误 |
| `503000` | 503 | `service unavailable` | 通用依赖不可用 |

## 6. Bot 管理、Bot 目录、Bot A2A

当前主要来源：

- `ServiceError::{BotNotFound, BotNotRegistered, BotNotConnected, BotHidden, MessageLimitReached}`
- `BotUseCaseError::{Unauthorized, Forbidden, InvalidVisibility, InvalidBotId, InvalidProviderBotRef, ProviderNotFound, ProviderNotReadyForDownlink, BotAlreadyBound, Connect}`
- `ConnectError::{AlreadyConnected, AlreadyRegistered, InvalidBotId, InvalidToken}`
- HTTP 映射：`bcs-http/src/error.rs`、`routes/bots.rs`

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400101` | 400 | `invalid bot id` | bot id 为空或格式非法；`InvalidBotId` |
| `400102` | 400 | `invalid bot visibility` | visibility 不是 `public`、`protected`、`private` |
| `400103` | 400 | `invalid provider bot ref` | provider bot ref 为空或格式非法 |
| `400104` | 400 | `invalid bot request` | Bot 请求体格式或语义不合法 |
| `401101` | 401 | `valid bot token is required` | bot runtime token 缺失或无效；`ConnectError::InvalidToken` |
| `401102` | 401 | `bot caller identity required` | Bot 目录/API 需要 human cookie 或 agent identity，但当前请求未通过身份解析 |
| `403101` | 403 | `bot access denied` | 当前身份无权访问或修改该 bot |
| `403102` | 403 | `bot is not collaborative` | bot 被隐藏或不可协作；`BotHidden` |
| `404101` | 404 | `bot not found` | `BotNotFound` |
| `404102` | 404 | `bot not registered` | `BotNotRegistered` |
| `404103` | 404 | `bot group binding not found` | bot 关联 group 查询为空且语义要求存在绑定 |
| `409101` | 409 | `bot already connected` | `ConnectError::AlreadyConnected` |
| `409102` | 409 | `bot already registered` | `ConnectError::AlreadyRegistered` |
| `409103` | 409 | `bot already bound to provider` | `BotAlreadyBound` |
| `409104` | 409 | `provider downlink not ready` | `ProviderNotReadyForDownlink` |
| `429101` | 429 | `message limit reached` | `MessageLimitReached` |
| `503101` | 503 | `bot is not connected` | `BotNotConnected` |
| `500101` | 500 | `bot operation failed` | Bot 管理内部错误兜底 |

## 7. 好友关系、Actor 目录

当前主要来源：

- `ServiceError::{CannotAddSelf, PendingRequestExists, CannotAcceptRejected, CannotRejectAccepted, NotFriends, FriendRequestNotFound, PrivateBotCannotCollaborate, ParticipantNotFound}`
- `FriendUseCaseError::{Forbidden, Service}`
- Actor routes 中的 no-auth/actor service error 映射

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400151` | 400 | `cannot add yourself as friend` | `CannotAddSelf` |
| `400152` | 400 | `invalid friend request` | 好友请求参数不合法 |
| `401151` | 401 | `actor caller identity required` | Actor/Friend API 需要 human cookie 或 agent identity |
| `403151` | 403 | `not friends` | `NotFriends` |
| `403152` | 403 | `private bot cannot collaborate` | `PrivateBotCannotCollaborate` |
| `403153` | 403 | `actor access denied` | Actor 访问无权限 |
| `404151` | 404 | `friend request not found` | `FriendRequestNotFound` |
| `404152` | 404 | `actor not found` | Actor 目录资源不存在；当前实现部分复用 `BotNotFound` |
| `404153` | 404 | `participant not found` | `ParticipantNotFound` |
| `409151` | 409 | `pending friend request already exists` | `PendingRequestExists` |
| `409152` | 409 | `friend request state conflict` | `CannotAcceptRejected`、`CannotRejectAccepted`、好友状态冲突 |
| `500151` | 500 | `friend operation failed` | 好友关系内部错误兜底 |
| `500152` | 500 | `actor operation failed` | Actor 目录内部错误兜底 |

## 8. 群组管理、群组提案、群组内部控制

当前主要来源：

- `ServiceError::{GroupNotFound, ProposalNotFound, ParticipantNotFound, ExistNonPublicBots}`
- `GroupUseCaseError::{Unauthorized, Forbidden, InvalidGroupId, InvalidGroupStatus, InvalidProposal, ProposalNotFound, ProposalExpired, InvalidHistoryLimit, ActorNotFound, InvalidParticipantMode, Conflict, Service}`
- HTTP 映射：`routes/groups.rs`

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400201` | 400 | `invalid group id` | `InvalidGroupId` |
| `400202` | 400 | `invalid group status` | `InvalidGroupStatus` |
| `400203` | 400 | `invalid proposal` | `InvalidProposal` |
| `400204` | 400 | `invalid history limit` | `InvalidHistoryLimit` |
| `400205` | 400 | `invalid participant mode` | `InvalidParticipantMode` |
| `400206` | 400 | `group contains non-public bots` | `ExistNonPublicBots` |
| `400207` | 400 | `invalid group request` | 群组请求体格式或语义不合法 |
| `401201` | 401 | `group caller identity required` | 群组 API 需要 human cookie 或 agent identity |
| `403201` | 403 | `group access denied` | `Forbidden` |
| `404201` | 404 | `group not found` | `GroupNotFound` |
| `404202` | 404 | `participant not found` | `ParticipantNotFound`、`ActorNotFound` |
| `404203` | 404 | `proposal not found` | `ProposalNotFound` |
| `410201` | 410 | `proposal expired` | `ProposalExpired`；当前代码映射为 404，目标建议使用 410 |
| `409201` | 409 | `group state conflict` | `Conflict` |
| `500201` | 500 | `group operation failed` | 群组管理内部错误兜底 |

## 9. Session、session 服务调用、Chat Run

当前主要来源：

- `ServiceError::{SessionNotFound, SessionInvalidParams, SessionCallbackPending}`
- `SessionUseCaseError::{NotFound, InvalidParams, CallbackPending, Conflict, Internal}`
- HTTP 映射：`routes/sessions.rs`、`bcs-http/src/error.rs`
- Chat run 查询/取消接口：`GET /chat/runs/{run_id}`、`POST /chat/runs/{run_id}/cancel`

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400301` | 400 | `invalid session params` | `SessionInvalidParams`、`SessionUseCaseError::InvalidParams` |
| `400302` | 400 | `invalid service invocation request` | session 服务调用请求格式或参数不合法 |
| `401301` | 401 | `session caller identity required` | Session API 需要 human cookie 或 agent identity |
| `403301` | 403 | `session access denied` | 当前身份无权访问 session |
| `404301` | 404 | `session not found` | `SessionNotFound`、`SessionUseCaseError::NotFound` |
| `404302` | 404 | `chat run not found` | chat run 查询/取消目标不存在 |
| `409301` | 409 | `session callback pending` | `SessionCallbackPending`；当前 `routes/sessions.rs` 对直接 use case 映射为 400，目标建议统一为 409 |
| `409302` | 409 | `session conflict` | `SessionUseCaseError::Conflict` |
| `409303` | 409 | `chat run conflict` | chat run 已完成、已取消或状态不可变更 |
| `429301` | 429 | `chat run limit reached` | chat run 队列或容量限制 |
| `500301` | 500 | `session operation failed` | Session 内部错误兜底 |
| `500302` | 500 | `service invocation failed` | session 服务调用内部错误兜底 |

## 10. Provider 管理、Provider 内部/灰度/回调

当前主要来源：

- `ServiceError::{ProviderNotFound, ProviderNotReadyForDownlink, BotAlreadyBound, BotNotFound, BotNotRegistered}`
- `ProviderBotEventError::{Unauthorized, Forbidden, InvalidRequest, RunNotFound, RunTerminated, BotNotFound, Internal}`
- Provider routes 中的 `ProviderRouteError`、`BotEventRouteError`
- HTTP 映射：`routes/providers.rs`、`routes/bot_events.rs`

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400401` | 400 | `invalid provider request` | provider 请求体或参数不合法 |
| `400402` | 400 | `provider header is required` | provider 身份 header 缺失 |
| `400403` | 400 | `invalid provider bot ref` | provider bot ref 为空或格式非法 |
| `400404` | 400 | `auth mode mismatch` | provider event 的 auth mode 与 bot 绑定不匹配；当前 bot event route 使用字符串码 `auth_mode_mismatch` |
| `401401` | 401 | `valid provider token is required` | provider admin/runtime token 缺失或无效 |
| `401402` | 401 | `valid bot runtime token is required` | provider callback 或 bot event 需要 bot runtime token |
| `403401` | 403 | `provider id mismatch` | header/path/provider claim 不一致；当前 bot event route 使用字符串码 `provider_id_mismatch` |
| `403402` | 403 | `provider access denied` | `ProviderBotEventError::Forbidden` |
| `404401` | 404 | `provider not found` | `ProviderNotFound` |
| `404402` | 404 | `provider bot not found` | provider 侧 bot 映射不存在；当前部分实现返回 `bot not found: {bot_id}` |
| `404403` | 404 | `provider run not found` | `ProviderBotEventError::RunNotFound` |
| `409401` | 409 | `provider downlink not ready` | `ProviderNotReadyForDownlink` |
| `409402` | 409 | `bot already bound to provider` | `BotAlreadyBound` |
| `410401` | 410 | `provider run terminated` | `ProviderBotEventError::RunTerminated` |
| `500401` | 500 | `provider operation failed` | Provider 管理内部错误兜底 |
| `500402` | 500 | `provider callback failed` | Provider 回调处理内部错误兜底 |

## 11. 协作模板、状态机运行

当前主要来源：

- `CollaborationTemplateError::{NotFound, LanguageNotAvailable, InvalidFormat, InvalidTags, InvalidLanguage, RegistryInvalid, YamlInvalid, Io}`
- `CollaborationRuntimeError::{RunNotFound, DefinitionNotFound, InvalidDefinition, InvalidParticipantBinding, InvalidRequest, Conflict, Internal}`
- HTTP 映射：`routes/templates.rs`、`routes/collaboration_runs.rs`、`routes/groups.rs`

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400501` | 400 | `invalid template format` | `InvalidFormat`；当前字符串码 `INVALID_TEMPLATE_FORMAT` |
| `400502` | 400 | `invalid template tags` | `InvalidTags` |
| `400503` | 400 | `invalid template language` | `InvalidLanguage` |
| `400511` | 400 | `invalid collaboration definition` | `InvalidDefinition` |
| `400512` | 400 | `invalid participant binding` | `InvalidParticipantBinding` |
| `400513` | 400 | `invalid collaboration runtime request` | `InvalidRequest` |
| `401501` | 401 | `collaboration caller identity required` | 协作模板/运行 API 需要 human cookie 或 agent identity |
| `403501` | 403 | `collaboration access denied` | 当前身份无权访问模板或运行 |
| `404501` | 404 | `template not found` | `NotFound`；当前字符串码 `TEMPLATE_NOT_FOUND` |
| `404502` | 404 | `template language not available` | `LanguageNotAvailable` |
| `404511` | 404 | `collaboration definition not found` | `DefinitionNotFound` |
| `404512` | 404 | `state machine run not found` | `RunNotFound` |
| `409511` | 409 | `state machine run conflict` | `Conflict` |
| `500501` | 500 | `template registry invalid` | `RegistryInvalid` |
| `500502` | 500 | `template yaml invalid` | `YamlInvalid` |
| `500503` | 500 | `template io error` | `Io` |
| `500511` | 500 | `collaboration runtime failed` | 状态机运行内部错误兜底 |

## 12. 邀请、注册、OAuth

当前主要来源：

- `InviteUseCaseError::{InvalidToken, Expired, LoginRequired, Forbidden, NotFound, Conflict, Service}`
- onboard/register 相关 route-level error
- OAuth 相关接口目前应先复用通用身份错误，后续如有独立 use case error 再扩展子码

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400601` | 400 | `invalid registration request` | bot onboard/register 请求体或 URL 参数不合法 |
| `400602` | 400 | `botchat url is not configured` | onboard URL 依赖配置缺失 |
| `400621` | 400 | `invalid oauth request` | OAuth 参数不合法 |
| `401601` | 401 | `invalid invite token` | `InviteUseCaseError::InvalidToken` |
| `401602` | 401 | `login required` | `InviteUseCaseError::LoginRequired` |
| `401621` | 401 | `oauth authentication required` | OAuth 会话或授权凭据缺失 |
| `403601` | 403 | `invite access denied` | `InviteUseCaseError::Forbidden` |
| `404601` | 404 | `invite target not found` | `InviteUseCaseError::NotFound` |
| `409601` | 409 | `invite conflict` | `InviteUseCaseError::Conflict` |
| `410601` | 410 | `invite link has expired` | `InviteUseCaseError::Expired`；当前 route 已返回 410 |
| `500601` | 500 | `registration failed` | 注册/入网内部错误兜底 |
| `500602` | 500 | `invite operation failed` | 邀请内部错误兜底 |
| `500621` | 500 | `oauth operation failed` | OAuth 内部错误兜底 |

## 13. 系统/运维、Secret、依赖错误

当前主要来源：

- health/metrics 等系统接口
- `SecretServiceError::{NotFound, Unavailable, InvalidInput}`
- 通用 IO/JSON 错误：`ServiceError::{IoError, JsonError, InternalError}`

| Code | HTTP | Message | 适用错误/场景 |
| --- | --- | --- | --- |
| `400901` | 400 | `invalid secret input` | `SecretServiceError::InvalidInput` |
| `400902` | 400 | `invalid system request` | 系统/运维接口参数不合法 |
| `403901` | 403 | `loopback access required` | 仅允许本机或内部控制面访问的接口 |
| `404901` | 404 | `secret not found` | `SecretServiceError::NotFound` |
| `500901` | 500 | `system operation failed` | 系统/运维内部错误兜底 |
| `500902` | 500 | `json serialization failed` | `ServiceError::JsonError` |
| `500903` | 500 | `io operation failed` | `ServiceError::IoError` |
| `503901` | 503 | `secret backend unavailable` | `SecretServiceError::Unavailable` |
| `503902` | 503 | `dependency unavailable` | 外部依赖不可用 |

## 14. 当前实现需要迁移的历史格式

BCS 当前代码中存在多种历史错误响应格式。新规范落地时应统一为本文 envelope。

| 当前位置 | 当前格式 | 目标格式 |
| --- | --- | --- |
| `bcs-http/src/error.rs` | `{ "status": 404, "code": "BOT_NOT_FOUND", "params": {}, "message": "...", "error": "..." }` | `{ "code": 404101, "message": "bot not found", "data": null, "request_id": "..." }` |
| `routes/bots.rs` visibility error | `{ "success": false, "error": "..." }` | 统一 envelope |
| `routes/friends.rs` | `{ "success": false, "error": "..." }` | 统一 envelope |
| `routes/sessions.rs` | `{ "error": "..." }` | 统一 envelope |
| `routes/providers.rs` | `{ "error": "...", "status": 400 }` | 统一 envelope |
| `routes/bot_events.rs` | `{ "error": "invalid_request", "message": "...", "status": 400 }` | 统一 envelope |
| `routes/templates.rs` | `{ "error": { "code": "TEMPLATE_NOT_FOUND", "message": "..." } }` | 统一 envelope |
| `routes/collaboration_runs.rs` | `{ "error": "not_found", "message": "..." }` | 统一 envelope |

迁移建议：

1. 在 HTTP adapter 层提供统一 response helper，例如 `ApiResponse<T>` 与 `ApiErrorResponse`。
2. 所有 route-level error 先映射到内部标准错误枚举，再由统一 mapper 输出 `code/message/status`。
3. 保持日志中的内部错误详情不变，但对客户端只输出标准 `message` 与 `request_id`。
4. 对当前已确认要 deprecated 的接口，仍可先套统一 envelope，避免客户端同时处理两套错误格式。

## 15. Message 规范

错误 `message` 使用英文、稳定短句：

- 使用小写短语，除非包含专有名词。
- 不以句号结尾。
- 不把参数 schema 细节放进 message；详细校验错误可后续放入 `data.validation_errors`，但失败响应默认 `data: null`，需 API Server 规范确认后再扩展。
- 不输出敏感信息，包括 token、cookie、secret、内部 URL、DB key。

推荐消息格式：

| 场景 | 推荐 |
| --- | --- |
| 资源不存在 | `{resource} not found` |
| 权限不足 | `{resource} access denied` |
| 参数非法 | `invalid {resource} request` 或 `invalid {field}` |
| 状态冲突 | `{resource} conflict` 或 `{resource} state conflict` |
| 异步任务终止 | `{resource} terminated` |
| 依赖不可用 | `{dependency} unavailable` |

