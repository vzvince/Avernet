# BCS 接口分类

更新时间：2026-07-06

本文档基于 BCS 当前代码中实际挂载的 HTTP 路由和 WebSocket 入口整理，用于判断哪些接口适合作为 OpenAPI 对外暴露，哪些接口应保留为 BCN/BCS 内部协议。

主要代码来源：

- HTTP API 路由：`crates/adapters/http/bcs-http/src/router.rs`
- WebSocket、metrics、OAuth 挂载：`crates/bootstrap/bcs/src/server.rs`
- OAuth 子路由：`crates/adapters/http/bcs-http/src/oauth/mod.rs`
- Frontend WS 帧方法：`crates/adapters/ws/bcs-ws/src/web/dispatcher.rs`
- Bot WS 帧方法：`crates/adapters/ws/bcs-ws/src/bot/dispatcher.rs`

## 分类原则

| 分类 | 判定 |
|------|------|
| 可作为 OpenAPI 暴露 | 面向前端、三方应用、群组管理、成员管理、Session、邀请、服务调用、目录查询等稳定业务能力 |
| 不作为 OpenAPI 暴露 | 系统运维、内部健康检查、metrics、密钥、Bot runtime 通信、provider 回调、OAuth 内部流程、灰度开关、状态机内部运行、Bot-to-BCS 内部通道 |
| 条件挂载 | `/metrics` 仅在 metrics 配置启用时挂载；`/auth/*` 仅在 OAuth provider 且 `jwt_secret` 配置有效时挂载 |

## 路径作用域与前缀

本文档表格中的路径默认是 BCS 服务当前实现的原始路径，例如 `/bots`、`/groups/{id}`。对外或内部文档生成时按分类结果套用 BCN canonical 前缀：

| 作用域 | Canonical 前缀 | 适用范围 | 示例 |
|--------|----------------|----------|------|
| BCN OpenAPI REST | `/openapi/bcn/v1` | “可作为 OpenAPI 暴露”的稳定 REST 能力 | `GET /bots` -> `GET /openapi/bcn/v1/bots` |
| BCN OpenAPI WebSocket | `/openapi/bcn/v1/ws` | 需要对外说明的前端实时通道 | `GET /ws` -> `WS /openapi/bcn/v1/ws` |
| BCN Internal REST | `/api/bcn/v1` | “不作为 OpenAPI 暴露”的内部 HTTP 能力 | `POST /bot/events` -> `POST /api/bcn/v1/bot/events` |
| BCN Internal WebSocket | `/api/bcn/v1/ws/*` | Bot Runtime 或内部实时协议 | `GET /ws/bot` -> `WS /api/bcn/v1/ws/bot` |

兼容路径不作为 canonical：Workbench 的 `/bcnproxy/...`、Backend Gateway 的 `/api/v1/engine/...` 和 `/api/v1/admin/...`、以及 BCS 裸路径 `/bots`、`/groups` 等只作为当前实现/代理入口。新的外部 API 文档统一使用 `/openapi/bcn/v1/...`，新的内部 API 文档统一使用 `/api/bcn/v1/...`。

## 与原接口列表的差异

原接口列表基本覆盖了 BCS 当前挂载的 API，但代码里还实际注册了以下接口：

| 分类 | Method | Path | 说明 | OpenAPI |
|------|--------|------|------|---------|
| Provider 内部/灰度 | GET | `/providers/stream-gray` | 查询 provider stream 灰度名单 | 不暴露 |
| Provider 内部/灰度 | PUT | `/providers/stream-gray` | 更新 provider stream 灰度名单 | 不暴露 |
| Provider 内部/回调 | POST | `/bot/events/coordination` | provider 上报 bot 协作/工具调用事件 | deprecated |

## 总览

| 分类 | 接口范围 | OpenAPI 建议 |
|------|----------|--------------|
| 系统/运维 | `/health`、`/metrics`、`/manifest`、`/admin/secret/{name}` | 不暴露 |
| 前端 WS / Workbench | `/ws`，帧方法：`connect`、`chat.send`、`chat.abort` | 可暴露 `/ws` 协议 |
| Bot Runtime 内部通信 | `/ws/bot`，帧方法：`bot.connect`、`bot.status`、`task.dispatch`、`task.message`、`route.resolve`、`task.complete`、`session.complete` | 不暴露 |
| 身份/登录/OAuth | `/me`、`/me/repair-info`、`/me/ensure-human`、`/auth/*`、`/onboard/url`、`/register/token`、`/register` | 多数不暴露 |
| Bot 目录查询 | bot 查询、我的 bot、分页、批量查询、详情、好友、群组、可见性 | 可暴露 |
| Bot 内部生命周期/消息 | bot connect、discover、onboard、admin onboard、status、legacy blocking chat | 不暴露；connect 待删除，status 待废弃，blocking chat 待废弃 |
| 单 Bot 异步调用 | `/bots/{id}/chat-async`、`/chat/runs/{run_id}`、`/chat/runs/{run_id}/cancel` | 单独成组；当前不暴露，若对外提供单 bot invocation 则三件套一起暴露 |
| Provider 管理 | provider 查询/更新、provider bot 列表/注册/删除 | 可作为 Provider/Admin OpenAPI |
| Provider 内部/灰度/回调 | provider 注册、agentpass 解析、灰度、启停、投递切换、bot event 回调 | 不暴露 |
| Actor 目录 | actor 列表、搜索、状态更新 | 可暴露 |
| 好友关系 | 好友申请、申请列表、接受、拒绝 | 可暴露 |
| 群组管理 | 群组 CRUD、成员、可见性、设置、参与者模式 | 可暴露 |
| 群组内部控制 | 协作定义、路由策略、群状态、终止、标签、workspace | 不暴露或仅内部管理 |
| 群消息/回调 | 群聊消息、callback、历史消息、fuse | 不暴露；整类废弃，待删除 |
| 群组提案/确认页 | group request、confirm HTML/提交 | 不暴露 |
| 协作模板 | collaboration templates | 可暴露 |
| 状态机运行 | state-machine-runs 相关接口 | 暂不暴露 |
| Session | session 创建、列表、详情、更新、删除、完成、成员、聊天、消息 | 可暴露 |
| 服务调用 | service session 创建、查询 | 可暴露 |
| 邀请 | group/session invite link、join | 可暴露 |

## 完整分类清单

### 系统/运维

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/health` | 健康检查 | 不暴露 |
| GET | `/metrics` | Prometheus 指标，路径可配置，启用时挂载 | 不暴露 |
| GET | `/manifest` | 前端 bundle manifest | 不暴露 |
| GET | `/admin/secret/{name}` | 拉取密钥，loopback/admin 内部能力 | 不暴露 |

### 前端 WS / Workbench

| Method | Path / WS Method | 说明 | OpenAPI |
|--------|------------------|------|---------|
| GET | `/ws` | 前端客户端 WebSocket | 可暴露 |
| WS | `connect` | 连接并订阅 group/session | 可暴露 |
| WS | `chat.send` | 前端向 group/session 发送消息 | 可暴露 |
| WS | `chat.abort` | 取消前端发起的 chat run | 可暴露 |

### Bot Runtime 内部通信

| Method | Path / WS Method | 说明 | OpenAPI |
|--------|------------------|------|---------|
| GET | `/ws/bot` | Bot WebSocket 连接入口 | 不暴露 |
| WS | `bot.connect` | Bot 连接握手 | 不暴露 |
| WS | `bot.status` | Bot 心跳/状态更新 | 不暴露 |
| WS | `task.dispatch` | manager bot 派发任务给 worker bot | 不暴露 |
| WS | `task.message` | worker bot 回传任务消息 | 不暴露 |
| WS | `route.resolve` | Bot 解析路由目标 | 不暴露 |
| WS | `task.complete` | Bot 完成任务 | 不暴露 |
| WS | `session.complete` | Bot 完成 session | 不暴露 |
| WS | `chat.send` | Bot 侧当前返回未实现 | 不暴露 |
| WS | `chat.abort` | Bot 侧当前返回未实现 | 不暴露 |

### 身份/登录/OAuth/Register

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/me` | 当前用户身份 | 不暴露 |
| GET | `/me/repair-info` | 身份修复信息 | 不暴露 |
| POST | `/me/ensure-human` | 确保人类 actor 与 bot 绑定 | 不暴露 |
| GET | `/auth/url` | 获取授权 URL，条件挂载 | 不暴露 |
| GET | `/auth/callback/{provider}` | OAuth 回调，条件挂载 | 不暴露 |
| POST | `/auth/logout` | 登出，条件挂载 | 不暴露 |
| POST | `/auth/refresh` | 刷新 session，条件挂载 | 不暴露 |
| GET | `/auth/user` | 当前 OAuth 用户，条件挂载 | 不暴露 |
| GET | `/auth/user/{user_id}` | 指定 OAuth 用户，条件挂载 | 不暴露 |
| GET | `/onboard/url` | 生成 onboard URL | deprecated |
| GET | `/register/token` | 获取人工注册 token | 不暴露 |
| POST | `/register` | 人工注册 bot | 不暴露 |

### Bot 目录查询

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/bots` | 列出所有 bot | 可暴露 |
| GET | `/bots/my` | 我的 bot 列表 | 可暴露 |
| GET | `/bots/paged` | 分页列出 bot；建议并入 `GET /bots` | deprecated |
| POST | `/bots/query` | 批量查询 bot | 可暴露 |
| GET | `/bots/{id}` | 获取 bot 详情 | 可暴露 |
| DELETE | `/bots/{id}` | Bot 退出网络 | 可暴露 |
| GET | `/bots/{id}/friends` | Bot 好友列表 | 可暴露 |
| GET | `/bots/{id}/groups` | Bot 所属群组 | 可暴露 |
| GET | `/bots/{id}/visibility` | 查询 bot 可见性 | 可暴露 |
| PUT | `/bots/{id}/visibility` | 设置 bot 可见性 | 可暴露 |

### Bot 目录管理详细接口

本节展开“Bot 目录查询/管理”相关接口的鉴权、入参和出参。这里的“Bot 目录管理”包括列表、详情、批量查询、我的 Bot、好友、所属群组、可见性和退出网络接口。

路径参数说明：当前路由沿用 `/bots/{id}` 写法，但这里的 `{id}` 语义是 BCS 的 `bot_uuid`/actor id，不是数据库表里的自增主键或内部 `id` 字段。OpenAPI 建议将 path parameter 命名为 `bot_uuid`；若需要兼容现有 URL，也可以保留路径 `/bots/{id}`，但参数说明必须写明“Bot UUID”。

#### 通用鉴权约定

- Bot token：优先读取 `X-BCS-Bot-Token`；没有该 header 时读取 `Authorization: Bearer <token>`。生产环境下 JWT/AgentPass token 由 auth chain 解析为 `principal.bot_uuid`；本地/测试场景下非 JWT session token 可回退到 registry 查询。
- 人类身份：通过 `agent-pass-user-sdk` 从 Cookie 或 Bearer token 中解析 `staff_no`，在 BCS 内部转换为 `human_{staff_no}`。
- “可选身份”表示接口允许匿名调用；若提供身份，则服务层可能基于 caller 做可见性过滤或权限判断。
- “Bot 自身或 owner 人类”表示调用者必须是目标 bot 本身，或目标 bot 的 `created_by` 对应的人类身份。历史 bot 的 `created_by = null` 在部分 owner 校验中按兼容逻辑处理。

#### 通用输出字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `bot_uuid` | string | BCS 内 bot/actor 唯一 ID |
| `capabilities` | object | Bot 能力信息，常见字段包括 `name`、`summary`、`domains`、`skills`、`scopes`、`binding_channels`、`visibility` |
| `created_by` | string/null | 创建者 staff_no，可能为空 |
| `visibility` | string | 协作可见性，取值 `public`、`protected`、`private` |
| `status` | string | actor 原始状态，取值 `online` 或 `hidden` |
| `dynamic_status.status` | string | 运行时有效在线状态，取值 `active` 或 `offline` |
| `actor_kind` | string | actor 类型，取值 `bot` 或 `human` |
| `env` | string/null | actor/bot 所属环境，可能为空 |

注意：

- `skills` 的服务端模型是结构化对象，字段为 `name` 和可选 `description`。当前 `GET /bots` 和 `GET /bots/my` 为兼容旧客户端，会把 `capabilities.skills` 扁平化为技能名字符串数组；`GET /bots/{id}`、`GET /bots/paged`、`POST /bots/query` 使用 `BotCapabilities` 的结构化序列化。
- 如果面向 OpenAPI 重新定义 Bot 目录接口，建议统一使用结构化 `skills`：`[{ "name": "...", "description": "..." }]`，其中 `description` 可省略。扁平字符串数组只作为 legacy 兼容格式保留。
- `scopes` 始终是字符串数组，用于表达访问域/授权范围，例如 `["repo:read", "logs:query", "prod-db:readonly"]`。

#### `GET /bots` 与 `GET /bots/paged` 的关系

这两个接口同质化程度较高，但当前 wire contract 不完全一致：

| 对比项 | `GET /bots` | `GET /bots/paged` |
|--------|-------------|-------------------|
| 返回外层 | legacy 数组：`[...]` | 分页对象：`{ items, total, offset, limit }` |
| 默认 limit | `200` | `20` |
| 查询参数 | `onboarded`、`offset`、`limit` | `user_id`、`offset`、`limit` |
| `skills` 输出 | 当前被扁平化为 `string[]` | 结构化 `Skill[]` |
| `scopes` 输出 | `string[]` | `string[]` |
| 顶层字段 | `bot_uuid`、`capabilities`、`created_by` | `bot_uuid`、`capabilities`、`created_by` |
| 分页总数 | 不返回 `total` | 返回 `total` |

合并建议：

- OpenAPI 只暴露一个 canonical API，建议命名为 `GET /bots`；`GET /bots/paged` 标记为 `deprecated`。
- canonical `GET /bots` 使用 `/bots/paged` 的分页 envelope：`{ items, total, offset, limit }`。
- canonical `GET /bots` 同时吸收两边的查询能力：`onboarded`、`user_id`、`offset`、`limit`。
- canonical `GET /bots` 统一输出结构化 `skills` 和字符串数组 `scopes`。
- 现有 `GET /bots` legacy 数组格式可作为兼容行为逐步迁移；`GET /bots/paged` 只作为兼容接口保留一段时间，不进入 OpenAPI。

#### `GET /bots`

鉴权方式：

- 必须有调用方身份，不能是 public。
- 支持人身份 `human(cookie)`，或 agent 身份 `agent token` / `AgentPass token`。
- 当前代码实现仍是可选 caller，匿名请求也能进入 `list_bots`；需要补强为无法解析人/agent 身份时返回 401。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Query | `onboarded` | bool | 否 | 无 | 是否只返回已 onboard 的 bot |
| Query | `offset` | u64 | 否 | `0` | 起始偏移 |
| Query | `limit` | u64 | 否 | `200` | 返回数量 |

输出格式：

```json
[
  {
    "bot_uuid": "bot-alpha",
    "capabilities": {
      "name": "Bot Alpha",
      "summary": "Bot summary",
      "domains": ["code"],
      "skills": ["review", "ops"],
      "scopes": ["repo:read", "logs:query", "prod-db:readonly"],
      "visibility": "protected"
    },
    "created_by": "123456"
  }
]
```

当前实现说明：`GET /bots` 现在输出 legacy 数组，且 `skills` 被扁平化为 `string[]`。如果作为 OpenAPI canonical 接口，建议改为如下统一格式：

```json
{
  "items": [
    {
      "bot_uuid": "bot-alpha",
      "capabilities": {
        "name": "Bot Alpha",
        "summary": "Bot summary",
        "domains": ["code"],
        "skills": [
          {
            "name": "review",
            "description": "Review code"
          },
          {
            "name": "ops"
          }
        ],
        "scopes": ["repo:read", "logs:query", "prod-db:readonly"],
        "visibility": "protected"
      },
      "created_by": "123456"
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 200
}
```

#### `GET /bots/my`

鉴权方式：

- 人类身份必需。
- 未解析到 `staff_no` 时返回 `401 Unauthorized`。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Query | `offset` | u64 | 否 | `0` | 起始偏移 |
| Query | `limit` | u64 | 否 | `20` | 返回数量 |
| Query | `active_only` | bool | 否 | `false` | 是否只返回 active bot |

输出格式：

```json
{
  "items": [
    {
      "bot_uuid": "bot-alpha",
      "capabilities": {
        "name": "Bot Alpha",
        "summary": "Bot summary",
        "domains": ["code"],
        "skills": ["review"],
        "scopes": ["repo:read", "logs:query"],
        "visibility": "public"
      },
      "visibility": "public",
      "created_by": "123456",
      "actor_kind": "bot",
      "env": "dev",
      "status": "online",
      "dynamic_status": {
        "status": "active"
      }
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 20
}
```

#### `GET /bots/paged`

状态：`deprecated`。该接口与 `GET /bots` 同质化，建议将分页 envelope、`user_id` 过滤能力和结构化 `skills` 输出合并到 canonical `GET /bots` 后停止对外暴露本接口。

鉴权方式：

- `human(cookie)` 或 `agent token`/`AgentPass token`。
- 路径 `{id}` 为 `bot_uuid`/actor id；调用方必须有权查询该 actor 的群列表。
- 当前代码实现尚未读取 cookie/header，实际仍是匿名可访问，需要补强为无身份返回 `401`、越权返回 `403`。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Query | `user_id` | string | 否 | 无 | 按创建者/用户过滤 |
| Query | `offset` | u64 | 否 | `0` | 起始偏移 |
| Query | `limit` | u64 | 否 | `20` | 返回数量 |

输出格式：

```json
{
  "items": [
    {
      "bot_uuid": "bot-alpha",
      "capabilities": {
        "name": "Bot Alpha",
        "summary": "Bot summary",
        "domains": ["code"],
        "skills": [
          {
            "name": "review",
            "description": "Review code"
          }
        ],
        "scopes": ["repo:read", "logs:query"],
        "visibility": "protected"
      },
      "created_by": "123456"
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 20
}
```

#### `POST /bots/query`

鉴权方式：

- 无鉴权要求。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Body | `bot_uuids` | string[] | 是 | 无 | 要批量查询的 bot UUID 列表 |

请求体示例：

```json
{
  "bot_uuids": ["bot-alpha", "bot-beta"]
}
```

输出格式：

```json
[
  {
    "bot_uuid": "bot-alpha",
    "capabilities": {
      "name": "Bot Alpha",
      "summary": "Bot summary",
      "domains": ["code"],
      "skills": [
        {
          "name": "review",
          "description": "Review code"
        }
      ],
      "scopes": ["repo:read", "logs:query"],
      "visibility": "public"
    },
    "visibility": "public",
    "status": "online",
    "actor_kind": "bot",
    "dynamic_status": {
      "status": "active"
    }
  }
]
```

#### `GET /bots/{id}`

鉴权方式：

- 必须有调用方身份，不能是 public。
- 支持人身份 `human(cookie)`，或 agent 身份 `agent token` / `AgentPass token`。
- 当前代码实现仍是可选 caller，匿名请求也能进入 `get_bot`；需要补强为无法解析人/agent 身份时返回 401。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | Bot UUID；不是数据库主键 |

输出格式：

```json
{
  "bot_uuid": "bot-alpha",
  "capabilities": {
    "name": "Bot Alpha",
    "summary": "Bot summary",
    "domains": ["code"],
    "skills": [
      {
        "name": "review",
        "description": "Review code"
      }
    ],
    "scopes": ["repo:read", "logs:query"],
    "visibility": "protected"
  },
  "created_by": "123456",
  "actor_kind": "bot",
  "env": "dev",
  "status": "online",
  "dynamic_status": {
    "status": "active"
  }
}
```

注意：该接口当前不在顶层输出 `visibility` 字段；需要读取顶层可见性时使用 `GET /bots/{id}/visibility`，或从 `capabilities.visibility` 读取兼容字段。

#### `DELETE /bots/{id}`

鉴权方式：

- 人类身份必需。
- 调用者必须满足 owner 删除规则；服务层要求 caller 是 `human_{staff_no}`，并和解析到的人类身份一致。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | Bot UUID；不是数据库主键 |

输出格式：

```json
{
  "left": true,
  "bot_uuid": "bot-alpha"
}
```

#### `GET /bots/{id}/friends`

鉴权方式：

- Bot 自身或 owner 人类必需。
- Bot token 调用时 caller 必须能通过服务层 `self-or-owner` 权限检查；人类调用时必须能证明拥有目标 bot。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | 目标 bot_uuid/actor id；不是数据库主键 |

输出格式：

```json
{
  "success": true,
  "data": [
    {
      "bot_uuid": "friend-bot",
      "name": "Friend Bot",
      "summary": "Friend summary",
      "is_online": true,
      "dynamic_status": {
        "status": "active"
      }
    }
  ]
}
```

常见错误输出：

```json
{
  "success": false,
  "error": "Not authorized to access bot 'bot-alpha'"
}
```

#### `GET /bots/{id}/groups`

鉴权方式：

- 无鉴权要求。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | Bot UUID/actor id；不是数据库主键 |
| Query | `offset` | u64 | 否 | `0` | 起始偏移 |
| Query | `limit` | u64 | 否 | `10` | 返回数量 |
| Query | `group_kind` | string | 否 | `normal` | 群类型过滤，取值 `normal`、`dm`、`all` |
| Query | `q` | string | 否 | 无 | 按群 label 模糊过滤 |

输出格式：

```json
{
  "bot_uuid": "bot-alpha",
  "items": [
    {
      "group_id": "group-1",
      "label": "Project Group",
      "coordinator_bot": "bot-alpha",
      "participants": [
        {
          "bot_uuid": "bot-alpha",
          "bot_name": "Bot Alpha",
          "role": "driver",
          "actor_kind": "bot",
          "mode": "auto"
        }
      ],
      "created_at": 1710000000000,
      "updated_at": 1710000000000,
      "group_kind": "normal",
      "group_strategy": "chat",
      "visibility": "private"
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 10
}
```

#### `GET /bots/{id}/visibility`

鉴权方式：

- `human(cookie)` 或 `agent token`/`AgentPass token` 必需；该接口不作为 public 接口暴露。
- Bot 自身可读；owner 人类可读。
- 其他已认证 caller 是否可读由 service 按目标 Bot 的可见性策略判断；若要开放发现语义，优先通过 `GET /bots/discover` 承载。
- 当前代码无身份时仍会传 `None` 并被 service 放行，需要补强为无身份返回 `401`。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | Bot UUID；不是数据库主键 |

输出格式：

```json
{
  "success": true,
  "data": {
    "bot_uuid": "bot-alpha",
    "visibility": "protected"
  }
}
```

错误输出格式：

```json
{
  "success": false,
  "error": "Bot 'bot-alpha' not found"
}
```

#### `PUT /bots/{id}/visibility`

鉴权方式：

- `agent token`/`AgentPass token` 的 Bot 自身，或 `human(cookie)` owner 必需；该接口不作为 public 接口暴露。
- 匿名调用会在服务层返回 `401`。
- 其他人类或其他 bot 调用会返回 `403`。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | Bot UUID；不是数据库主键 |
| Body | `visibility` | string | 是 | 无 | 取值 `public`、`protected`、`private` |

请求体示例：

```json
{
  "visibility": "private"
}
```

输出格式：

```json
{
  "success": true,
  "data": {
    "bot_uuid": "bot-alpha",
    "visibility": "private"
  }
}
```

错误输出格式：

```json
{
  "success": false,
  "error": "visibility must be 'public', 'protected', or 'private'"
}
```

### Bot 内部生命周期/消息

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/bots/connect` | Bot 连接握手兼容接口；WebSocket 连接模型下不需要，建议标记待删除 | 不暴露；待删除 |
| GET | `/bots/discover` | Bot 发现接口 | 不暴露 |
| POST | `/bots/onboard` | Bot 注册能力 | deprecated |
| POST | `/admin/bots/onboard` | Admin/前端代理使用的 Bot 入网或能力更新接口 | 不暴露；管理/内部接口 |
| POST | `/bots/status` | Bot 动态状态更新；当前 dynamic status 基本没有实际业务用途 | 不暴露；待废弃 |
| POST | `/bots/{id}/chat` | Bot 1:1 同步消息；legacy blocking 形态 | 不暴露；建议并入 async 后废弃 |

#### 1:1 Bot Chat 收敛建议

`/bots/{id}/chat` 和 async 三件套能力同质化：前者在 HTTP 请求内阻塞等待最终响应，后者先提交 run，再通过 run 查询接口读取状态和结果。建议 `/bots/{id}/chat` 仅作为 legacy blocking shim 兼容旧客户端，后续调用方迁移完成后删除。

#### Bot 内部生命周期/消息详细接口

本节展开 Bot 内部生命周期/消息接口的鉴权、入参和出参。该组接口不建议进入主 OpenAPI；其中 `/bots/connect` 和 `/bots/onboard` 待删除，`/bots/status` 待废弃，`/bots/{id}/chat` 是 legacy blocking 形态。

##### 通用鉴权约定

- Bot token：优先读取 `X-BCS-Bot-Token`；没有该 header 时读取 `Authorization: Bearer <token>`。
- 容器校验：`/bots/onboard`、`/bots/status`、`/bots/{id}/chat` 会校验可选 header `x-agentclaw-bolt-id`。当 `strict_container_validation` 开启时，token 解析出的 bot_uuid 必须和该 header 匹配。
- 人类身份：`/bots/connect`、`/bots/onboard`、`/admin/bots/onboard` 会尝试从 Cookie 或 Bearer token 解析 `staff_no`；onboard 类接口会把该人类身份作为 owner 信息写入 onboard 上下文。
- 路径参数：`/bots/{id}/chat` 的 `{id}` 是目标 Bot UUID，不是数据库主键。

##### `POST /bots/connect`

鉴权方式：

- 可选身份。
- Body 中的 `token` 用于重连；未传 token 或 token 无效时会创建新的 bot_uuid/token。
- 当前 HTTP connect 只是兼容接口；WebSocket-only 连接模型下建议待删除。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Body | `token` | string/null | 否 | 无 | 旧 session token；有效时返回原 bot_uuid，未传或无效时创建新连接 |
| Body | `bot_id` | string/null | 否 | 自动生成 | 预配置 Bot UUID；不是数据库主键。仅新连接时使用，已
注册会报错 |
| Body | `protocol_version` | u32/null | 否 | 服务端默认 | 客户端请求的协议版本 |
| Body | `client_kind` | string/null | 否 | 无 | DTO 支持该字段，但当前 HTTP handler 没有传给 service，实际不生效 |

请求体示例：

```json
{
  "token": null,
  "bot_id": "bot-alpha",
  "protocol_version": 2
}
```

输出格式：

```json
{
  "is_new": true,
  "bot_uuid": "bot-alpha",
  "token": "bcs-session-token"
}
```

常见错误：

- `400 Bad Request`：`bot_id` 非法、`bot_id` 已注册，或 connect service 返回兼容错误。

##### `GET /bots/discover`

鉴权方式：

- 无鉴权要求。
- 服务层只返回可发现的 Bot：`public`、`protected`；`private` 不出现在结果中。
- 如果传 `collaborate_bot`，结果会进一步过滤为该 bot 可协作的 Bot：public + friends；同时输出 `is_friend`。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Query | `q` | string | 否 | 无 | 综合搜索 name、summary、dynamic_summary、domains、skills 等 |
| Query | `name` | string | 否 | 无 | 按 Bot 名称模糊搜索 |
| Query | `skills` | string | 否 | 无 | 逗号分隔，要求全部技能匹配 |
| Query | `domains` | string | 否 | 无 | 逗号分隔，要求全部领域匹配 |
| Query | `scopes` | string | 否 | 无 | 逗号分隔，要求全部 scope 匹配 |
| Query | `visibility` | string | 否 | 无 | 可见性过滤，常见取值 `public`、`protected` |
| Query | `collaborate_bot` | string | 否 | 无 | 以该 Bot UUID 的协作资格过滤结果 |

注意：`q`、`name`、`skills`、`domains`、`scopes` 在当前实现中是优先级选择关系，优先级为 `q > name > skills > domains > scopes`；`visibility` 和 `collaborate_bot` 是额外过滤。

输出格式：

```json
{
  "bots": [
    {
      "bot_uuid": "bot-alpha",
      "capabilities": {
        "name": "Bot Alpha",
        "summary": "Bot summary",
        "domains": ["code"],
        "skills": [
          {
            "name": "review",
            "description": "Review code"
          }
        ],
        "scopes": ["repo:read"],
        "visibility": "public"
      },
      "visibility": "public",
      "is_friend": false,
      "agent_code": "agent-code",
      "provider_info": {
        "provider_id": "provider-1",
        "provider_name": "Provider"
      }
    }
  ],
  "count": 1
}
```

`is_friend` 仅在请求包含 `collaborate_bot` 时输出；`agent_code`、`provider_info` 仅在服务端有对应数据时输出。

##### `POST /bots/onboard`

状态：`deprecated`。该 Bot Runtime 自助入网入口进入兼容下线阶段，不再作为目标态 Internal API 发布。

鉴权方式：

- Bot token 必需。
- 支持 `X-BCS-Bot-Token` 或 `Authorization: Bearer <token>`。
- 校验可选 `x-agentclaw-bolt-id` 容器 header。
- 可选读取人类身份作为 owner 信息；可选读取 `x-agentclaw-agent-code` 和 `Authorization` 作为 agent credential 相关信息。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Body | `name` | string | 是 | 无 | Bot 展示名 |
| Body | `summary` | string/null | 否 | 无 | Bot 能力摘要 |
| Body | `domains` | string[] | 否 | `[]` | 领域标签 |
| Body | `skills` | Skill[] | 否 | `[]` | 技能列表，支持 `[{ "name": "...", "description": "..." }]`，也兼容 legacy `string[]` |
| Body | `scopes` | string[] | 否 | `[]` | scope 列表，例如 `["repo:read", "logs:query"]` |
| Body | `binding_channels` | object/null | 否 | 无 | 待删除；仅兼容旧客户端的外部通道绑定，key 为通道名，value 形如 `{ "binding_key": "..." }` |
| Header | `Authorization` | string | 否 | 无 | 当前实现会把原始值作为 agent_token 传入 onboarding service；也可能用于 Bearer Bot token |
| Header | `x-agentclaw-agent-code` | string | 否 | 无 | AI Security Gateway agent_code |
| Header | `x-agentclaw-bolt-id` | string | 否 | 无 | 容器侧 bot 标识，用于 strict 校验 |

字段状态：`binding_channels` 已标记为待删除，推荐调用方停止传入；后续如果仍需要通道绑定能力，应拆到独立管理接口或由 provider/admin 管理。

请求体示例：

```json
{
  "name": "Bot Alpha",
  "summary": "Review and ops helper",
  "domains": ["code", "ops"],
  "skills": [
    {
      "name": "review",
      "description": "Review code"
    }
  ],
  "scopes": ["repo:read", "logs:query"]
}
```

成功输出：

```json
{
  "bot_uuid": "bot-alpha",
  "onboarded": true,
  "name": "Bot Alpha",
  "binding_results": {},
  "unbound": []
}
```

未注册/不可 onboard 时的兼容输出：

```json
{
  "bot_uuid": "bot-alpha",
  "onboarded": false,
  "message": "Bot 未在协作网络注册，请尝试重启"
}
```

##### `POST /admin/bots/onboard`

路径与代理口径：

- BCS 原始路径：`POST /admin/bots/onboard`。
- 前端同源调用路径：`POST /bcnproxy/admin/bots/onboard`，由 Bigfish/Tern proxy rewrite 到 BCS 原始路径。
- Backend 透明网关路径：`POST /api/v1/admin/bots/onboard`，由 backend gateway 转发到 BCS 原始路径。
- 如果浏览器直接向前端 origin 或 AgentClaw backend origin 调 `POST /admin/bots/onboard`，通常会返回 `404`；这不是 BCS route 不存在，而是缺少 `/bcnproxy` 或 `/api/v1` 网关前缀。

鉴权方式：

- BCS handler 本身不要求 Bot token。
- 会尝试从 Cookie 或 Bearer token 解析人类身份；如果解析成功，会用于绑定 `created_by` 和 owner 关系。
- 在办公网网关后直接访问 `https://bcn-pre.alipay.com/admin/bots/onboard` 或 `https://bcn.alipay.com/admin/bots/onboard` 时，可能先被 ACE/Buservice 登录拦截；这发生在请求到达 BCS handler 之前。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Body | `bot_id` | string | 是 | 无 | Bot UUID；不是数据库主键。目标 Bot 必须已通过连接流程存在于 registry |
| Body | `name` | string/null | 条件必填 | 保留旧值 | 展示名。首次 admin onboard 时必须提供非空 name；更新时可省略并保留旧值 |
| Body | `summary` | string/null | 否 | 保留旧值 | Bot 能力摘要；空/缺省时保留旧值 |
| Body | `domains` | string[] | 否 | 保留旧值或 `[]` | 非空时覆盖；空数组/缺省时保留旧值 |
| Body | `skills` | Skill[] | 否 | 保留旧值或 `[]` | 非空时覆盖；支持结构化 skill，也兼容 legacy `string[]` |
| Body | `scopes` | string[] | 否 | 保留旧值或 `[]` | 非空时覆盖；例如 `["repo:read", "logs:query"]` |
| Body | `binding_channels` | object/null | 否 | 保留旧值 | 删除；仅兼容旧客户端的外部通道绑定，key 为通道名，value 形如 `{ "binding_key": "..." }` |
| Body | `hidden` | bool/null | 否 | 无 | 兼容旧客户端字段；当前 handler 忽略，不应用于可见性控制 |

字段状态：`binding_channels` 已标记为删除，推荐调用方停止传入；后续如果仍需要通道绑定能力，应拆到独立管理接口或由 provider/admin 管理。

前端请求示例：

```json
{
  "bot_id": "bot-alpha:123456",
  "name": "Bot Alpha",
  "summary": "Review and ops helper",
  "domains": ["code", "ops"],
  "skills": [
    {
      "name": "review",
      "description": "Review code"
    }
  ],
  "scopes": ["repo:read", "logs:query"],
  "hidden": false
}
```

成功输出：

```json
{
  "bot_uuid": "bot-alpha:123456",
  "onboarded": true,
  "name": "Bot Alpha",
  "binding_results": {},
  "unbound": []
}
```

目标 Bot 不在 registry 中时的兼容输出：

```json
{
  "bot_uuid": "bot-alpha:123456",
  "onboarded": false,
  "message": "Bot 未在协作网络注册，请尝试重启"
}
```


##### `POST /bots/status`

鉴权方式：

- Bot token 必需。
- 支持 `X-BCS-Bot-Token` 或 `Authorization: Bearer <token>`。
- 校验可选 `x-agentclaw-bolt-id` 容器 header。
- 只能更新 token 所属 Bot 的状态。`bot_uuid` 为空字符串时按调用方自身处理；如果传其他 bot_uuid，返回 `403 Forbidden`。
- 当前 dynamic status 基本没有实际业务用途，建议待废弃。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Body | `bot_uuid` | string | 是 | 无 | 目标 Bot UUID；可传空字符串表示 token 所属 Bot |
| Body | `status` | object | 是 | 无 | 动态状态对象 |
| Body | `status.status` | string | 否 | `""` | 动态状态，例如 `idle`、`busy`、`offline` |
| Body | `status.dynamic_summary` | string/null | 否 | 无 | 当前动态摘要 |
| Body | `status.load` | number/null | 否 | 无 | 负载，约定范围 0.0-1.0 |
| Body | `status.updated_at` | u64/null | 否 | 无 | 状态更新时间戳 |

请求体示例：

```json
{
  "bot_uuid": "bot-alpha",
  "status": {
    "status": "busy",
    "dynamic_summary": "Reviewing repository changes",
    "load": 0.7,
    "updated_at": 1710000000000
  }
}
```

输出格式：

```json
{
  "updated": true,
  "bot_uuid": "bot-alpha",
  "status": {
    "status": "busy",
    "dynamic_summary": "Reviewing repository changes",
    "load": 0.7,
    "updated_at": 1710000000000
  }
}
```

##### `POST /bots/{id}/chat`

鉴权方式：

- Bot token 必需；调用方 Bot 从 token 解析，不能通过 body 指定。
- `{id}` 是目标 Bot UUID，不是数据库主键。
- 校验可选 `x-agentclaw-bolt-id` 容器 header。
- 可选读取人类身份 `staff_no` 作为调用上下文。
- 这是 legacy blocking 接口；建议迁移到“单 Bot 异步调用”三件套。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `id` | string | 是 | 无 | 目标 Bot UUID |
| Body | `message` | string | 是 | 无 | 发给目标 Bot 的消息 |
| Body | `from` | string/null | 否 | `"user"` | 消息发送方标识 |
| Body | `timeout_ms` / `timeoutMs` | u64/null | 否 | `300000` | blocking 等待超时，服务端上限 300000ms |
| Body | `session_id` / `sessionId` | string/null | 否 | 自动生成 | 会话 ID；最长 128，只允许 ASCII 字母数字和 `-`、`_`、`:`、`.` |
| Body | `tags` | string[] | 否 | `[]` | 标签，空白 tag 会被过滤 |
| Body | `response_mode` / `responseMode` | string | 否 | `full` | 取值 `full`、`after-last-tool-call` |
| Body | `caller_wait_mode` / `callerWaitMode` | string/null | 否 | 无 | DTO 接受该字段，但 blocking `/chat` 当前不使用 |

请求体示例：

```json
{
  "message": "Please review this change",
  "from": "user",
  "timeout_ms": 300000,
  "session_id": "chat:bot-alpha:001",
  "tags": ["review"],
  "response_mode": "full"
}
```

输出格式：

```json
{
  "delivered": true,
  "bot_uuid": "bot-target",
  "session_id": "chat:bot-alpha:001",
  "response": {
    "content": "Review result"
  }
}
```

常见错误输出：

```json
{
  "error": "Invalid or expired session token",
  "status": 401
}
```

其他常见错误包括目标 Bot 不存在/未连接时 `404`，目标 Bot 不可协作或非好友时 `403`，`session_id` 非法时 `400`。

### 单 Bot 异步调用

这一组接口共同组成“调用单个 Bot”的 async invocation 链路，建议作为单独 API group 管理，而不是拆到 Bot 生命周期或通用 Chat Run 分类里。`bcs-cli chat` 当前已经只使用 `/bots/{bot_uuid}/chat-async`，并通过 `GET /chat/runs/{run_id}` 长轮询结果；`--detach` 也是先提交 `chat-async`，再等到 `running` 或 `completed` 作为首个 ack 后返回。

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/bots/{id}/chat-async` | 提交单 Bot 异步调用，返回 run 信息 | 当前不暴露；单 Bot invocation group |
| GET | `/chat/runs/{run_id}` | 获取单 Bot 异步调用的 run 状态和结果，支持长轮询 | 当前不暴露；单 Bot invocation group |
| POST | `/chat/runs/{run_id}/cancel` | 取消单 Bot 异步调用 run | 当前不暴露；单 Bot invocation group |

建议：

- 不在 OpenAPI 同时暴露 `/bots/{id}/chat` 和 `/bots/{id}/chat-async`。
- 单 Bot invocation 的 canonical 形态是这组三件套，不要只暴露 submit 而缺少 query/cancel。
- submit 使用 `/bots/{bot_uuid}/chat-async`，返回 `run_id`、`bot_uuid`、`session_id`、`status`、`expires_at_ms`。
- 结果查询使用 `GET /chat/runs/{run_id}`，支持 `wait_ms`、`since_version` 做长轮询；取消使用 `POST /chat/runs/{run_id}/cancel`。
- `/bots/{id}/chat` 标记为 legacy blocking shim，仅兼容旧客户端；后续可在调用方迁移完成后删除。
- 如果未来需要对外提供“调用单个 Bot”的 OpenAPI，也建议只暴露 async 三件套：submit、query、cancel，而不是同步 blocking API。

`POST /bots/{id}/chat-async` 典型输出：

```json
{
  "run_id": "run-1",
  "bot_uuid": "bot-target",
  "session_id": "session-1",
  "status": "submitted",
  "expires_at_ms": 1710001800000
}
```

`GET /chat/runs/{run_id}` 查询参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Query | `wait_ms` | u64 | 否 | `0` | 长轮询等待时间，服务端会按配置截断 |
| Query | `since_version` | u64 | 否 | `0` | 仅当 run 版本大于该值、或进入终态、或等待超时后返回 |

`GET /chat/runs/{run_id}` 典型输出：

```json
{
  "run_id": "run-1",
  "bot_uuid": "bot-target",
  "from_bot_id": "bot-source",
  "session_id": "session-1",
  "state": "completed",
  "response": {
    "content": "done"
  },
  "error_message": null,
  "created_at_ms": 1710000000000,
  "updated_at_ms": 1710000001000,
  "completed_at_ms": 1710000001000,
  "expires_at_ms": 1710001800000,
  "version": 3,
  "content_truncated": false,
  "is_terminal": true
}
```

`POST /chat/runs/{run_id}/cancel` 典型输出：

```json
{
  "run_id": "run-1",
  "cancelled": true,
  "state": "cancelled",
  "response": {
    "content": ""
  },
  "error_message": null,
  "version": 4,
  "content_truncated": false
}
```

`state` 常见取值包括 `pending`、`submitted`、`running`、`completed`、`failed`、`cancelled`。旧客户端未声明 `BCS_CHAT_VERSION >= 2` 时，服务端会把 `submitted` 兼容映射为 `running`。

### Provider 管理

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/providers/{provider_id}` | 获取 provider | 可暴露 |
| PATCH | `/providers/{provider_id}` | 更新 provider | 可暴露 |
| GET | `/providers/{provider_id}/bots` | provider 下 bot 列表 | 可暴露 |
| POST | `/providers/{provider_id}/bots` | 注册 provider bot | 可暴露 |
| DELETE | `/providers/{provider_id}/bots/{provider_bot_ref}` | 删除 provider bot | 可暴露 |

### Provider 管理详细接口

本节展开“Provider 管理”相关接口的鉴权、入参和出参。这里的 Provider 管理不包括 provider 注册、agentpass 解析、灰度、启停、投递切换和 bot event 回调；这些仍归入“Provider 内部/灰度/回调”。

#### 通用鉴权约定

- Provider admin token：读取 `Authorization: Bearer <provider_admin_token>`。该 token 是 `POST /providers` 注册 provider 时返回的 `provider_admin_token`，不是 Bot runtime token，也不是 `bcs_to_provider_token`。
- 路径中的 `provider_id` 必须与 provider admin token 所属 provider 匹配；不匹配时返回 `403 provider_id_mismatch`。
- `GET /providers/{provider_id}`、`GET /providers/{provider_id}/bots`、`POST /providers/{provider_id}/bots`、`DELETE /providers/{provider_id}/bots/{provider_bot_ref}` 不要求人类身份。
- `PATCH /providers/{provider_id}` 额外要求从 Cookie 或 Bearer token 解析到人类身份，并且该 staff_no 必须是 provider owner。
- 已禁用 provider：`GET /providers/{provider_id}` 可读取元信息；注册/list/delete provider bot 会拒绝禁用 provider；`PATCH /providers/{provider_id}` 仍可用于更新元信息。

通用错误输出：

```json
{
  "error": "valid provider admin token is required",
  "status": 401
}
```

#### 通用对象结构

Provider 元数据输出：

```json
{
  "provider_id": "provider_abc",
  "name": "Provider",
  "webhook_url": "https://provider.example.com/bcs/webhook",
  "auth_mode": "static_bearer",
  "coordination": {
    "mode": "mcporter_mcp",
    "mcp_server": "bcs",
    "mcporter_command": "mcporter"
  },
  "disabled": false,
  "created_at": 1710000000000,
  "updated_at": 1710000000000
}
```

注意：

- Provider 元数据输出不会返回 `provider_admin_token` 或 `bcs_to_provider_token`。
- `auth_mode` 取值：`static_bearer`、`agentpass`、`provider_admin`。
- `coordination.mode` 取值：`mcporter_mcp`、`native_mcp`、`native_tool`、`disabled`；未配置时 `coordination` 可省略。

Provider bot binding 输出：

```json
{
  "bot_uuid": "bot_abc",
  "provider_id": "provider_abc",
  "provider_bot_ref": "reviewer-v2",
  "disabled": false,
  "created_at": 1710000000000,
  "updated_at": 1710000000000
}
```

#### `GET /providers/{provider_id}`

鉴权方式：

- Provider admin token 必需。
- 不要求人类身份。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `provider_id` | string | 是 | 无 | Provider ID |
| Header | `Authorization` | string | 是 | 无 | `Bearer <provider_admin_token>` |

输出格式：

```json
{
  "provider_id": "provider_abc",
  "name": "Provider",
  "webhook_url": "https://provider.example.com/bcs/webhook",
  "auth_mode": "static_bearer",
  "disabled": false,
  "created_at": 1710000000000,
  "updated_at": 1710000000000
}
```

#### `PATCH /providers/{provider_id}`

鉴权方式：

- Provider admin token 必需。
- 人类身份必需，且必须是 provider owner。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `provider_id` | string | 是 | 无 | Provider ID |
| Header | `Authorization` | string | 是 | 无 | `Bearer <provider_admin_token>`；同一请求还会尝试用 Cookie 或 Bearer 解析人类身份 |
| Body | `name` | string/null | 否 | 保留旧值 | Provider 展示名 |
| Body | `webhook_url` | string/null | 否 | 保留旧值 | Provider downlink webhook；必须以 `http://` 或 `https://` 开头 |
| Body | `protocol_version` | string/null | 否 | 保留旧值 | Downlink 协议版本，支持 `1.0`、`2.0`；空字符串按 `1.0` 处理 |
| Body | `coordination` | object/null | 否 | 保留旧值 | Provider 协作配置 |
| Body | `coordination.mode` | string | 是 | 无 | `mcporter_mcp`、`native_mcp`、`native_tool`、`disabled` |
| Body | `coordination.mcp_server` | string/null | 条件必填 | 无 | `mcporter_mcp`、`native_mcp` 需要 |
| Body | `coordination.mcporter_command` | string/null | 条件必填 | 无 | `mcporter_mcp` 需要；`native_mcp`、`native_tool` 不应设置 |

请求体示例：

```json
{
  "name": "Updated Provider",
  "webhook_url": "https://provider.example.com/updated/webhook",
  "protocol_version": "2.0",
  "coordination": {
    "mode": "native_mcp",
    "mcp_server": "bcs"
  }
}
```

输出格式：同 `GET /providers/{provider_id}`。

#### `GET /providers/{provider_id}/bots`

鉴权方式：

- Provider admin token 必需。
- 不要求人类身份。
- Provider 被禁用时返回错误。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `provider_id` | string | 是 | 无 | Provider ID |
| Header | `Authorization` | string | 是 | 无 | `Bearer <provider_admin_token>` |

输出格式：

```json
{
  "items": [
    {
      "bot_uuid": "bot_abc",
      "provider_id": "provider_abc",
      "provider_bot_ref": "reviewer-v2",
      "disabled": false,
      "created_at": 1710000000000,
      "updated_at": 1710000000000
    }
  ]
}
```

说明：当前返回该 provider 下未 disabled 的 binding；已 soft-delete/disabled 的 provider bot 不在列表中。

#### `POST /providers/{provider_id}/bots`

鉴权方式：

- Provider admin token 必需。
- 不要求人类身份。
- Provider 被禁用时返回错误。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `provider_id` | string | 是 | 无 | Provider ID |
| Header | `Authorization` | string | 是 | 无 | `Bearer <provider_admin_token>` |
| Body | `name` | string | 是 | 无 | Provider bot 展示名，会写入 BCS bot capability name |
| Body | `summary` | string/null | 否 | 无 | Provider bot 能力摘要 |
| Body | `owners` | string[] | 是 | 无 | 必须且只能包含一个非空 staff_no；用于绑定 owner human actor |
| Body | `provider_bot_ref` | string | 是 | 无 | Provider 内部 Bot ID。只能包含 ASCII 字母数字和 `-`、`_`、`.`、`:`，最长 256 |
| Body | `domains` | string[] | 否 | `[]` | 领域标签 |
| Body | `skills` | Skill[] | 否 | `[]` | 技能列表，统一使用 `[{ "name": "...", "description": "..." }]`；不在 OpenAPI 透出 legacy `string[]` |
| Body | `scopes` | string[] | 否 | `[]` | scope 列表，例如 `["repo:read", "logs:query"]` |

请求体示例：

```json
{
  "name": "Code Reviewer",
  "summary": "Reviews code",
  "owners": ["197262"],
  "provider_bot_ref": "reviewer-v2",
  "domains": ["development", "security"],
  "skills": [
    {
      "name": "code_review",
      "description": "Reviews source code"
    },
    {
      "name": "sql_analysis",
      "description": "Analyzes SQL"
    }
  ],
  "scopes": ["production"]
}
```

成功输出：

```json
{
  "bot_uuid": "bot_abc",
  "provider_id": "provider_abc",
  "provider_bot_ref": "reviewer-v2",
  "bot_runtime_token": "runtime-token"
}
```

说明：

- `auth_mode=static_bearer` 或 `auth_mode=provider_admin` 时返回 `bot_runtime_token`；`auth_mode=agentpass` 时通常不返回该字段。
- 如果同一 `provider_bot_ref` 已注册，接口幂等返回已有 bot，通常不返回 `bot_runtime_token`，并带 `message`：

```json
{
  "bot_uuid": "bot_abc",
  "provider_id": "provider_abc",
  "provider_bot_ref": "reviewer-v2",
  "message": "provider bot ref already registered; returning existing bot"
}
```

#### `DELETE /providers/{provider_id}/bots/{provider_bot_ref}`

鉴权方式：

- Provider admin token 必需。
- 不要求人类身份。
- Provider 被禁用时返回错误。

接受的参数：

| 位置 | 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|------|--------|------|
| Path | `provider_id` | string | 是 | 无 | Provider ID |
| Path | `provider_bot_ref` | string | 是 | 无 | Provider 内部 Bot ID；不是数据库主键。当前代码实际按 `provider_bot_ref` 解析最后一段，不是 BCS `bot_uuid` |
| Header | `Authorization` | string | 是 | 无 | `Bearer <provider_admin_token>` |

成功输出：

```json
{
  "deleted": true,
  "provider_id": "provider_abc",
  "provider_bot_ref": "reviewer-v2",
  "bot_uuid": "bot_abc"
}
```

目标 Bot 不存在或不在 BCS registry 中时，当前实现兼容返回 200：

```json
{
  "deleted": false,
  "provider_id": "provider_abc",
  "provider_bot_ref": "reviewer-v2",
  "message": "bot is not registered in BCS"
}
```

### Provider 内部/灰度/回调

| Method | Path | 说明 | OpenAPI   |
|--------|------|------|-----------|
| POST | `/providers` | 注册 provider，返回 admin/runtime token | 不暴露       |
| POST | `/providers/agentpass/resolve` | 解析 agentpass bot | 不暴露 / 待废弃 |
| GET | `/providers/stream-gray` | 查询 provider stream 灰度名单 | 不暴露 / 待废弃 |
| PUT | `/providers/stream-gray` | 更新 provider stream 灰度名单 | 不暴露 / 待废弃 |
| POST | `/providers/{provider_id}/disable` | 停用 provider | 不暴露       |
| POST | `/providers/{provider_id}/enable` | 启用 provider | 不暴露       |
| POST | `/providers/{provider_id}/delivery/switch-bot` | 切换 bot 投递方式 | 不暴露       |
| POST | `/bot/events` | provider 上报 bot 运行时事件 | 不暴露       |
| POST | `/bot/events/coordination` | provider 上报 bot 协作/工具调用事件 | deprecated   |

### Provider 内部/灰度/回调详细接口

本节展开“Provider 内部/灰度/回调”相关接口的鉴权、入参和出参。这组接口用于 Provider 自注册、内部灰度、Provider downlink 切换，以及 Provider 向 BCS 回调 bot 运行事件；不建议进入面向普通 OpenAPI 用户的公开接口面。

通用鉴权约定：

| 鉴权材料 | 使用场景 | 说明 |
|----------|----------|------|
| 人类身份 | `POST /providers`、`POST /providers/{provider_id}/disable`、`POST /providers/{provider_id}/enable` | 通过登录态 Cookie 或 Bearer token 解析 `staff_no`。缺失时返回 401。 |
| Provider admin token | `disable`、`enable`、`delivery/switch-bot`、以及部分回调模式 | 请求头 `Authorization: Bearer <provider_admin_token>`。token 来自 `POST /providers` 返回。 |
| AgentPass token | `POST /providers/agentpass/resolve`、`POST /bot/events`、`POST /bot/events/coordination` | 请求头 `Authorization: Bearer <agentpass_jwt>`，BCS 解析为 `agent_code`。 |
| Bot runtime token | `POST /bot/events`、`POST /bot/events/coordination` | 请求头 `Authorization: Bearer <bot_runtime_token>`，适用于 `static_bearer` Provider bot 回调。 |
| Provider 标识 | Provider 回调接口 | 请求头 `X-BCN-Provider-Id: <provider_id>`。 |
| Provider bot 引用 | 使用 Provider admin token 进行 bot event 回调时 | 请求头 `X-BCN-Provider-Bot-Ref: <provider_bot_ref>`，用于定位 provider 侧 bot。 |

错误格式：

- Provider 管理类接口使用：

```json
{
  "error": "valid provider admin token is required",
  "status": 401
}
```

- Bot event 回调接口使用：

```json
{
  "error": "unauthorized",
  "message": "unauthorized",
  "status": 401
}
```

#### `POST /providers`

注册 Provider，生成 Provider 管理 token 和 BCS 调 Provider 的 token。当前属于内部 bootstrap 能力，不建议暴露给普通 OpenAPI 用户。

鉴权方式：

- 必须具备人类身份。BCS 从请求登录态中解析 `staff_no` 并作为 Provider owner。
- 该接口不使用 Provider admin token，因为 token 正是在本接口中签发。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Body | `name` | string | 是 | Provider 名称。 |
| Body | `webhook_url` | string | 是 | Provider 下行 webhook 地址。 |
| Body | `auth.mode` | string | 是 | Provider bot event 回调鉴权模式：`static_bearer`、`agentpass`、`provider_admin`。 |
| Body | `protocol_version` | string | 否 | Downlink 协议版本，当前常见值为 `1.0` 或 `2.0`；缺省兼容 `1.0`。 |
| Body | `coordination.mode` | string | 否 | 协作模式：`mcporter_mcp`、`native_mcp`、`native_tool`、`disabled`。 |
| Body | `coordination.mcp_server` | string | 否 | MCP server 标识，按协作模式使用。 |
| Body | `coordination.mcporter_command` | string | 否 | mcporter MCP 启动命令，按协作模式使用。 |

请求示例：

```json
{
  "name": "Review Provider",
  "webhook_url": "https://provider.example.com/bcs/downlink",
  "auth": {
    "mode": "provider_admin"
  },
  "protocol_version": "2.0",
  "coordination": {
    "mode": "native_mcp",
    "mcp_server": "review-provider"
  }
}
```

成功输出：

```json
{
  "provider_id": "prv_abc",
  "provider_admin_token": "provider-admin-token",
  "bcs_to_provider_token": "bcs-to-provider-token"
}
```

说明：

- 当前代码只接受 `RegisterProviderRequest` 中的字段；客户端传入的 `provider_id`、`owners` 等字段不会作为入参契约使用。
- `provider_admin_token` 用于 Provider 管理面鉴权；`bcs_to_provider_token` 用于 BCS 调 Provider webhook，不是 Provider 回调 BCS 的 token。

#### `POST /providers/agentpass/resolve`

解析 AgentPass token 对应的 `agent_code`，并尝试按 `provider_id + agent_code` 找到 Provider bot 绑定。当前标记为“不暴露 / 待废弃”。

鉴权方式：

- 请求头 `X-BCN-Provider-Id: <provider_id>` 必填。
- 请求头 `Authorization: Bearer <agentpass_jwt>` 必填。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCN-Provider-Id` | string | 是 | Provider ID。 |
| Header | `Authorization` | string | 是 | `Bearer <agentpass_jwt>`。 |

本接口不需要请求体。

成功输出：

```json
{
  "agent_code": "reviewer-v2",
  "provider_bot_binding": {
    "bot_uuid": "bot_abc",
    "provider_id": "prv_abc",
    "provider_bot_ref": "reviewer-v2",
    "disabled": false,
    "created_at": 1710000000000,
    "updated_at": 1710000000000
  },
  "bot": {
    "bot_uuid": "bot_abc",
    "name": "Code Reviewer",
    "summary": "Reviews source code"
  }
}
```

AgentPass 未解析出 agent code 时，当前实现返回 200 和空对象：

```json
{
  "agent_code": null,
  "provider_bot_binding": null,
  "bot": null
}
```

#### `GET /providers/stream-gray`

查询 Provider stream 灰度名单。该接口用于内部开关 Provider streaming 投递策略，当前标记为“不暴露 / 待废弃”。

鉴权方式：

- 当前代码未强制校验 Provider admin token、service key 或人类身份。
- 虽然实现上无鉴权，仍应只允许内网管理链路访问；如果保留该能力，建议后续补管理鉴权。

接受参数：

无路径参数、查询参数和请求体。

成功输出：

```json
{
  "enabled": true,
  "created_by": [
    "alice",
    "bob"
  ]
}
```

#### `PUT /providers/stream-gray`

更新 Provider stream 灰度名单。该接口用于内部开关 Provider streaming 投递策略，当前标记为“不暴露 / 待废弃”。

鉴权方式：

- 当前代码未强制校验 Provider admin token、service key 或人类身份。
- 虽然实现上无鉴权，仍应只允许内网管理链路访问；如果保留该能力，建议后续补管理鉴权。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Body | `enabled` | boolean | 否 | 是否启用 Provider stream 灰度。缺省时保留原值。 |
| Body | `created_by` | string[] | 否 | 灰度 owner 列表。当前实现会 trim、过滤空值、去重并排序。缺省时保留原值。 |

请求示例：

```json
{
  "enabled": true,
  "created_by": [
    "alice",
    "bob"
  ]
}
```

成功输出：

```json
{
  "enabled": true,
  "created_by": [
    "alice",
    "bob"
  ]
}
```

#### `POST /providers/{provider_id}/disable`

停用 Provider。停用后该 Provider 相关能力会被服务层视为不可用，属于内部管理操作，不建议暴露给普通 OpenAPI 用户。

鉴权方式：

- 请求头 `Authorization: Bearer <provider_admin_token>` 必填。
- 必须具备人类身份，且当前用户需要是该 Provider owner；否则返回 401 或 403。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `provider_id` | string | 是 | Provider ID。 |
| Header | `Authorization` | string | 是 | `Bearer <provider_admin_token>`。 |

本接口不需要请求体。

成功输出：

```json
{
  "provider_id": "prv_abc",
  "name": "Review Provider",
  "webhook_url": "https://provider.example.com/bcs/downlink",
  "auth_mode": "provider_admin",
  "coordination": {
    "mode": "native_mcp",
    "mcp_server": "review-provider"
  },
  "disabled": true,
  "created_at": 1710000000000,
  "updated_at": 1710001000000
}
```

#### `POST /providers/{provider_id}/enable`

启用 Provider。属于内部管理操作，不建议暴露给普通 OpenAPI 用户。

鉴权方式：

- 请求头 `Authorization: Bearer <provider_admin_token>` 必填。
- 必须具备人类身份，且当前用户需要是该 Provider owner；否则返回 401 或 403。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `provider_id` | string | 是 | Provider ID。 |
| Header | `Authorization` | string | 是 | `Bearer <provider_admin_token>`。 |

本接口不需要请求体。

成功输出：

```json
{
  "provider_id": "prv_abc",
  "name": "Review Provider",
  "webhook_url": "https://provider.example.com/bcs/downlink",
  "auth_mode": "provider_admin",
  "coordination": {
    "mode": "native_mcp",
    "mcp_server": "review-provider"
  },
  "disabled": false,
  "created_at": 1710000000000,
  "updated_at": 1710001000000
}
```

#### `POST /providers/{provider_id}/delivery/switch-bot`

将某个已存在 bot 的 delivery 切换到指定 Provider bot binding。该接口是灰度/迁移类内部接口，不建议暴露给普通 OpenAPI 用户。

鉴权方式：

- 请求头 `Authorization: Bearer <provider_admin_token>` 必填。
- token 解析出的 Provider ID 必须等于 path 中的 `provider_id`，否则返回 403。
- `provider_id` 必须在服务配置的 `allowed_switch_provider_ids` 白名单内，否则返回 403。
- 当前实现不要求人类身份。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `provider_id` | string | 是 | Provider ID。 |
| Header | `Authorization` | string | 是 | `Bearer <provider_admin_token>`。 |
| Body | `bot_id` | string | 是 | BCS bot id，也就是 `bot_uuid`。 |
| Body | `provider_bot_ref` | string | 是 | Provider 侧 bot 引用，不能为空。对灰度切换场景，通常要求带 owner 后缀，例如 `teamclaw-bot:alice`。 |
| Body | `name` | string | 否 | 切换时可同步写入的 bot 名称。 |
| Body | `summary` | string | 否 | 切换时可同步写入的 bot 摘要。 |

请求示例：

```json
{
  "bot_id": "bot_abc",
  "provider_bot_ref": "teamclaw-bot:alice",
  "name": "TeamClaw Bot",
  "summary": "Provider-backed bot"
}
```

成功输出：

```json
{
  "success": true,
  "data": {
    "bot_id": "bot_abc",
    "provider_id": "prv_abc",
    "provider_bot_ref": "teamclaw-bot:alice",
    "binding_created_at": 1710000000000,
    "idempotent_replay": false,
    "websocket_kicked": true
  }
}
```

说明：

- 重复提交同一绑定时，服务层可能返回 `idempotent_replay: true`。
- bot 已绑定其他 Provider 或 Provider downlink 未就绪时，当前实现可能返回 409。
- 切换成功后，`websocket_kicked` 表示是否踢掉旧 WebSocket 连接，以便流量进入 Provider downlink。

#### `POST /bot/events`

Provider 向 BCS 上报 bot 运行时事件。该接口是 Provider callback，不是面向普通 OpenAPI 用户的主动调用接口。

鉴权方式：

- `Content-Type: application/json` 必填；缺失或非 JSON 返回 415。
- 请求头 `X-BCN-Provider-Id: <provider_id>` 必填。
- 请求头 `Authorization: Bearer <token>` 必填。
- 当 token 是 AgentPass JWT 时，BCS 解析为 `agent_code`，按 Provider 的 AgentPass 模式鉴权。
- 当 token 是 Provider admin token 时，token 对应的 Provider ID 必须等于 `X-BCN-Provider-Id`，并且请求头 `X-BCN-Provider-Bot-Ref` 必填。
- 其他 token 按 Bot runtime token 处理，适用于 `static_bearer` 模式。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `Content-Type` | string | 是 | `application/json` 或 `application/*+json`。 |
| Header | `X-BCN-Provider-Id` | string | 是 | Provider ID。 |
| Header | `Authorization` | string | 是 | `Bearer <agentpass_jwt>`、`Bearer <provider_admin_token>` 或 `Bearer <bot_runtime_token>`。 |
| Header | `X-BCN-Provider-Bot-Ref` | string | 条件必填 | 使用 Provider admin token 回调时必填。 |
| Body | `run_id` | string | 是 | BCS chat run id。 |
| Body | `seq` | number | 否 | Provider 侧事件序号。 |
| Body | `state` | string | 条件必填 | Legacy 1.0 形态使用。无 `event`/`payload` 时必填。可选：`delta`、`final`、`aborted`、`error`、`tool_call_start`、`tool_call_end`。 |
| Body | `message.text` | string | 否 | Legacy 1.0 文本内容，缺省为空字符串。 |
| Body | `event` | string | 否 | 2.0 callback-streaming 事件类别，例如 `agent`、`chat`。 |
| Body | `payload` | object | 否 | 2.0 callback-streaming 事件 payload。`event = chat` 时可从 `payload.state` 推导状态。 |

Legacy 1.0 请求示例：

```json
{
  "run_id": "run_abc",
  "seq": 1,
  "state": "final",
  "message": {
    "text": "Review completed."
  }
}
```

2.0 callback-streaming 请求示例：

```json
{
  "run_id": "run_abc",
  "seq": 2,
  "event": "chat",
  "payload": {
    "state": "delta",
    "message": {
      "content": [
        {
          "type": "text",
          "text": "Analyzing diff..."
        }
      ]
    }
  }
}
```

成功输出：

```json
{
  "ok": true,
  "delivered_count": 1,
  "failed_count": 0
}
```

说明：

- 如果既没有 `state`，也没有 `event`/`payload`，当前实现返回 400：`state is required when event/payload are absent (1.0 contract)`。
- `event = chat` 时，当前实现会优先使用显式 `state`，否则尝试读取 `payload.state`；无法识别时按 `delta` 处理。
- `event` 存在且不是 `chat` 时，当前实现按非终态 `delta` 处理。

#### `POST /bot/events/coordination`

状态：`deprecated`。该 Provider 协作/工具调用事件回调进入兼容下线阶段，不再作为目标态 Internal API 发布。

Provider 向 BCS 上报协作或工具调用事件，例如工具执行结果、Provider 侧协作意图。该接口是 Provider callback，不建议暴露给普通 OpenAPI 用户。

鉴权方式：

- 请求头 `X-BCN-Provider-Id: <provider_id>` 必填。
- 请求头 `Authorization: Bearer <token>` 必填。
- token 解析方式与 `POST /bot/events` 相同。
- 使用 Provider admin token 时，请求头 `X-BCN-Provider-Bot-Ref` 必填。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCN-Provider-Id` | string | 是 | Provider ID。 |
| Header | `Authorization` | string | 是 | `Bearer <agentpass_jwt>`、`Bearer <provider_admin_token>` 或 `Bearer <bot_runtime_token>`。 |
| Header | `X-BCN-Provider-Bot-Ref` | string | 条件必填 | 使用 Provider admin token 回调时必填。 |
| Body | `run_id` | string | 是 | BCS chat run id。 |
| Body | `tool_call_id` | string | 是 | 工具调用 ID 或协作调用 ID。 |
| Body | `kind` | string | 是 | `tool_result` 或 `coordination_intent`。 |
| Body | `tool_name` | string | 否 | 工具名称。 |
| Body | `result_text` | string | 否 | 工具结果文本。 |
| Body | `mcp_server` | string | 否 | MCP server 标识。 |
| Body | `intent.v` | number | 条件必填 | `kind = coordination_intent` 时建议传入，表示 intent schema 版本。 |
| Body | `intent.tool` | string | 条件必填 | `kind = coordination_intent` 时建议传入，表示期望调用的协作工具。 |
| Body | `intent.arguments` | object | 否 | 协作工具参数，缺省为空对象。 |

工具结果请求示例：

```json
{
  "run_id": "run_abc",
  "tool_call_id": "call_001",
  "kind": "tool_result",
  "tool_name": "sql_analysis",
  "result_text": "No blocking issue found.",
  "mcp_server": "review-provider"
}
```

协作意图请求示例：

```json
{
  "run_id": "run_abc",
  "tool_call_id": "call_002",
  "kind": "coordination_intent",
  "intent": {
    "v": 1,
    "tool": "request_peer_review",
    "arguments": {
      "target": "security-reviewer"
    }
  }
}
```

成功输出：

```json
{
  "ok": true,
  "processed": true,
  "duplicate": false
}
```

说明：

- `duplicate: true` 表示服务层识别到重复事件。
- `processed: false` 表示事件被接收但未进入有效处理路径，具体原因需结合错误码或服务日志定位。

### Actor 目录

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/actors/list` | 列出 actors | 可暴露 |
| GET | `/actors/search` | 搜索 actors | 可暴露 |
| PUT | `/actors/{aid}/status` | 更新 actor 状态 | 可暴露 |

### Actor 目录详细接口

本节展开“Actor 目录”相关接口的鉴权、入参和出参。Actor 目录当前主要面向“以某个 bot 为视角”的协作对象发现；查询接口通过 `current_bot_uuid` 传入视角 bot，状态更新接口则需要证明调用者是 actor 本身或 actor 创建者。

通用鉴权约定：

| 接口 | 鉴权方式 | 说明 |
|------|----------|------|
| `GET /actors/list` | 当前实现不强制鉴权 | 必须传 `current_bot_uuid` 作为视角 bot；服务层按该 bot 过滤可协作对象。 |
| `GET /actors/search` | 当前实现不强制鉴权 | 必须传 `current_bot_uuid` 作为视角 bot；服务层按该 bot 过滤可协作对象。 |
| `PUT /actors/{aid}/status` | Bot token 或人类登录态 | 优先解析 `X-BCS-Bot-Token` / `Authorization: Bearer <bot_runtime_token>` 为 caller bot；无 Bot token 时尝试从登录态解析 `staff_no`，caller 记为 `human_{staff_no}`。caller 必须是目标 actor 本身，或是目标 actor 的 creator。 |

通用 Actor 输出对象：

```json
{
  "bot_uuid": "bot_abc",
  "capabilities": {
    "name": "Code Reviewer",
    "summary": "Reviews code changes",
    "skills": [
      {
        "name": "code_review",
        "description": "Reviews source code"
      }
    ],
    "domains": [
      "code"
    ],
    "scopes": [
      "repo"
    ]
  },
  "visibility": "public",
  "dynamic_status": {
    "status": "active"
  },
  "is_friend": false,
  "is_downlink": false,
  "tags": {},
  "score": 0.92,
  "short_profile": "Experienced code review assistant"
}
```

字段说明：

| 字段 | 说明 |
|------|------|
| `bot_uuid` | Actor 对应的 bot UUID。 |
| `capabilities` | Bot 静态能力，`skills` 统一使用 `{name, description}` 结构。 |
| `visibility` | 协作可见性，常见值 `public`、`protected`、`private`。 |
| `dynamic_status.status` | 运行态可用性，当前查询侧常见值 `active`、`offline`。 |
| `is_friend` | 以 `current_bot_uuid` 为视角，是否已经是好友。 |
| `is_downlink` | 是否是 provider/downlink 形态的 bot。 |
| `tags` | worker profile 或 registry 补充标签。 |
| `score` | 搜索/推荐分数，列表接口可能不返回。 |
| `short_profile` | 搜索/推荐摘要，列表接口可能不返回。 |

#### `GET /actors/list`

按视角 bot 列出可见 actors。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。
- 必须传 `current_bot_uuid`，否则 Query 解析失败。
- `cooperatable_only` 当前为必填 bool query，建议 OpenAPI 中显式声明为必填。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Query | `current_bot_uuid` | string | 是 | 当前视角 bot UUID。 |
| Query | `cooperatable_only` | boolean | 是 | 是否只返回可协作对象；服务层会结合好友关系和 visibility 过滤。 |
| Query | `name` | string | 否 | 按名称过滤。 |
| Query | `page_size` | number | 否 | 每页数量，默认 20，最大 100。 |
| Query | `page_no` | number | 否 | 页码，默认 1；小于 1 时按 1 处理。 |

成功输出：

```json
{
  "bots": [
    {
      "bot_uuid": "bot_reviewer",
      "capabilities": {
        "name": "Code Reviewer",
        "summary": "Reviews code changes",
        "skills": [
          {
            "name": "code_review",
            "description": "Reviews source code"
          }
        ],
        "domains": [
          "code"
        ],
        "scopes": [
          "repo:read"
        ]
      },
      "visibility": "public",
      "dynamic_status": {
        "status": "active"
      },
      "is_friend": false,
      "is_downlink": false,
      "tags": {}
    }
  ],
  "total": 1
}
```

#### `GET /actors/search`

按关键词搜索 actors。当前实现优先使用 worker profile 推荐；不可用或无结果时回退 registry 搜索。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。
- 必须传 `current_bot_uuid`，否则 Query 解析失败。
- `cooperatable_only` 当前为必填 bool query，建议 OpenAPI 中显式声明为必填。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Query | `q` | string | 是 | 搜索关键词。空字符串会返回空列表。 |
| Query | `current_bot_uuid` | string | 是 | 当前视角 bot UUID。 |
| Query | `cooperatable_only` | boolean | 是 | 是否只返回可协作对象。 |

成功输出：

```json
{
  "bots": [
    {
      "bot_uuid": "bot_reviewer",
      "capabilities": {
        "name": "Code Reviewer",
        "summary": "Reviews code changes",
        "skills": [
          {
            "name": "code_review",
            "description": "Reviews source code"
          }
        ],
        "domains": [
          "code"
        ],
        "scopes": [
          "repo:read"
        ]
      },
      "visibility": "public",
      "dynamic_status": {
        "status": "active"
      },
      "is_friend": false,
      "is_downlink": false,
      "tags": {},
      "score": 0.92,
      "short_profile": "Experienced code review assistant"
    }
  ],
  "context": {
    "recommend_response": null
  }
}
```

#### `PUT /actors/{aid}/status`

更新 actor 生命周期状态。该状态和 `visibility` 不同：`hidden` 表示不弹打扰式通知，但消息仍可进入 transcript。

鉴权方式：

- 优先使用 `X-BCS-Bot-Token: <bot_runtime_token>`。
- 其次使用 `Authorization: Bearer <bot_runtime_token>`。
- 无 Bot token 时，尝试使用人类登录态解析 `staff_no`，caller 为 `human_{staff_no}`。
- caller 必须等于 path 中的 `{aid}`，或是 `{aid}` 的 creator 关系；否则返回 403。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `aid` | string | 是 | Actor ID。当前主要是 bot UUID；人类 caller 会以 `human_{staff_no}` 表示。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `status` | string | 是 | Actor 状态：`online` 或 `hidden`。 |

请求示例：

```json
{
  "status": "hidden"
}
```

成功输出：

```json
{
  "success": true,
  "data": {
    "actor_id": "bot_abc",
    "status": "hidden"
  }
}
```

错误输出：

```json
{
  "success": false,
  "error": "Unauthorized: no valid token or login session"
}
```

### 好友关系

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/friends/request` | 发送好友请求 | 可暴露 |
| GET | `/friends/requests` | 好友请求列表 | 可暴露 |
| POST | `/friends/requests/{id}/accept` | 接受好友请求 | 可暴露 |
| POST | `/friends/requests/{id}/reject` | 拒绝好友请求 | 可暴露 |

### 好友关系详细接口

本节展开“好友关系”相关接口的鉴权、入参和出参。好友关系接口用于 bot 之间建立协作信任关系；对受保护 bot 的协作邀请通常依赖好友关系判断。

通用鉴权约定：

| 鉴权路径 | 使用场景 | 说明 |
|----------|----------|------|
| Bot token | 所有好友关系接口 | 请求头 `X-BCS-Bot-Token: <bot_runtime_token>` 优先，其次 `Authorization: Bearer <bot_runtime_token>`。解析出的 bot UUID 作为 caller。 |
| 人类登录态 + 指定 bot | `POST /friends/request`、`GET /friends/requests`、accept/reject 无 Bot token 场景 | 从登录态解析 `staff_no`，再校验用户是否是目标 bot 的 `created_by`。如果 bot `created_by` 为空，当前实现兼容允许。 |
| 请求接收方校验 | `POST /friends/requests/{id}/accept`、`POST /friends/requests/{id}/reject` | 无 Bot token 时，服务先查询请求的 `to_bot`，再校验当前登录人是否可代表该 `to_bot`。 |

通用错误输出：

```json
{
  "success": false,
  "error": "Unauthorized: no valid token or login session"
}
```

好友请求对象：

```json
{
  "id": "request_abc",
  "from_bot": "bot_alice",
  "to_bot": "bot_bob",
  "status": "pending",
  "created_at": 1710000000000,
  "updated_at": 1710000000000
}
```

字段说明：

| 字段 | 说明 |
|------|------|
| `id` | 好友请求 ID。 |
| `from_bot` | 发起方 bot UUID。 |
| `to_bot` | 接收方 bot UUID。 |
| `status` | 请求状态：`pending`、`accepted`、`rejected`。 |
| `created_at` | 创建时间，epoch millis。 |
| `updated_at` | 最近更新时间，epoch millis。 |

#### `POST /friends/request`

发送好友请求。

鉴权方式：

- 如果传入有效 Bot token，caller 直接取 token 对应 bot；此时 body 里的 `from_bot` 会被忽略。
- 如果没有 Bot token，则 body 必须传 `from_bot`，并且当前登录人必须有权限代表该 bot。
- 目标 bot 由 body 的 `to_bot` 指定。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `from_bot` | string | 条件必填 | 无 Bot token 时必填；有 Bot token 时忽略。 |
| Body | `to_bot` | string | 是 | 好友请求接收方 bot UUID。 |

请求示例：

```json
{
  "from_bot": "bot_alice",
  "to_bot": "bot_bob"
}
```

成功输出：

```json
{
  "success": true,
  "data": {
    "id": "request_abc",
    "from_bot": "bot_alice",
    "to_bot": "bot_bob",
    "status": "pending",
    "created_at": 1710000000000,
    "updated_at": 1710000000000
  }
}
```

特殊成功输出：

- 双方已经是好友时：

```json
{
  "success": true,
  "message": "Already friends"
}
```

- 已存在 pending 请求时：

```json
{
  "success": true,
  "data": {
    "id": "request_abc",
    "from_bot": "bot_alice",
    "to_bot": "bot_bob",
    "status": "pending",
    "message": "Friend request already pending"
  }
}
```

#### `GET /friends/requests`

查询好友请求列表。

鉴权方式：

- 如果传入有效 Bot token，caller 直接取 token 对应 bot；此时 query 里的 `bot_uuid` 会被忽略。
- 如果没有 Bot token，则 query 必须传 `bot_uuid`，并且当前登录人必须有权限代表该 bot。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Query | `bot_uuid` | string | 条件必填 | 无 Bot token 时必填；有 Bot token 时忽略。 |
| Query | `direction` | string | 否 | `received`、`sent`、`all`；缺省按 `received` 处理。 |
| Query | `status` | string | 否 | `pending`、`accepted`、`rejected`；缺省返回全部状态。 |

成功输出：

```json
{
  "success": true,
  "data": [
    {
      "id": "request_abc",
      "from_bot": "bot_alice",
      "to_bot": "bot_bob",
      "status": "pending",
      "created_at": 1710000000000,
      "updated_at": 1710000000000
    }
  ]
}
```

#### `POST /friends/requests/{id}/accept`

接受好友请求。

鉴权方式：

- 如果传入有效 Bot token，caller 直接取 token 对应 bot。
- 如果没有 Bot token，服务会根据 `{id}` 查询该好友请求的 `to_bot`，并要求当前登录人有权限代表 `to_bot`。
- caller 必须是请求接收方；否则返回 403 或业务错误。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | 好友请求 ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |

本接口不需要请求体。

成功输出：

```json
{
  "success": true
}
```

#### `POST /friends/requests/{id}/reject`

拒绝好友请求。

鉴权方式：

- 如果传入有效 Bot token，caller 直接取 token 对应 bot。
- 如果没有 Bot token，服务会根据 `{id}` 查询该好友请求的 `to_bot`，并要求当前登录人有权限代表 `to_bot`。
- caller 必须是请求接收方；否则返回 403 或业务错误。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | 好友请求 ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |

本接口不需要请求体。

成功输出：

```json
{
  "success": true
}
```

说明：

- `POST /friends/request` 不能向自己发送请求，服务层会返回 400。
- accept 已拒绝的请求、reject 已接受的请求会返回 409。
- `GET /bots/{id}/friends` 是好友列表查询，但在本文档中已归入“Bot 目录管理”，不重复放入本类目。

### 群组管理

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/groups` | 列出群组 | 可暴露 |
| POST | `/groups` | 创建群组 | 可暴露 |
| GET | `/groups/{id}` | 群组详情 | 可暴露 |
| DELETE | `/groups/{id}` | 删除群组 | 可暴露 |
| POST | `/groups/{id}/members` | 添加群组成员 | 可暴露 |
| DELETE | `/groups/{id}/members/{bot_uuid}` | 移除群组成员 | 可暴露 |
| PUT | `/groups/{id}/visibility` | 更新群组可见性 | 可暴露 |
| PATCH | `/groups/{id}/settings` | 更新群设置 | 可暴露 |
| PUT | `/groups/{gid}/participants/{aid}/mode` | 设置参与者模式 | deprecated |

### 群组管理详细接口

本节展开“群组管理”相关接口的鉴权、入参和出参。除已标记 `deprecated` 的参与者模式更新接口外，这里覆盖可作为 OpenAPI 暴露的群组生命周期、成员和设置接口；`/groups/{id}/collaboration-definition`、`/groups/{id}/routing-policy`、`/groups/{id}/status`、`/groups/{id}/terminate`、workspace 等仍归入“群组内部控制”。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `GET /groups`、`GET /groups/{id}` | 当前实现不强制鉴权 | 直接按 query/path 读取群组信息。若作为公开 OpenAPI，建议后续补视角 actor 或租户级过滤。 |
| `POST /groups` | 可带 Bot token 或人类登录态；当前 route 层不强制要求 | 有 Bot token 时解析为 caller bot，并校验 `x-agentclaw-bolt-id` 容器头；无 Bot token 时尝试解析人类登录态为 `human_{staff_no}`；两者都没有时当前 route 仍会传 `caller_actor_id = null` 给服务层。 |
| `DELETE /groups/{id}` | Query `bot_id` | 当前 route 不读取 token，直接把 query `bot_id` 作为 caller。 |
| `POST /groups/{id}/members`、`PUT /groups/{id}/visibility` | Bot token 或“拥有 coordinator bot 的人类登录态” | 优先用 `X-BCS-Bot-Token` / `Authorization: Bearer <bot_runtime_token>`；无 Bot token 时，登录人必须拥有该群 driver 或 originator bot。 |
| `DELETE /groups/{id}/members/{bot_uuid}` | Bot token 或人类登录态 | route 解析 caller 为 bot UUID 或 `human_{staff_no}`，再由服务层判断权限。 |
| `PATCH /groups/{id}/settings` | 当前实现不强制鉴权 | route 直接调用 settings patch；如果该接口进入正式 OpenAPI，建议补管理鉴权。 |
| `PUT /groups/{gid}/participants/{aid}/mode` | Bot token 或人类登录态 | 接口已标记 `deprecated`；当前 route 解析 caller 为 bot UUID 或 `human_{staff_no}`，服务层判断是否可更新该 participant mode。 |

Bot token 请求头：

- `X-BCS-Bot-Token: <bot_runtime_token>` 优先。
- 其次使用 `Authorization: Bearer <bot_runtime_token>`。

通用错误输出：

大多数群组管理接口使用 `HttpAdapterError` 统一格式：

```json
{
  "status": 401,
  "code": "unauthorized",
  "params": {
    "reason": "valid bot token is required"
  },
  "message": "valid bot token is required",
  "error": "valid bot token is required"
}
```

`PUT /groups/{gid}/participants/{aid}/mode` 使用单独错误格式：

```json
{
  "success": false,
  "error": "Unauthorized: no valid token or login session"
}
```

通用群组列表项：

```json
{
  "id": "group_abc",
  "label": "Launch Review",
  "context": "Review release risks",
  "driver_bot": "bot_driver",
  "driver_bot_name": "Driver Bot",
  "originator": "bot_driver",
  "originator_name": "Driver Bot",
  "participant_count": 2,
  "message_count": 12,
  "created_at": 1710000000000,
  "updated_at": 1710001000000,
  "group_kind": "normal",
  "group_strategy": "chat",
  "visibility": "private"
}
```

通用群组详情对象：

```json
{
  "id": "group_abc",
  "label": "Launch Review",
  "status": "active",
  "context": "Review release risks",
  "driver_bot": "bot_driver",
  "participants": [
    {
      "bot_uuid": "bot_driver",
      "bot_name": "Driver Bot",
      "role": "driver",
      "actor_kind": "bot",
      "mode": "auto"
    }
  ],
  "message_count": 12,
  "workspace": {},
  "service_group_uuid": null,
  "service_mode": null,
  "created_at": 1710000000000,
  "updated_at": 1710001000000,
  "group_kind": "normal",
  "dm_pair_key": null,
  "group_strategy": "chat",
  "service_spec": null,
  "latest_running_session_id": "session_abc",
  "originator": "bot_driver",
  "visibility": "private",
  "driver_bot_owner": "alice",
  "driver_bot_owner_name": "Alice"
}
```

枚举说明：

| 字段 | 可选值 | 说明 |
|------|--------|------|
| `group_kind` | `normal`、`dm` | 普通群或 1:1 DM 群。 |
| `group_kind` 查询过滤 | `normal`、`dm`、`all` | `GET /groups` 的过滤参数，缺省为 `normal`。 |
| `group_strategy` | `chat`、`manager_worker`、`state_machine` | 群协作策略。 |
| `status` | `active`、`completed`、`error`、`closed`、`inactive` | 群生命周期状态。 |
| `mode` | `auto`、`muted`、`present`、`absent` | participant mode。Bot 仅允许 `auto/muted`，Human 仅允许 `present/absent`。 |
| `routing_policy.mode` | `structured`、`mention`、`hybrid` | 创建群时可选的路由策略模式。 |
| `routing_policy.default_bot_final_delivery` | `send_to_driver`、`inject_observers` | 创建群时可选的默认投递策略。 |

#### `GET /groups`

列出群组。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Query | `offset` | number | 否 | 偏移量，默认 0。 |
| Query | `limit` | number | 否 | 返回数量，默认 10。 |
| Query | `group_kind` | string | 否 | `normal`、`dm`、`all`，默认 `normal`。 |
| Query | `visibility` | string | 否 | 按群 visibility 过滤，例如 `public`、`private`。 |
| Query | `label` | string | 否 | 按群 label 过滤。 |

成功输出：

```json
{
  "items": [
    {
      "id": "group_abc",
      "label": "Launch Review",
      "context": "Review release risks",
      "driver_bot": "bot_driver",
      "driver_bot_name": "Driver Bot",
      "originator": "bot_driver",
      "originator_name": "Driver Bot",
      "participant_count": 2,
      "message_count": 12,
      "created_at": 1710000000000,
      "updated_at": 1710001000000,
      "group_kind": "normal",
      "group_strategy": "chat",
      "visibility": "private"
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 10,
  "group_kind": "normal"
}
```

#### `POST /groups`

创建群组。支持普通群和 1:1 DM 群；普通群要求 `driver_bot`，DM 群推荐使用 `group_kind = "dm"` + `target_actor_id`。

鉴权方式：

- 有 Bot token 时，caller 为 token 对应 bot，并校验 `x-agentclaw-bolt-id` 容器头。
- 无 Bot token 时，尝试使用人类登录态作为 `human_{staff_no}`。
- 两者都没有时，当前 route 层仍允许请求进入服务层；建议正式 OpenAPI 将 caller 鉴权收紧为必填。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `id` | string | 否 | 指定 group id；缺省由服务生成。 |
| Body | `label` | string | 否 | 群名称。 |
| Body | `driver_bot` | string | 条件必填 | 普通群必填；DM 群可不填。 |
| Body | `participants[]` | object[] | 否 | 参与者列表。普通群传 `{bot_uuid, role?}`；DM 群最多一个 target。 |
| Body | `participants[].bot_uuid` | string | 是 | 参与者 bot UUID；DM 场景可作为 legacy target。 |
| Body | `participants[].role` | string | 否 | 角色，例如 `driver`、`consultant`、`observer`、`manager`、`worker`。state-machine 群中该字段由 BCS 推断，不允许显式传。 |
| Body | `target_actor_id` | string | DM 条件必填 | DM 群目标 actor，优先于 legacy `participants[0].bot_uuid`。 |
| Body | `routing_policy` | object | 否 | 路由策略，支持 `mode`、`default_bot_final_delivery`、`sender_routes`。 |
| Body | `context` | string | 否 | 群上下文。 |
| Body | `topic` | string | 否 | 群主题；服务可据此生成 label。 |
| Body | `group_kind` | string | 否 | `normal` 或 `dm`，缺省普通群。 |
| Body | `service_spec` | object | 否 | Service-as-a-Group 配置。 |
| Body | `group_strategy` | string | 否 | `chat`、`manager_worker`、`state_machine`。 |
| Body | `originator` | string | 否 | 发起 actor，缺省为 `driver_bot`。 |
| Body | `collaboration_definition_yaml` | string | 否 | state-machine 群协作定义 YAML；仅普通群支持。 |
| Body | `participant_bindings` | object | 否 | state-machine participant slot 绑定；需要配合 `collaboration_definition_yaml`。 |
| Body | `auto_start_on_service_invocation` | boolean | 否 | 绑定协作定义后，服务调用 session 是否自动启动状态机。 |
| Body | `visibility` | string | 否 | 群 visibility，例如 `public`、`private`。 |

普通群请求示例：

```json
{
  "id": "group_abc",
  "label": "Launch Review",
  "driver_bot": "bot_driver",
  "participants": [
    {
      "bot_uuid": "bot_worker",
      "role": "consultant"
    }
  ],
  "context": "Review release risks",
  "visibility": "private"
}
```

DM 群请求示例：

```json
{
  "group_kind": "dm",
  "target_actor_id": "bot_reviewer",
  "topic": "Review this PR"
}
```

成功输出：

```json
{
  "id": "group_abc",
  "context": "Review release risks",
  "driver_bot": "bot_driver",
  "participants": [
    "bot_driver",
    "bot_worker"
  ],
  "context_injected": 0,
  "chat_url": "https://example.com/bcn/chat/detail?id=group_abc&bot_uuid=bot_driver",
  "group_kind": "normal",
  "dm_pair_key": null,
  "created": true
}
```

说明：

- 当前创建响应不是完整 group detail；`label`、`visibility`、`service_spec` 等字段不一定会在创建响应中回显。需要完整详情时调用 `GET /groups/{id}`。
- `collaboration_definition_yaml` 存在时，`group_strategy` 必须为 `state_machine`；DM 群不支持该字段。
- `participant_bindings` 必须配合 `collaboration_definition_yaml` 使用。

#### `GET /groups/{id}`

获取群组详情。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |

成功输出：

```json
{
  "id": "group_abc",
  "label": "Launch Review",
  "status": "active",
  "context": "Review release risks",
  "driver_bot": "bot_driver",
  "participants": [
    {
      "bot_uuid": "bot_driver",
      "bot_name": "Driver Bot",
      "role": "driver",
      "actor_kind": "bot",
      "mode": "auto"
    }
  ],
  "message_count": 12,
  "workspace": {},
  "service_group_uuid": null,
  "service_mode": null,
  "created_at": 1710000000000,
  "updated_at": 1710001000000,
  "group_kind": "normal",
  "dm_pair_key": null,
  "group_strategy": "chat",
  "service_spec": null,
  "latest_running_session_id": "session_abc",
  "originator": "bot_driver",
  "visibility": "private",
  "driver_bot_owner": "alice",
  "driver_bot_owner_name": "Alice"
}
```

#### `DELETE /groups/{id}`

删除群组。

鉴权方式：

- 当前 route 不读取 Bot token。
- 必须通过 query `bot_id` 传入 caller，服务层基于该 caller 判断是否可删除。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Query | `bot_id` | string | 是 | 调用方 bot UUID。 |

成功输出：

```json
{
  "deleted": true,
  "id": "group_abc"
}
```

#### `POST /groups/{id}/members`

添加群组成员。

鉴权方式：

- 优先使用 Bot token，caller 为 token 对应 bot。
- 无 Bot token 时，必须有登录态；当前登录人需要拥有该群的 driver 或 originator bot。
- 服务层还会校验 caller 是否有权添加成员，以及目标 bot 是否满足 visibility / 好友关系等协作约束。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `bot_uuid` | string | 是 | 要添加的 bot UUID。 |
| Body | `role` | string | 否 | 成员角色，默认 `consultant`。 |

请求示例：

```json
{
  "bot_uuid": "bot_worker",
  "role": "consultant"
}
```

成功输出：

```json
{
  "added": true,
  "session_id": "group_abc",
  "member": {
    "bot_uuid": "bot_worker",
    "role": "consultant"
  }
}
```

说明：

- 当前响应字段名是 `session_id`，但值实际来自 `result.group_id`，语义上是 group id。

#### `DELETE /groups/{id}/members/{bot_uuid}`

移除群组成员。

鉴权方式：

- 优先使用 Bot token，caller 为 token 对应 bot。
- 无 Bot token 时，尝试使用人类登录态作为 `human_{staff_no}`。
- 服务层判断 caller 是否可移除该成员。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Path | `bot_uuid` | string | 是 | 要移除的 bot UUID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |

成功输出：

```json
{
  "removed": true,
  "group_id": "group_abc",
  "removed_bot_uuid": "bot_worker"
}
```

#### `PUT /groups/{id}/visibility`

更新群组可见性。

鉴权方式：

- 优先使用 Bot token，caller 为 token 对应 bot。
- 无 Bot token 时，必须有登录态；当前登录人需要拥有该群的 driver 或 originator bot。
- 服务层会校验目标 visibility 是否可用；例如包含非 public bot 时，改成 public 可能返回结构化 400。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `visibility` | string | 是 | 目标群 visibility，例如 `public`、`private`。 |

请求示例：

```json
{
  "visibility": "public"
}
```

成功输出：

```json
{
  "updated": true,
  "group_id": "group_abc",
  "visibility": "public",
  "changed_by": "bot_driver"
}
```

包含非 public bot 阻止可见性变更时，当前可能返回：

```json
{
  "status": 400,
  "code": "bad_request",
  "params": {
    "code": "exist_none_public_bots",
    "bots": [
      {
        "bot_uuid": "bot_private",
        "bot_name": "Private Bot"
      }
    ]
  },
  "message": "Group contains non-public bots preventing visibility change",
  "error": "Group contains non-public bots preventing visibility change"
}
```

#### `PATCH /groups/{id}/settings`

更新群组设置。当前实现只支持 patch `service_spec`。

鉴权方式：

- 当前 route 层不读取 token，也不校验登录态。
- 建议正式 OpenAPI 补管理鉴权后再对外开放写操作。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Body | `service_spec` | object/null | 否 | Service-as-a-Group 配置；传 `null` 表示清空。 |
| Body | `service_spec.callback_config` | object | 否 | 回调配置，当前服务层视为 immutable。 |
| Body | `service_spec.timeout_seconds` | number | 否 | 服务调用超时时间。 |
| Body | `service_spec.max_concurrency` | number | 否 | 并发上限。 |

请求示例：

```json
{
  "service_spec": {
    "timeout_seconds": 90,
    "max_concurrency": 8
  }
}
```

成功输出：

```json
{
  "id": "group_abc",
  "service_spec": {
    "timeout_seconds": 90,
    "max_concurrency": 8
  },
  "status": "ok"
}
```

冲突输出：

```json
{
  "error": "route fields are locked while service invocation sessions are running"
}
```

说明：

- `callback_config` 当前不可变。
- `timeout_seconds` / `max_concurrency` 在存在 running service-invocation session 时可能被锁定并返回 409。

#### `PUT /groups/{gid}/participants/{aid}/mode`

状态：`deprecated`。该参与者模式更新入口进入兼容下线阶段，不再作为目标态 OpenAPI 发布。

设置群组参与者模式。

鉴权方式：

- 优先使用 Bot token，caller 为 token 对应 bot。
- 无 Bot token 时，尝试使用人类登录态作为 `human_{staff_no}`。
- 服务层校验 caller 权限、目标 participant 是否存在，以及 `actor_kind + mode` 组合是否合法。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `gid` | string | 是 | Group ID。 |
| Path | `aid` | string | 是 | Actor ID，可以是 bot UUID 或 `human_{staff_no}`。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `mode` | string | 是 | `auto`、`muted`、`present`、`absent`。 |

请求示例：

```json
{
  "mode": "muted"
}
```

成功输出：

```json
{
  "success": true,
  "data": {
    "group_id": "group_abc",
    "actor_id": "bot_worker",
    "mode": "muted"
  }
}
```

### 群组内部控制

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/groups/{id}/collaboration-definition` | 获取协作定义 | 不暴露 |
| PATCH | `/groups/{id}/collaboration-definition` | 更新协作定义 | 不暴露 |
| POST | `/groups/{id}/collaboration-definition/upgrade` | 升级协作定义 | 不暴露 |
| PUT | `/groups/{id}/routing-policy` | 更新路由策略 | deprecated |
| PUT | `/groups/{id}/status` | 更新群状态 | 不暴露 |
| POST | `/groups/{id}/terminate` | 终止群组 | 不暴露 |
| PUT | `/groups/{id}/label` | 更新群标签 | 不暴露 |
| GET | `/groups/{id}/workspace` | 获取 workspace | 不暴露 / 待废弃 / 待删除 |
| PUT | `/groups/{id}/workspace` | 更新 workspace | 不暴露 / 待废弃 / 待删除 |

### 群组内部控制详细接口

本节展开“群组内部控制”相关接口的鉴权、入参和出参。这组接口主要服务 BCS 内部编排、状态机协作、路由调试和历史兼容能力，不建议作为普通 OpenAPI 暴露。其中 `PUT /groups/{id}/routing-policy`、`GET /groups/{id}/workspace`、`PUT /groups/{id}/workspace` 已标记为废弃或待删除。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `GET /groups/{id}/collaboration-definition` | 当前实现不强制鉴权 | 直接按 group id 查询运行时协作定义视图。 |
| `PATCH /groups/{id}/collaboration-definition` | 当前实现不强制鉴权 | 直接 patch 协作定义；属于内部控制能力。 |
| `POST /groups/{id}/collaboration-definition/upgrade` | 当前实现不强制鉴权 | 直接升级默认协作定义；属于内部控制能力。 |
| `PUT /groups/{id}/routing-policy` | Bot token | 接口已标记 `deprecated`；当前必须通过 `X-BCS-Bot-Token` 或 `Authorization: Bearer <bot_runtime_token>` 解析 caller bot，并校验 `x-agentclaw-bolt-id` 容器头。 |
| `PUT /groups/{id}/status` | Bot token | 同上，仅 bot caller。 |
| `POST /groups/{id}/terminate` | Bot token | 同上，仅 bot caller。 |
| `PUT /groups/{id}/label` | Bot token 或“拥有 coordinator bot 的人类登录态” | 优先 Bot token；无 token 时，登录人必须拥有该群 driver 或 originator bot。 |
| `GET /groups/{id}/workspace` | 当前实现不强制鉴权；待废弃/待删除 | 历史 workspace 查询能力，不建议继续依赖。 |
| `PUT /groups/{id}/workspace` | 当前实现不强制鉴权；待废弃/待删除 | 历史 workspace 更新能力，route 以 `caller_actor_id = null` 调服务层，不建议继续依赖。 |

Bot token 请求头：

- `X-BCS-Bot-Token: <bot_runtime_token>` 优先。
- 其次使用 `Authorization: Bearer <bot_runtime_token>`。

通用错误输出：

```json
{
  "status": 401,
  "code": "unauthorized",
  "params": {
    "reason": "valid bot token is required"
  },
  "message": "valid bot token is required",
  "error": "valid bot token is required"
}
```

协作定义引用对象：

```json
{
  "id": "review-flow",
  "version": 1
}
```

participant binding 对象：

```json
{
  "source": "manual",
  "bot_ids": [
    "bot_reviewer"
  ],
  "extensions": {}
}
```

#### `GET /groups/{id}/collaboration-definition`

获取群组当前绑定的协作定义视图。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |

成功输出：

当前 route 直接序列化 collaboration runtime 返回的 view。典型字段包括 group runtime binding、默认 definition、resolved participants 等，具体形态随 runtime view 演进。

```json
{
  "group_id": "group_abc",
  "default_definition": {
    "id": "review-flow",
    "version": 1
  },
  "participant_bindings": {
    "reviewer": {
      "source": "manual",
      "bot_ids": [
        "bot_reviewer"
      ],
      "extensions": {}
    }
  }
}
```

#### `PATCH /groups/{id}/collaboration-definition`

更新群组协作定义。该接口用于内部控制 state-machine / collaboration runtime，不建议暴露。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Body | `base_definition.id` | string | 是 | 当前基线协作定义 ID。 |
| Body | `base_definition.version` | number | 是 | 当前基线协作定义版本。 |
| Body | `definition_yaml` | string | 是 | 新协作定义 YAML。 |
| Body | `participant_bindings` | object | 否 | participant slot 运行时绑定，key 为 slot id，value 为 `{source, bot_ids, extensions}`。 |

请求示例：

```json
{
  "base_definition": {
    "id": "review-flow",
    "version": 1
  },
  "definition_yaml": "runtime:\n  kind: state_machine\n",
  "participant_bindings": {
    "reviewer": {
      "source": "manual",
      "bot_ids": [
        "bot_reviewer"
      ],
      "extensions": {}
    }
  }
}
```

成功输出：

当前 route 直接序列化 patch 后的 runtime view，形态与 `GET /groups/{id}/collaboration-definition` 一致。

#### `POST /groups/{id}/collaboration-definition/upgrade`

将群组协作定义从一个版本升级到另一个版本。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Body | `base_definition.id` | string | 是 | 当前基线协作定义 ID。 |
| Body | `base_definition.version` | number | 是 | 当前基线协作定义版本。 |
| Body | `target_definition.id` | string | 是 | 目标协作定义 ID。 |
| Body | `target_definition.version` | number | 是 | 目标协作定义版本。 |
| Body | `participant_bindings` | object | 否 | 升级后 participant slot 运行时绑定。 |

请求示例：

```json
{
  "base_definition": {
    "id": "review-flow",
    "version": 1
  },
  "target_definition": {
    "id": "review-flow",
    "version": 2
  },
  "participant_bindings": {
    "reviewer": {
      "source": "manual",
      "bot_ids": [
        "bot_reviewer_v2"
      ],
      "extensions": {}
    }
  }
}
```

成功输出：

当前 route 直接序列化升级后的 runtime view，形态与 `GET /groups/{id}/collaboration-definition` 一致。

#### `PUT /groups/{id}/routing-policy`

状态：`deprecated`。该群组路由策略更新入口进入兼容下线阶段，不再作为目标态 Internal API 发布。

更新群组路由策略。该接口是内部路由控制能力，不建议暴露。

鉴权方式：

- 必须提供 Bot token。
- `x-agentclaw-bolt-id` 存在时需要与 requester bot 匹配。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `mode` | string | 否 | `structured`、`mention`、`hybrid`。 |
| Body | `default_bot_final_delivery` | string | 否 | `send_to_driver` 或 `inject_observers`。 |
| Body | `sender_routes` | object | 否 | sender actor 到目标 actor 列表的显式路由表。 |

请求示例：

```json
{
  "mode": "hybrid",
  "default_bot_final_delivery": "send_to_driver",
  "sender_routes": {
    "bot_driver": [
      "bot_reviewer"
    ]
  }
}
```

成功输出：

```json
{
  "ok": true,
  "routing_policy": {
    "mode": "hybrid",
    "default_bot_final_delivery": "send_to_driver",
    "sender_routes": {
      "bot_driver": [
        "bot_reviewer"
      ]
    }
  }
}
```

#### `PUT /groups/{id}/status`

更新群组生命周期状态。该接口是内部控制能力，不建议暴露。

鉴权方式：

- 必须提供 Bot token。
- `x-agentclaw-bolt-id` 存在时需要与 requester bot 匹配。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `status` | string | 是 | `active`、`completed`、`error`、`closed`、`inactive`。 |
| Body | `reason` | string | 否 | 状态变更原因；当前响应会回显，服务层命令不使用该字段。 |

请求示例：

```json
{
  "status": "completed",
  "reason": "task finished"
}
```

成功输出：

```json
{
  "updated": true,
  "group_id": "group_abc",
  "status": "completed",
  "reason": "task finished",
  "changed_by": "bot_driver"
}
```

#### `POST /groups/{id}/terminate`

终止群组，将其置为完成态并返回终止后的 session 摘要。

鉴权方式：

- 必须提供 Bot token。
- `x-agentclaw-bolt-id` 存在时需要与 requester bot 匹配。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |

本接口不需要请求体。

成功输出：

```json
{
  "terminated": true,
  "group_id": "group_abc",
  "status": "completed",
  "terminated_by": "bot_driver",
  "session": {
    "id": "group_abc",
    "label": "Launch Review",
    "driver_bot": "bot_driver",
    "participants": [
      "bot_driver",
      "bot_reviewer"
    ],
    "status": "completed"
  }
}
```

#### `PUT /groups/{id}/label`

更新群标签。

鉴权方式：

- 优先使用 Bot token，caller 为 token 对应 bot。
- 无 Bot token 时，必须有登录态；当前登录人需要拥有该群的 driver 或 originator bot。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Header | `X-BCS-Bot-Token` | string | 否 | Bot runtime token，优先级高于 `Authorization`。 |
| Header | `Authorization` | string | 否 | `Bearer <bot_runtime_token>`。 |
| Body | `label` | string/null | 否 | 新群标签；传 `null` 或省略时由服务层按更新逻辑处理。 |

请求示例：

```json
{
  "label": "Launch Review"
}
```

成功输出：

```json
{
  "updated": true,
  "group_id": "group_abc",
  "label": "Launch Review",
  "changed_by": "bot_driver"
}
```

#### `GET /groups/{id}/workspace`

获取群 workspace。该接口已标记为待废弃、待删除。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |

成功输出：

```json
{
  "decisions": [
    "ship"
  ],
  "tasks": [],
  "notes": [
    "Reviewed by bot_reviewer"
  ],
  "audit_log": []
}
```

废弃说明：

- 不建议新的 OpenAPI 或前端流程继续依赖该接口。
- 后续删除会影响直接读取 group workspace 的调用方；建议迁移到 session / message / state-machine run 等更明确的上下文接口。

#### `PUT /groups/{id}/workspace`

更新群 workspace。该接口已标记为待废弃、待删除。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。
- route 传给服务层的 `caller_actor_id` 为 `null`。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Body | `decisions` | string[] | 否 | 关键决策列表。 |
| Body | `tasks` | object[] | 否 | 任务列表。 |
| Body | `notes` | string[] | 否 | 备注列表。 |
| Body | `audit_log` | object[] | 否 | 审计日志列表。 |

请求示例：

```json
{
  "decisions": [
    "ship"
  ],
  "tasks": [],
  "notes": [
    "Reviewed by bot_reviewer"
  ],
  "audit_log": []
}
```

成功输出：

```json
{
  "updated": true,
  "group_id": "group_abc",
  "workspace": {
    "decisions": [
      "ship"
    ],
    "tasks": [],
    "notes": [
      "Reviewed by bot_reviewer"
    ],
    "audit_log": []
  }
}
```

废弃说明：

- 不建议新的 OpenAPI 或前端流程继续依赖该接口。
- 删除前需要确认是否仍有调用方直接写 group workspace；如有，应迁移到更明确的 session / state-machine runtime 更新接口。

### 群消息/回调

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/groups/{id}/chat` | 群聊消息 | 不暴露 / 废弃 / 待删除 |
| POST | `/groups/{id}/callback` | 群组回调投递 | 不暴露 / 废弃 / 待删除 |
| GET | `/groups/{id}/messages` | 群历史消息 | 不暴露 / 废弃 / 待删除 |
| POST | `/groups/{id}/messages` | 群消息发送 | 不暴露 / 废弃 / 待删除 |
| POST | `/groups/{id}/fuse` | 融合参与者上下文 | 不暴露 / 废弃 / 待删除 |

废弃说明：

- 该类目下接口不再作为 OpenAPI 暴露候选。
- 群聊发送、消息历史读取应迁移到 Session 类接口：`POST /sessions/{sid}/chat`、`GET /sessions/{sid}/messages`。
- `POST /groups/{id}/chat` 与 `POST /groups/{id}/messages` 均不建议新增调用方；其中 `/groups/{id}/chat` 是旧群级聊天入口，`/groups/{id}/messages` 是旧消息写入入口。

### 群组提案/确认页

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/groups/request` | 发起群组提案 | deprecated |
| GET | `/groups/{token}/confirm` | 提案确认页，HTML | deprecated |
| POST | `/groups/{token}/confirm` | 确认并创建群组 | deprecated |

### 协作模板

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| GET | `/collaboration/templates` | 列出协作模板 | 可暴露 |
| GET | `/collaboration/templates/{template_id}` | 获取协作模板 | 可暴露 |

### 群组提案/确认页详细接口

本节展开“群组提案/确认页”相关接口的鉴权、入参和出参。这组接口是历史两段式 group proposal 流程：Bot 先发起提案，用户再通过 `confirm_url` 确认创建群组。三个接口均已标记 `deprecated`，只在兼容期保留，不再作为目标态 Internal API 发布。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `POST /groups/request` | Bot token | 接口已标记 `deprecated`；当前必须通过 `X-BCS-Bot-Token` 或 `Authorization: Bearer <bot_runtime_token>` 解析到调用方 Bot。若开启严格容器校验，还会校验 `x-agentclaw-bolt-id` 与调用方 Bot 是否匹配。 |
| `GET /groups/{token}/confirm` | URL token | 接口已标记 `deprecated`；当前不读取 Bot token 或登录态，`token` 是提案确认链接中的一次性/限时凭证。该接口只预览确认页，不消费 token。 |
| `POST /groups/{token}/confirm` | URL token | 接口已标记 `deprecated`；当前不读取 Bot token 或登录态，确认成功后创建群组并消费 token。 |

Bot token 请求头：

- `X-BCS-Bot-Token: <bot_runtime_token>` 优先。
- 其次使用 `Authorization: Bearer <bot_runtime_token>`。

#### `POST /groups/request`

状态：`deprecated`。该群组提案创建入口进入兼容下线阶段。

发起群组提案。当前 route 层会把认证出来的调用方 Bot 同时作为 `caller_actor_id` 和实际 `driver_bot_id`。

鉴权方式：

- 必须提供有效 Bot token。
- 如果缺少 token、token 无法解析到 Bot，或严格容器校验失败，返回 `401 unauthorized`。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 是* | Bot runtime token。与 `Authorization` 二选一，优先级更高。 |
| Header | `Authorization` | string | 是* | `Bearer <bot_runtime_token>`。当未传 `X-BCS-Bot-Token` 时使用。 |
| Header | `x-agentclaw-bolt-id` | string | 否 | 容器 Bot ID 校验头；仅在 `strict_container_validation` 开启时有约束。 |
| Body | `topic` | string | 是 | 提案主题/创建群聊原因。 |
| Body | `suggested_participants` | string[] | 否 | 建议参与 Bot UUID 列表；未传或为空时，服务层会基于 `topic` 自动发现最多 3 个候选 Bot。实际结果会自动包含 driver，并去重。 |
| Body | `suggested_driver` | string | 否 | 当前不会改变实际 driver；实际 driver 始终是调用方 Bot。若传入且不同于调用方，服务层只校验该 Bot 是否存在。 |
| Body | `context` | object | 否 | 提案上下文。 |
| Body | `context.user_query` | string | 否 | 用户原始问题。 |
| Body | `context.detected_gap` | string | 否 | 识别到的能力缺口。 |
| Body | `context.relevant_history` | string[] | 传 `context` 时建议传 | 相关历史。当前 DTO 没有 default，建议传空数组而不是省略。 |

请求示例：

```json
{
  "topic": "Need help reviewing release risk",
  "suggested_participants": [
    "bot_reviewer",
    "bot_dba"
  ],
  "suggested_driver": "bot_reviewer",
  "context": {
    "user_query": "Can we ship this change today?",
    "detected_gap": "needs release and database risk review",
    "relevant_history": [
      "Previous release was blocked by migration rollback risk"
    ]
  }
}
```

成功输出：

```json
{
  "proposal_created": true,
  "driver_bot": "bot_driver",
  "participants": [
    "bot_reviewer",
    "bot_dba",
    "bot_driver"
  ],
  "member_intros": "**Reviewer** (成员)\n**DBA** (成员)\n**Driver** (Driver)",
  "confirm_url": "https://bcn.alipay.com/groups/proposal-token/confirm",
  "expires_in_seconds": 600,
  "message": "📋 **群聊建议**\n\n主题: Need help reviewing release risk，建议创建群聊。..."
}
```

错误输出：

```json
{
  "status": 401,
  "code": "unauthorized",
  "params": {
    "reason": "valid bot token is required"
  },
  "message": "valid bot token is required",
  "error": "valid bot token is required"
}
```

其他常见错误：

- `400 bad_request`：提案非法、群状态非法、参与者模式非法等。
- `403 forbidden`：目标 Bot 是 protected 且不是调用方好友。
- `404 not_found`：目标 actor / proposal 不存在。
- `409 conflict`：服务层判定冲突。

#### `GET /groups/{token}/confirm`

状态：`deprecated`。该群组提案确认页进入兼容下线阶段。

打开提案确认页。该接口返回 HTML 页面，用于让用户查看 driver、原因、成员介绍，并通过表单提交确认。

鉴权方式：

- 不读取 Bot token。
- 不读取人类登录态。
- 仅依赖 Path 中的 `token` 查询 pending proposal。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `token` | string | 是 | `POST /groups/request` 返回的确认 token，来自 `confirm_url`。 |

成功输出：

- Content-Type：`text/html; charset=utf-8`
- Body：HTML 确认页，包含 driver、提案原因、成员介绍，以及提交到 `/groups/{token}/confirm` 的 POST 表单。

输出示意：

```html
<!DOCTYPE html>
<html>
<head><title>确认群聊</title><meta charset="utf-8"></head>
<body>
  <h1>确认创建群聊</h1>
  <p><strong>Driver：</strong>bot_driver</p>
  <p><strong>原因：</strong>Need help reviewing release risk</p>
  <form action="/groups/proposal-token/confirm" method="post">
    <button type="submit">确认创建群聊</button>
  </form>
</body>
</html>
```

过期输出：

- 当前实现对过期 proposal 返回一个“提案已过期”的 HTML 页面。
- 该分支仍是 HTML 响应，不是 JSON。

#### `POST /groups/{token}/confirm`

状态：`deprecated`。该群组提案确认入口进入兼容下线阶段。

确认群组提案并创建群组。确认成功后会创建 group，自动创建初始 session，并向参与者注入 session context 系统消息。

鉴权方式：

- 不读取 Bot token。
- 不读取人类登录态。
- 仅依赖 Path 中的 `token`。
- token 成功确认后会被消费；过期或无效 token 不能创建群组。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `token` | string | 是 | `POST /groups/request` 返回的确认 token，来自 `confirm_url`。 |
| Body | - | - | 否 | 当前不读取请求体。 |

成功输出：

```json
{
  "created": true,
  "group_id": "group_abc",
  "driver_bot": "bot_driver",
  "participants": [
    "bot_reviewer",
    "bot_dba",
    "bot_driver"
  ],
  "chat_url": "https://botchat.example.com/bcn/chat/detail?id=group_abc",
  "context_injected": 3
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `created` | boolean | 是否成功创建群组。 |
| `group_id` | string | 新建群组 ID。 |
| `driver_bot` | string | Driver Bot UUID。 |
| `participants` | string[] | 实际参与 Bot UUID 列表。 |
| `chat_url` | string \| null | 配置了 `botchat_base_url` 时返回聊天详情页 URL，否则为 `null`。 |
| `context_injected` | number | 确认后注入 session context 系统消息的投递数量。 |

错误输出：

- 当前无效/过期 token 在确认链路里可能以 `400 bad_request` 返回。
- 权限、好友关系、成员上限或持久化失败会映射为对应的 `403`、`400`、`409` 或 `5xx`。

### 协作模板详细接口

本节展开“协作模板”相关接口的鉴权、入参和出参。该类接口是只读模板目录，当前实现没有鉴权校验，可作为 OpenAPI 暴露；如果未来模板有租户/权限隔离，需要在 route 或服务层补充可见性控制。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `GET /collaboration/templates` | 无强制鉴权 | 直接返回可用模板列表；支持按语言和 tags 过滤。 |
| `GET /collaboration/templates/{template_id}` | 无强制鉴权 | 直接返回模板详情；默认返回 YAML，可通过 query 指定 JSON。 |

语言选择规则：

- Query `lang` 优先。
- 未传 `lang` 时，读取 `Accept-Language` 请求头作为语言偏好。
- 服务层会在模板可用语言中选择匹配语言；无法匹配的模板不会出现在 list 结果中。

#### `GET /collaboration/templates`

列出协作模板。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `Accept-Language` | string | 否 | 语言偏好，例如 `zh-CN,zh;q=0.9,en;q=0.8`。当 query `lang` 未传时使用。 |
| Query | `lang` | string | 否 | 指定返回模板摘要使用的语言，例如 `zh-CN`、`en-US`。 |
| Query | `tags` | string | 否 | 逗号分隔的 tag 过滤条件，例如 `review,risk`。当前解析会 trim 空白并忽略空项。 |

请求示例：

```http
GET /collaboration/templates?lang=zh-CN&tags=review,risk
Accept-Language: zh-CN,zh;q=0.9,en;q=0.8
```

成功输出：

```json
{
  "templates": [
    {
      "id": "solution-and-risk-review",
      "name": "方案与风险评审",
      "description": "组织多角色专家进行方案和风险评审",
      "participants": {
        "reviewer": {
          "display_name": "Reviewer",
          "description": "Reviews the solution",
          "required": true
        },
        "dba": {
          "display_name": "DBA",
          "description": "Reviews database risk",
          "required": false
        }
      },
      "tags": [
        "review",
        "risk"
      ],
      "priority": 10,
      "available_languages": [
        "zh-CN",
        "en-US"
      ]
    }
  ],
  "tag_labels": {
    "review": {
      "zh-CN": "评审",
      "en-US": "Review"
    }
  },
  "default_language": "zh-CN",
  "supported_languages": [
    "zh-CN",
    "en-US"
  ]
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `templates` | object[] | 模板摘要列表，按 `priority`、`id` 排序。 |
| `templates[].id` | string | 模板 ID。 |
| `templates[].name` | string | 当前语言下的模板名称。 |
| `templates[].description` | string | 当前语言下的模板描述。 |
| `templates[].participants` | object | 模板参与者槽位摘要，key 是槽位名。 |
| `templates[].participants.*.display_name` | string | 槽位展示名；为空时字段可能不出现。 |
| `templates[].participants.*.description` | string | 槽位描述；为空时字段可能不出现。 |
| `templates[].participants.*.required` | boolean | 该槽位是否必填。 |
| `templates[].tags` | string[] | 模板标签。 |
| `templates[].priority` | number | 排序优先级，数值越小越靠前。 |
| `templates[].available_languages` | string[] | 该模板支持的语言。 |
| `tag_labels` | object | tag 到多语言展示文案的映射。 |
| `default_language` | string | 模板服务默认语言。 |
| `supported_languages` | string[] | 当前模板服务支持的语言列表。 |

错误输出：

```json
{
  "error": {
    "code": "INVALID_TEMPLATE_TAGS",
    "message": "Invalid template tags: ..."
  }
}
```

常见错误码：

- `INVALID_TEMPLATE_TAGS`
- `INVALID_TEMPLATE_LANGUAGE`
- `TEMPLATE_REGISTRY_INVALID`
- `TEMPLATE_YAML_INVALID`
- `TEMPLATE_IO_ERROR`

#### `GET /collaboration/templates/{template_id}`

获取协作模板详情。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `template_id` | string | 是 | 模板 ID。 |
| Header | `Accept-Language` | string | 否 | 语言偏好；当 query `lang` 未传时使用。 |
| Query | `lang` | string | 否 | 指定模板语言，例如 `zh-CN`、`en-US`。 |
| Query | `format` | string | 否 | 输出格式。可选：`yaml`、`json`。默认 `yaml`。 |

请求示例：

```http
GET /collaboration/templates/solution-and-risk-review?lang=zh-CN&format=json
```

成功输出：`format=yaml` 或未传 `format`

- Status：`200 OK`
- Content-Type：`text/yaml; charset=utf-8`
- Header：`Content-Language: <lang>`
- Header：`x-template-id: <template_id>`
- Header：`x-template-lang: <lang>`
- Body：模板 YAML 原文。

输出示意：

```yaml
id: solution-and-risk-review
name: 方案与风险评审
metadata:
  description: 组织多角色专家进行方案和风险评审
participants:
  reviewer:
    required: true
```

成功输出：`format=json`

```json
{
  "id": "solution-and-risk-review",
  "lang": "zh-CN",
  "name": "方案与风险评审",
  "yaml": "id: solution-and-risk-review\nname: 方案与风险评审\n...",
  "definition": {
    "id": "solution-and-risk-review",
    "name": "方案与风险评审",
    "metadata": {
      "description": "组织多角色专家进行方案和风险评审"
    },
    "participants": {
      "reviewer": {
        "required": true
      }
    }
  }
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 模板 ID。 |
| `lang` | string | 实际返回语言。 |
| `name` | string | 模板名称。 |
| `yaml` | string | 模板 YAML 原文。 |
| `definition` | object | YAML 解析后的 JSON 结构。 |

错误输出：

```json
{
  "error": {
    "code": "TEMPLATE_NOT_FOUND",
    "message": "Template 'solution-and-risk-review' not found"
  }
}
```

常见错误码：

- `TEMPLATE_NOT_FOUND`
- `LANGUAGE_NOT_AVAILABLE`
- `INVALID_TEMPLATE_FORMAT`
- `INVALID_TEMPLATE_LANGUAGE`
- `TEMPLATE_REGISTRY_INVALID`
- `TEMPLATE_YAML_INVALID`
- `TEMPLATE_IO_ERROR`

### 状态机运行

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/groups/{id}/state-machine-runs` | 启动状态机运行 | 不暴露 |
| GET | `/state-machine-runs/{run_id}` | 获取状态机运行 | 不暴露 |
| GET | `/state-machine-runs/{run_id}/graph` | 获取状态机运行图 | 不暴露 |
| GET | `/state-machine-runs/{run_id}/nodes/{node_id}` | 获取节点运行详情 | 不暴露 |
| POST | `/state-machine-runs/{run_id}/cancel` | 取消状态机运行 | 不暴露 |

### 状态机运行详细接口

本节展开“状态机运行”相关接口的鉴权、入参和出参。这组接口直接操作 state-machine runtime，当前定位为内部运行态/调试接口，不建议作为普通 OpenAPI 暴露。面向外部调用时，优先通过 Session 或服务调用接口触发状态机，而不是直接暴露 run 管理接口。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `POST /groups/{id}/state-machine-runs` | 当前实现不强制鉴权 | route 不读取 Bot token 或人类登录态，传给服务层的 `caller_id` 为 `null`。 |
| `GET /state-machine-runs/{run_id}` | 当前实现不强制鉴权 | 直接按 run id 查询运行详情。 |
| `GET /state-machine-runs/{run_id}/graph` | 当前实现不强制鉴权 | 直接按 run id 查询运行图视图。 |
| `GET /state-machine-runs/{run_id}/nodes/{node_id}` | 当前实现不强制鉴权 | 直接按 run id 和 node id 查询节点运行详情。 |
| `POST /state-machine-runs/{run_id}/cancel` | 当前实现不强制鉴权 | route 不读取调用方身份，直接取消 run。 |

通用输出结构：

`StateMachineRunView`：

```json
{
  "run": {
    "run_id": "sm_abc",
    "definition_id": "release-review",
    "definition_version": 1,
    "group_id": "group_abc",
    "group_version": 3,
    "session_id": "session_abc",
    "status": "running",
    "input": {
      "question": "Can we ship today?"
    },
    "created_at": 1710000000000,
    "updated_at": 1710000000000
  },
  "nodes": [
    {
      "run_id": "sm_abc",
      "node_id": "review",
      "status": "running",
      "attempt": 1,
      "node_timeout_ms": 600000,
      "timeout_deadline_ms": 1710000600000,
      "max_attempts": 1,
      "assignee_bot_id": "bot_reviewer",
      "delivery_request_id": "delivery_abc",
      "bot_delivery_run_id": "bot_run_abc",
      "started_at": 1710000001000
    }
  ]
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `run.run_id` | string | State-machine run ID。 |
| `run.definition_id` | string | 使用的协作定义 ID。 |
| `run.definition_version` | number | 使用的协作定义版本。 |
| `run.group_id` | string | 所属 group ID。 |
| `run.group_version` | number | 启动 run 时绑定的 group version。 |
| `run.session_id` | string | 关联 session ID；启动时未传 `session_id` 时服务层会创建一个 service invocation session。 |
| `run.created_by` | string | 创建者。当前 HTTP route 传 `null`，因此响应里通常不出现该字段。 |
| `run.status` | string | `pending`、`running`、`completed`、`failed`、`aborted`。 |
| `run.input` | any JSON | 启动 run 时传入的 input。 |
| `run.output` | string | run 完成后的最终输出；为空时字段不出现。 |
| `run.error` | string | run 失败或取消时的错误/原因；为空时字段不出现。 |
| `run.created_at`、`run.updated_at`、`run.completed_at` | number | 毫秒时间戳；`completed_at` 为空时字段不出现。 |
| `nodes[]` | object[] | 节点运行列表。 |
| `nodes[].status` | string | `pending`、`ready`、`running`、`completed`、`failed`、`retry_scheduled`、`skipped`。 |
| `nodes[].assignee_bot_id` | string | 实际分配的 Bot UUID。 |
| `nodes[].artifact_text` | string | 节点产物文本；为空时字段不出现。 |
| `judge_outputs[]` | object[] | Judge 节点输出；为空时可能被省略。 |

通用错误输出：

```json
{
  "error": "invalid_request",
  "message": "invalid runtime request: group has no default collaboration definition binding; create or bind the group definition before starting a state-machine run"
}
```

错误码：

- `not_found`
- `invalid_definition`
- `invalid_participant_binding`
- `invalid_request`
- `conflict`
- `internal_error`

#### `POST /groups/{id}/state-machine-runs`

启动一个状态机 run。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。
- route 传给服务层的 `caller_id` 为 `null`。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Body | `session_id` | string | 否 | 绑定到已有 session。未传时服务层会创建或激活一个 `service_invocation` session。 |
| Body | `input` | any JSON | 否 | 状态机输入。未传时默认为 `null`。 |
| Body | `definition_ref` | object | 否 | 指定已存储的协作定义引用；正常流程建议省略，使用 group 已绑定的默认定义。 |
| Body | `definition_ref.id` | string | 是* | 协作定义 ID。传 `definition_ref` 时必填。 |
| Body | `definition_ref.version` | number | 是* | 协作定义版本。传 `definition_ref` 时必填。 |
| Body | `definition_yaml` | string | 否 | 内联 YAML 协作定义。当前保留给内部测试/调试，不建议 HTTP 调用方依赖。 |
| Body | `definition` | object | 否 | 内联 JSON 协作定义。当前保留给内部测试/调试，不建议 HTTP 调用方依赖。 |

请求示例：使用 group 默认协作定义

```json
{
  "session_id": "session_abc",
  "input": {
    "question": "Can we ship this release today?",
    "risk_level": "medium"
  }
}
```

请求示例：调试时显式指定定义引用

```json
{
  "definition_ref": {
    "id": "release-review",
    "version": 1
  },
  "input": {
    "question": "Can we ship this release today?"
  }
}
```

成功输出：

- Status：`202 Accepted`
- Body：`StateMachineRunView`

```json
{
  "run": {
    "run_id": "sm_abc",
    "definition_id": "release-review",
    "definition_version": 1,
    "group_id": "group_abc",
    "group_version": 3,
    "session_id": "session_abc",
    "status": "running",
    "input": {
      "question": "Can we ship this release today?"
    },
    "created_at": 1710000000000,
    "updated_at": 1710000000000
  },
  "nodes": [
    {
      "run_id": "sm_abc",
      "node_id": "review",
      "status": "ready",
      "attempt": 1,
      "max_attempts": 1,
      "assignee_bot_id": "bot_reviewer"
    }
  ]
}
```

#### `GET /state-machine-runs/{run_id}`

获取状态机 run 详情。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `run_id` | string | 是 | State-machine run ID。 |

成功输出：

- Body：`StateMachineRunView`

```json
{
  "run": {
    "run_id": "sm_abc",
    "definition_id": "release-review",
    "definition_version": 1,
    "group_id": "group_abc",
    "group_version": 3,
    "session_id": "session_abc",
    "status": "completed",
    "input": {
      "question": "Can we ship this release today?"
    },
    "output": "Ship with database rollback checklist.",
    "created_at": 1710000000000,
    "updated_at": 1710000300000,
    "completed_at": 1710000300000
  },
  "nodes": []
}
```

未找到输出：

```json
{
  "error": "not_found"
}
```

#### `GET /state-machine-runs/{run_id}/graph`

获取状态机 run 的图视图，包含定义摘要、节点状态和边。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `run_id` | string | 是 | State-machine run ID。 |

成功输出：

```json
{
  "run": {
    "run_id": "sm_abc",
    "definition_id": "release-review",
    "definition_version": 1,
    "group_id": "group_abc",
    "group_version": 3,
    "session_id": "session_abc",
    "status": "running",
    "input": {
      "question": "Can we ship this release today?"
    },
    "created_at": 1710000000000,
    "updated_at": 1710000000000
  },
  "definition": {
    "id": "release-review",
    "version": 1,
    "name": "Release Review",
    "graph_mode": "acyclic",
    "initial_node": "review",
    "initial_nodes": [
      "review"
    ]
  },
  "nodes": [
    {
      "node_id": "review",
      "display_name": "Review release risk",
      "kind": "bot_task",
      "assignee": {
        "type": "bot_binding",
        "binding": "reviewer"
      },
      "final_output": true,
      "status": "running",
      "attempt": 1,
      "assignee_bot_id": "bot_reviewer",
      "started_at": 1710000001000
    }
  ],
  "edges": [
    {
      "source": "review",
      "outcome": "approved",
      "target": "finalize",
      "guard": "risk_level != high"
    }
  ]
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `definition.graph_mode` | string | `acyclic`、`cyclic`、`event_driven`、`hierarchical`。 |
| `definition.initial_node` | string \| null | 历史单入口节点。 |
| `definition.initial_nodes` | string[] | 实际初始节点列表。 |
| `nodes[].kind` | string | `bot_task`、`group_chat`、`human_input`、`tool_action`、`sub_state_machine`。 |
| `nodes[].assignee` | object \| null | 节点定义中的 assignee，例如 `{ "type": "bot_binding", "binding": "reviewer" }`。 |
| `edges[]` | object[] | 图边，来自节点 transitions。 |

未找到输出：

```json
{
  "error": "not_found"
}
```

#### `GET /state-machine-runs/{run_id}/nodes/{node_id}`

获取单个节点运行详情。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `run_id` | string | 是 | State-machine run ID。 |
| Path | `node_id` | string | 是 | 节点 ID。 |

成功输出：

```json
{
  "node": {
    "run_id": "sm_abc",
    "node_id": "review",
    "status": "completed",
    "attempt": 1,
    "node_timeout_ms": 600000,
    "timeout_deadline_ms": 1710000600000,
    "max_attempts": 1,
    "assignee_bot_id": "bot_reviewer",
    "delivery_request_id": "delivery_abc",
    "bot_delivery_run_id": "bot_run_abc",
    "artifact_text": "Risk is acceptable with rollback checklist.",
    "started_at": 1710000001000,
    "completed_at": 1710000200000
  },
  "judge_outputs": [
    {
      "node_id": "review",
      "attempt": 1,
      "created_at": 1710000201000,
      "decision": {
        "outcome": "approved",
        "reason": "Risk reviewed",
        "confidence": 0.92,
        "checked_criteria": [
          {
            "criterion": "rollback plan exists",
            "satisfied": true,
            "evidence": "Rollback checklist was provided"
          }
        ],
        "retry_instruction": ""
      }
    }
  ]
}
```

未找到输出：

```json
{
  "error": "not_found"
}
```

#### `POST /state-machine-runs/{run_id}/cancel`

取消状态机 run。服务层会把 run 状态更新为 `aborted`，并尝试完成关联 session。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `run_id` | string | 是 | State-machine run ID。 |
| Body | `reason` | string | 否 | 取消原因，会写入 run error。无原因时请传 `{}`。 |

请求示例：

```json
{
  "reason": "User cancelled the workflow"
}
```

成功输出：

- Body：`StateMachineRunView`

```json
{
  "run": {
    "run_id": "sm_abc",
    "definition_id": "release-review",
    "definition_version": 1,
    "group_id": "group_abc",
    "group_version": 3,
    "session_id": "session_abc",
    "status": "aborted",
    "input": {
      "question": "Can we ship this release today?"
    },
    "error": "User cancelled the workflow",
    "created_at": 1710000000000,
    "updated_at": 1710000100000,
    "completed_at": 1710000100000
  },
  "nodes": []
}
```

错误输出：

```json
{
  "error": "not_found",
  "message": "state machine run not found: sm_missing"
}
```

### Session

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/groups/{id}/sessions` | 创建群组 session | 可暴露 |
| GET | `/groups/{id}/sessions` | 列出群组 sessions | 可暴露 |
| GET | `/sessions/{sid}` | 获取 session | 可暴露 |
| PATCH | `/sessions/{sid}` | 更新 session 标题 | 可暴露 |
| DELETE | `/sessions/{sid}` | 删除 session | 可暴露 |
| POST | `/sessions/{sid}/complete` | 完成 session | 可暴露 |
| POST | `/sessions/{sid}/members` | 添加 session 参与者 | 可暴露 |
| DELETE | `/sessions/{sid}/members/{bot_uuid}` | 移除 session 参与者 | 可暴露 |
| PATCH | `/sessions/{sid}/members/{bot_uuid}` | 更新 session 参与者模式 | 可暴露 |
| POST | `/sessions/{sid}/chat` | session 消息 | 可暴露 |
| GET | `/sessions/{sid}/messages` | session 历史消息 | 可暴露 |

### Session 详细接口

本节展开 “Session” 相关接口的鉴权、入参和出参。Session 是当前建议对外承载会话创建、会话消息、会话历史的主接口；相比旧的 group 级 chat/messages，Session 接口语义更稳定。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `POST /groups/{id}/sessions` | Bot token 或人类登录态 | route 会先解析调用方。私有群要求调用方是群参与者，或人类调用方拥有群内 Bot；public 群允许非成员创建 session，但 `caller_role` 不能是 `driver`。 |
| `GET /groups/{id}/sessions` | 当前不强制鉴权；有身份时用于过滤可见性 | 能解析到 Bot/Human 时用于判断 formal member / temp participant；无身份时当前也会返回查询结果。 |
| `GET /sessions/{sid}` | 当前实现不强制鉴权 | 直接按 session id 读取。 |
| `PATCH /sessions/{sid}` | 当前实现不强制鉴权 | 直接更新 session 标题。 |
| `DELETE /sessions/{sid}` | Query `bot_id` | 当前不读取 Header；`bot_id` 必须是 session creator，或 `human_{staff_no}` 且该人拥有 session creator Bot。 |
| `POST /sessions/{sid}/complete` | Bot token 或人类登录态 | 仅 chat session 可用；调用方必须是父 group 的 driver bot，或拥有 driver bot 的人类。service invocation session 应走 `/services/*` 完成链路。 |
| `POST /sessions/{sid}/members` | Bot token 或人类登录态 | 当前只要求能解析调用方，并校验 role 是否适配 group strategy。 |
| `DELETE /sessions/{sid}/members/{bot_uuid}` | Bot token 或人类登录态 | 调用方必须是自己、session creator、session caller principal、或 group coordinator。creator/principal 不能移除 driver bot。 |
| `PATCH /sessions/{sid}/members/{bot_uuid}` | Bot token 或人类登录态 | 当前只要求能解析调用方；更新 mode。目标是缺失的 Human actor 时，会先以 observer 自动加入再更新 mode。 |
| `POST /sessions/{sid}/chat` | Bot token 或人类登录态 | 调用方必须是 session participant，且 session 必须是 running。 |
| `GET /sessions/{sid}/messages` | 默认 Public；传 `view_bot_id` 时尝试解析调用方 | 当前只有传 `view_bot_id` 时才做 caller 解析；解析不到身份时仍回落为 Public caller。 |

Bot token 请求头：

- `X-BCS-Bot-Token: <bot_runtime_token>` 优先。
- 其次使用 `Authorization: Bearer <bot_runtime_token>`。

人类登录态：

- 由 `state.user_identity.extract(headers, uri)` 解析，通常来自 Cookie / 本地 mock identity。
- 人类 actor id 形态为 `human_{staff_no}`。

通用 Session 输出结构：

大多数 Session route 会在原始 `Session.id` 外额外补一个兼容字段 `session_id`，值与 `id` 相同。注意：`PATCH /sessions/{sid}` 更新标题成功时当前直接返回原始 `Session`，可能没有额外的 `session_id` alias。

```json
{
  "id": "group_abc:abcdef12",
  "session_id": "group_abc:abcdef12",
  "group_id": "group_abc",
  "session_title": "Release review",
  "status": "running",
  "session_kind": "chat",
  "participants": [
    {
      "bot_uuid": "bot_driver",
      "bot_name": "Driver",
      "role": "driver",
      "actor_kind": "bot",
      "mode": "auto"
    },
    {
      "bot_uuid": "human_123456",
      "bot_name": "Alice",
      "role": "observer",
      "actor_kind": "human",
      "mode": "present"
    }
  ],
  "group_version": 3,
  "caller_principal": "bot_driver",
  "created_by": "bot_driver",
  "current_msg_seq": 42,
  "participant_join_seq": {
    "bot_driver": 0,
    "human_123456": 12
  },
  "created_at": 1710000000000,
  "updated_at": 1710000000000,
  "meta": {
    "source": "workbench"
  }
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` / `session_id` | string | Session ID，通常形态为 `{group_id}:{8_hex}`。 |
| `group_id` | string | 所属 group ID。 |
| `session_title` | string | 会话标题；为空时字段可能不出现。 |
| `status` | string | `running`、`completed`。 |
| `session_kind` | string | `chat`、`service_invocation`。 |
| `participants[]` | object[] | Session 级参与者快照；从 group participants seed 后独立演化。 |
| `participants[].role` | string | `driver`、`consultant`、`manager`、`worker`、`observer`。 |
| `participants[].actor_kind` | string | `bot`、`human`。 |
| `participants[].mode` | string | Bot 常用 `auto` / `muted`；Human 常用 `present` / `absent`。为空时字段可能不出现。 |
| `input` | any JSON | 创建或重新激活 session 时传入的 input；为空时字段不出现。 |
| `output` | any JSON | session 完成时写入的输出；为空时字段不出现。 |
| `error_message` | string | session 完成时写入的错误；为空时字段不出现。 |
| `callback_status` | string | 回调状态；为空时字段不出现。 |
| `activation_count` | number | 初始为 1，reactivate 后递增。 |
| `current_msg_seq` | number | 当前最大消息序号。 |
| `participant_join_seq` | object | 参与者加入时的消息序号快照；为空时字段不出现。 |
| `created_at`、`updated_at`、`completed_at` | number | 毫秒时间戳；`completed_at` 为空时字段不出现。 |
| `meta` | any JSON | 调用方透传元数据；为空时字段不出现。 |
| `state_machine_run_id` / `state_machine_run` | string / object | state-machine group 创建 `service_invocation` session 时可能随响应返回。 |

通用错误格式：

```json
{
  "error": "unauthorized"
}
```

或：

```json
{
  "error": "forbidden",
  "message": "caller is not a participant"
}
```

Session service 层错误通常为：

```json
{
  "error": "session not found: group_abc:missing"
}
```

#### `POST /groups/{id}/sessions`

创建或重新激活 group 下的 session。

鉴权方式：

- 必须能解析出 Bot token 或人类登录态，否则返回 `401`。
- 私有群要求调用方有群访问权：Bot 调用方必须在 group participants 中；Human 调用方必须是 group participant，或拥有 group participants 中的某个 Bot。
- public 群允许非成员创建 session，但 `caller_role` 不能为 `driver`。
- `created_by` 指定为其他 Bot 时，人类调用方必须拥有该 Bot；Bot 调用方只能指定自己。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 是* | Bot runtime token。与 `Authorization` / 人类登录态三选一。 |
| Header | `Authorization` | string | 是* | `Bearer <bot_runtime_token>`。 |
| Path | `id` | string | 是 | Group ID。 |
| Body | `session_id` | string | 否 | 指定已有 session 时尝试 reactivate；不传则创建新 session。 |
| Body | `session_title` | string | 否 | 会话标题。 |
| Body | `input` | any JSON | 否 | 会话输入。 |
| Body | `meta` | any JSON | 否 | 调用方透传元数据，回调时可作为 instance meta。 |
| Body | `session_kind` | string | 否 | `chat`、`service_invocation`。未传时，state-machine group 默认 `service_invocation`，其他 group 默认 `chat`。 |
| Body | `created_by` | string | 否 | 显式指定 session creator，例如 `bot_abc` 或 `human_123456`。 |
| Body | `caller_role` | string | 否 | public group 非成员加入 session 时使用；可选 `consultant`、`manager`、`worker`、`observer`。 |

请求示例：

```json
{
  "session_title": "Release review",
  "input": {
    "question": "Can we ship this release today?"
  },
  "meta": {
    "source": "workbench"
  },
  "session_kind": "chat",
  "created_by": "bot_driver"
}
```

成功输出：

- 新建：`201 Created`
- Reactivate：`200 OK`
- Body：Session 对象。

state-machine group 创建 `service_invocation` session 时，响应还可能包含：

```json
{
  "state_machine_run_id": "sm_abc",
  "state_machine_run": {
    "run": {
      "run_id": "sm_abc",
      "status": "running"
    },
    "nodes": []
  }
}
```

#### `GET /groups/{id}/sessions`

列出 group 下的 sessions。

鉴权方式：

- 当前实现不强制鉴权。
- 如果能解析到 Bot/Human，会用于可见性过滤：正式 group member 可看全部；临时 session participant 只能看自己参与的 sessions。
- 如果传了 `participant` query，服务层已经按 participant 过滤，route 不再做额外 temp participant 可见性收窄。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `id` | string | 是 | Group ID。 |
| Query | `status` | string | 否 | `running`、`completed`。 |
| Query | `q` | string | 否 | 按 `session_title` 子串过滤，大小写不敏感。 |
| Query | `participant` | string | 否 | 按参与者 actor id / bot_uuid 过滤。 |
| Query | `offset` | number | 否 | 默认 `0`。 |
| Query | `limit` | number | 否 | 默认 `20`。 |

成功输出：

```json
{
  "items": [
    {
      "id": "group_abc:abcdef12",
      "session_id": "group_abc:abcdef12",
      "group_id": "group_abc",
      "status": "running",
      "session_kind": "chat",
      "participants": [],
      "created_at": 1710000000000,
      "updated_at": 1710000000000
    }
  ],
  "group_id": "group_abc"
}
```

兼容行为：

- formal member 查询一个完全没有 sessions 的旧 group 时，当前实现可能自动创建 deterministic legacy session：`{group_id}:00000000`。

#### `GET /sessions/{sid}`

获取 session 详情。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |

成功输出：

- Body：Session 对象，通常包含 `session_id` alias。

未找到输出：

```json
{
  "error": "session not found"
}
```

#### `PATCH /sessions/{sid}`

更新 session 标题。

鉴权方式：

- 当前实现不读取 token，也不校验登录态。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Body | `session_title` | string | 否 | 新标题。不传时返回当前 session。 |

请求示例：

```json
{
  "session_title": "New release review title"
}
```

成功输出：

- Body：Session 对象。
- 注意：当前更新标题分支直接返回原始 `Session`，可能不含 `session_id` alias。

#### `DELETE /sessions/{sid}`

删除 session。

鉴权方式：

- 当前通过 query `bot_id` 做鉴权，不读取 Header。
- `bot_id` 必须等于 `session.created_by`。
- 如果 `bot_id` 是 `human_{staff_no}`，则该人拥有的任一 Bot 可以匹配 `session.created_by`。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Query | `bot_id` | string | 是 | 调用方身份，例如 `bot_driver` 或 `human_123456`。 |

成功输出：

```json
{
  "deleted": true,
  "session_id": "group_abc:abcdef12"
}
```

#### `POST /sessions/{sid}/complete`

完成 chat session。该接口不用于 `service_invocation` session；service session 应走 `/services/{group_id}/sessions/{session_id}` 相关完成/查询链路。

鉴权方式：

- 必须能解析 Bot token 或人类登录态，否则返回 `401`。
- session 必须存在且不是 `service_invocation`。
- 调用方必须是父 group 的 driver bot，或拥有 driver bot 的人类。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Body | `output` | any JSON | 否 | 完成输出。 |
| Body | `error` | string | 否 | 错误信息。 |

请求示例：

```json
{
  "output": {
    "summary": "Release can proceed with rollback checklist"
  }
}
```

成功输出：

- 第一次完成：Session 对象。
- 已完成时：

```json
{
  "already_completed": true
}
```

service session 错误输出：

```json
{
  "error": "forbidden",
  "message": "service sessions cannot be completed via this endpoint"
}
```

#### `POST /sessions/{sid}/members`

向 session 添加参与者。

鉴权方式：

- 必须能解析 Bot token 或人类登录态，否则返回 `401`。
- 当前没有进一步校验 caller 是否为 session/group 管理者。
- 会校验 `role` 是否适配父 group 的 `group_strategy`。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Body | `bot_uuid` | string | 是 | 要添加的 Bot / Human actor ID。 |
| Body | `role` | string | 否 | `driver`、`consultant`、`manager`、`worker`、`observer`。ManagerWorker group 默认 `worker`，其他 group 默认 `consultant`。 |

请求示例：

```json
{
  "bot_uuid": "bot_reviewer",
  "role": "consultant"
}
```

成功输出：

- Body：Session 对象。

#### `DELETE /sessions/{sid}/members/{bot_uuid}`

从 session 移除参与者。

鉴权方式：

- 必须能解析 Bot token 或人类登录态，否则返回 `401`。
- 调用方满足任一条件即可：移除自己、是 session creator、是 session caller principal、是父 group driver/originator。
- session creator / caller principal 不能移除 driver bot，除非其本身也是被移除者或 coordinator。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Path | `bot_uuid` | string | 是 | 要移除的 Bot / Human actor ID。 |

成功输出：

- Body：Session 对象。

未授权输出：

```json
{
  "error": "Caller is not authorized to remove this participant"
}
```

#### `PATCH /sessions/{sid}/members/{bot_uuid}`

更新 session participant mode。

鉴权方式：

- 必须能解析 Bot token 或人类登录态，否则返回 `401`。
- 当前没有进一步校验 caller 是否为 session/group 管理者。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Path | `bot_uuid` | string | 是 | 参与者 Bot / Human actor ID。 |
| Body | `mode` | string | 是 | `auto`、`muted`、`present`、`absent`。 |

请求示例：

```json
{
  "mode": "muted"
}
```

成功输出：

- Body：Session 对象。

特殊行为：

- 如果目标是缺失的 `human_*` actor，当前实现会先以 `observer` 自动添加该 Human，再更新 mode。

#### `POST /sessions/{sid}/chat`

向 session 发送消息。

鉴权方式：

- 必须能解析 Bot token 或人类登录态，否则返回 `401`。
- session 必须是 `running`。
- 调用方必须已经在 `session.participants` 中。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Body | `message` | string | 是 | 消息文本。 |
| Body | `from` | string | 否 | 请求指定的发送者 actor/bot ID；最终会由服务层校验。 |

请求示例：

```json
{
  "message": "@Reviewer Please check rollback risk",
  "from": "human_123456"
}
```

成功输出：

```json
{
  "delivered": true,
  "session_id": "group_abc:abcdef12",
  "group_id": "group_abc",
  "driver_bot": "bot_driver",
  "delivered_count": 1,
  "failed_count": 0,
  "delivery_results": [
    {
      "bot_uuid": "bot_reviewer",
      "delivery_type": "send",
      "success": true
    }
  ],
  "mentions": [
    "bot_reviewer"
  ]
}
```

常见错误：

- `404`：session 或 group 不存在。
- `409`：session 不是 running。
- `401`：调用方未认证。
- `403`：调用方不是 session participant，或服务层拒绝发送。

#### `GET /sessions/{sid}/messages`

获取 session 历史消息。

鉴权方式：

- 当前默认使用 Public caller，不强制登录。
- 只有传 `view_bot_id` 时才尝试解析 Human / Bot caller；解析不到身份时仍会回落为 Public caller。
- state-machine group 会走 state-machine session history；其他 group 走 group message history。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `sid` | string | 是 | Session ID。 |
| Query | `view_bot_id` | string | 否 | 指定视角 Bot，用于历史消息视角/权限处理。 |
| Query | `limit` | number | 否 | 返回条数。未传时当前实现使用 `u64::MAX`。 |
| Query | `before` | number | 否 | 毫秒时间戳游标，返回该时间之前的消息。 |

成功输出：

- Body：`GroupMessage[]`，不是 wrapper。

```json
[
  {
    "id": "msg_abc",
    "timestamp": 1710000000000,
    "sender": "human_123456",
    "content": "Can we ship this release today?",
    "message_type": "bot",
    "role": "user",
    "run_id": "run_abc",
    "metadata": {
      "source": "session"
    }
  },
  {
    "id": "msg_def",
    "timestamp": 1710000001000,
    "sender": "bot_reviewer",
    "content": "Ship with rollback checklist.",
    "message_type": "bot",
    "bot_name": "Reviewer",
    "role": "assistant"
  }
]
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 消息 ID。 |
| `timestamp` | number | 毫秒时间戳。 |
| `sender` | string | 发送者 actor/bot ID 或 `system`。 |
| `content` | string | 消息内容。 |
| `message_type` | string | `bot`、`system`、`fusion`。 |
| `bot_name` | string | Bot 展示名；为空时字段不出现。 |
| `role` | string | `user`、`tool_result`、`assistant`、`system`。 |
| `run_id` | string | 上游运行 ID；为空时字段不出现。 |
| `historyMeta` | object | OpenClaw 历史元数据；为空时字段不出现。 |
| `metadata` | object | 其他元数据；为空时字段不出现。 |

错误输出：

```json
{
  "error": "invalid_limit",
  "message": "limit must be > 0, got 0"
}
```

### 服务调用

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/services/{group_id}/sessions` | 发起服务调用 | 可暴露 |
| GET | `/services/{group_id}/sessions/{session_id}` | 获取服务 session | 可暴露 |

### 邀请

| Method | Path | 说明 | OpenAPI |
|--------|------|------|---------|
| POST | `/groups/{id}/invite-link` | 创建群组邀请链接 | 可暴露 |
| POST | `/sessions/{sid}/invite-link` | 创建 session 邀请链接 | 可暴露 |
| POST | `/groups/join/{token}` | 通过邀请加入群组 | 可暴露 |
| POST | `/sessions/join/{token}` | 通过邀请加入 session | 可暴露 |

### 服务调用详细接口

本节展开“服务调用”相关接口的鉴权、入参和出参。服务调用是面向外部系统触发 service group 的入口，语义上固定创建或查询 `service_invocation` session；它和普通 `POST /groups/{id}/sessions` 的区别是：这里优先使用服务 API Key 鉴权，并用 `caller_principal` 做服务调用方隔离。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `POST /services/{group_id}/sessions` | `X-BCS-Service-Key` 或 Bot token | 优先使用服务 API Key；未传服务 Key 时 fallback 到 Bot token。服务 Key 可绑定 group，Bot token 会解析为 `bot:<bot_uuid>` caller principal。 |
| `GET /services/{group_id}/sessions/{session_id}` | `X-BCS-Service-Key` 或 Bot token | 除了校验 group 绑定，还要求 session 的 `caller_principal` 与当前调用方一致，否则返回 `403`。 |

服务 Key 规则：

- 请求头：`X-BCS-Service-Key: <raw_key>`。
- 服务端配置里存储的是 `sha256(raw_key)`，不会存储原始 key。
- `caller_principal` 形态为 `svc-key:{sha256前16位}`。
- 如果服务端 API Key registry 为空，当前实现会接受任意非空 `X-BCS-Service-Key` 并按其 sha256 生成 caller principal；这更像本地/过渡兼容行为，生产 OpenAPI 不建议依赖。
- 如果 registry 非空，未命中返回 `401 {"error":"invalid_key"}`。
- 如果 key 配置了 `bound_groups` 且不包含当前 `group_id`，返回 `403 {"error":"key_not_bound_to_group","group_id":"..."}`。

Bot token fallback：

- `X-BCS-Bot-Token: <bot_runtime_token>` 优先。
- 其次使用 `Authorization: Bearer <bot_runtime_token>`。
- 如果缺少服务 Key 和 Bot token，返回 `401 {"error":"missing_bot_identity"}`。
- 如果 Bot token 无效或容器头校验失败，返回 `401 {"error":"invalid_bot_token"}`。

服务调用 Session 输出结构：

`/services/*` 返回的是 service-invocation wire format，字段和普通 Session route 不完全一样：它只返回 `session_id`，不返回 `id`；并且一些 `Option` 字段为空时会显式为 `null`。

```json
{
  "session_id": "group_abc:abcdef12",
  "group_id": "group_abc",
  "session_title": "Release audit",
  "status": "running",
  "session_kind": "service_invocation",
  "activation_count": 1,
  "participants": [
    {
      "bot_uuid": "bot_worker",
      "bot_name": "Worker",
      "role": "worker",
      "actor_kind": "bot",
      "mode": "auto"
    }
  ],
  "input": {
    "task": "audit"
  },
  "output": null,
  "error_message": null,
  "callback_status": null,
  "meta": {
    "request_id": "req_abc"
  },
  "reused": false,
  "created_at": 1710000000000,
  "updated_at": 1710000000000,
  "completed_at": null
}
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string | Service invocation session ID。 |
| `group_id` | string | Service group ID。 |
| `session_title` | string \| null | 会话标题。 |
| `status` | string | `running`、`completed`。 |
| `session_kind` | string | 固定为 `service_invocation`。 |
| `activation_count` | number | 初始为 1；reactivate 后递增。 |
| `participants[]` | object[] | 参与者快照；mode 会被补默认值。 |
| `input` | any JSON \| null | 调用输入。 |
| `output` | any JSON \| null | 完成后的输出。 |
| `error_message` | string \| null | 错误信息。 |
| `callback_status` | string \| null | 回调状态。 |
| `meta` | any JSON \| null | 调用方透传元数据。 |
| `reused` | boolean | 本次是否复用/重新激活已有 session。 |
| `created_at`、`updated_at`、`completed_at` | number \| null | 毫秒时间戳。 |
| `state_machine_run_id` / `state_machine_run` | string / object | state-machine service group 启动运行时会附加。 |

#### `POST /services/{group_id}/sessions`

发起服务调用。目标 group 必须是 service group，即 group 上存在 `service_spec`。

鉴权方式：

- 优先 `X-BCS-Service-Key`。
- 未传服务 Key 时，使用 Bot token fallback。
- 新建 session 时，如果 `service_spec.max_concurrency` 已达到上限，返回 `429`。
- 如果请求体带 `session_id`，route 会先校验该 session 是否属于 path 中的 `group_id`；不属于则返回 `404`。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Service-Key` | string | 是* | 服务 API Key。与 Bot token 二选一，优先级更高。 |
| Header | `X-BCS-Bot-Token` | string | 是* | Bot runtime token。未传服务 Key 时可用。 |
| Header | `Authorization` | string | 是* | `Bearer <bot_runtime_token>`。未传服务 Key 和 `X-BCS-Bot-Token` 时可用。 |
| Path | `group_id` | string | 是 | Service group ID。 |
| Body | `session_id` | string | 否 | 指定已有 service invocation session 时尝试 reactivate。 |
| Body | `caller_id` | string | 否 | 调用方传入的业务 trace/caller id，会落到 session `caller_id`。 |
| Body | `input` | any JSON | 否 | 服务调用输入。 |
| Body | `session_title` | string | 否 | 会话标题。 |
| Body | `meta` | any JSON | 否 | 调用方透传元数据。 |

请求示例：

```json
{
  "caller_id": "trace-1",
  "session_title": "性能审计",
  "input": {
    "task": "audit"
  },
  "meta": {
    "request_id": "req_abc"
  }
}
```

成功输出：

- Status：`202 Accepted`
- Body：服务调用 Session 对象。

```json
{
  "session_id": "group_abc:abcdef12",
  "group_id": "group_abc",
  "session_title": "性能审计",
  "status": "running",
  "session_kind": "service_invocation",
  "activation_count": 1,
  "participants": [],
  "input": {
    "task": "audit"
  },
  "output": null,
  "error_message": null,
  "callback_status": null,
  "meta": {
    "request_id": "req_abc"
  },
  "reused": false,
  "created_at": 1710000000000,
  "updated_at": 1710000000000,
  "completed_at": null
}
```

state-machine service group 成功输出会额外包含：

```json
{
  "state_machine_run_id": "sm_abc",
  "state_machine_run": {
    "run": {
      "run_id": "sm_abc",
      "status": "running"
    },
    "nodes": []
  }
}
```

常见错误：

```json
{
  "error": "max_concurrency_exceeded",
  "max": 10,
  "current_running": 10,
  "retry_after_seconds": 10
}
```

其他错误：

- `401 {"error":"invalid_key"}`
- `401 {"error":"missing_bot_identity","message":"valid bot token is required when X-BCS-Service-Key is absent"}`
- `401 {"error":"invalid_bot_token"}`
- `403 {"error":"key_not_bound_to_group","group_id":"group_abc"}`
- `400 {"error":"invalid_params","message":"group group_abc is not a service group (no service_spec)"}`
- `404 {"error":"not_found","message":"group group_abc not found"}`
- `409 {"error":"..."}`：reactivate 仍在 running 的 session 等冲突场景。

#### `GET /services/{group_id}/sessions/{session_id}`

查询服务调用 session。

鉴权方式：

- 同 `POST /services/{group_id}/sessions`。
- 额外校验：session 必须属于 path 中的 `group_id`。
- 额外校验：session 的 `caller_principal` 必须等于当前服务 Key / Bot token 解析出的 caller principal。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Service-Key` | string | 是* | 服务 API Key。 |
| Header | `X-BCS-Bot-Token` | string | 是* | Bot runtime token fallback。 |
| Header | `Authorization` | string | 是* | `Bearer <bot_runtime_token>` fallback。 |
| Path | `group_id` | string | 是 | Service group ID。 |
| Path | `session_id` | string | 是 | Service invocation session ID。 |

成功输出：

- Status：`200 OK`
- Body：服务调用 Session 对象；查询场景 `reused` 固定为 `false`。

隔离错误：

```json
{
  "error": "forbidden",
  "message": "caller_principal mismatch"
}
```

未找到输出：

```json
{
  "error": "session not found"
}
```

### 邀请详细接口

本节展开“邀请”相关接口的鉴权、入参和出参。邀请分两类：创建 invite link 和通过 invite link 加入。创建 link 需要有 group 管理身份；join 需要人类登录态，且当前仅允许 Human actor 通过邀请加入。

通用鉴权约定：

| 接口 | 当前鉴权方式 | 说明 |
|------|--------------|------|
| `POST /groups/{id}/invite-link` | Bot token 或人类登录态 | 创建者必须是 group driver、originator，或拥有 driver/originator Bot 的人类。DM group 不支持邀请链接，非 active group 不支持 group invite。 |
| `POST /sessions/{sid}/invite-link` | Bot token 或人类登录态 | 先通过 session 找到 parent group，再套用 group invite 的创建权限。DM group 不支持邀请链接。 |
| `POST /groups/join/{token}` | 人类登录态 | 必须能解析出 `staff_no`；join 后会确保 `human_{staff_no}` actor 存在，并加入 group。 |
| `POST /sessions/join/{token}` | 人类登录态 | 必须能解析出 `staff_no`；join 后会确保 `human_{staff_no}` actor 存在，并加入 session。 |

Bot token / 人类登录态：

- 创建邀请时，Bot token 通过 `X-BCS-Bot-Token` 或 `Authorization: Bearer <bot_runtime_token>` 解析。
- 如果没有 Bot token，会尝试解析人类登录态。
- Join 邀请时不接受 Bot 作为加入者，必须是人类登录态。

Invite token：

- 创建接口返回 `invite_token` 和 `join_url`。
- token payload 包含版本 `v`、目标 ID `id`、过期秒级时间戳 `exp`，并使用 HMAC-SHA256 签名后做 URL-safe base64。
- `ttl_seconds` 未传时使用服务端 `invite_default_ttl_seconds`，当前默认值为 `86400` 秒。
- 过期 token 通常返回 `410 gone`；如果当前人已经是目标成员，当前实现会先返回 `already_member=true`，不会因为 token 过期而失败。

创建成功输出：

```json
{
  "invite_token": "eyJ2IjoxLCJpZCI6Imdyb3VwX2FiYyIsImV4cCI6MTcxMDA4NjQwMH0...",
  "expires_at": 1710086400,
  "join_url": "https://bcn.alipay.com/groups/join/eyJ2Ijox..."
}
```

加入成功输出：

```json
{
  "joined": true,
  "already_member": false,
  "target_type": "group",
  "target_id": "group_abc",
  "actor_id": "human_123456"
}
```

错误输出格式：

Invite route 使用 `HttpAdapterError` 统一错误格式：

```json
{
  "status": 410,
  "code": "gone",
  "params": {
    "reason": "invite link has expired"
  },
  "message": "invite link has expired",
  "error": "invite link has expired"
}
```

#### `POST /groups/{id}/invite-link`

为 group 创建邀请链接。

鉴权方式：

- 创建者必须是 group driver、originator，或拥有 driver/originator Bot 的人类。
- DM group 不支持邀请链接。
- group 必须是 active。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 是* | Bot runtime token。与人类登录态二选一。 |
| Header | `Authorization` | string | 是* | `Bearer <bot_runtime_token>`。 |
| Path | `id` | string | 是 | Group ID。 |
| Body | `ttl_seconds` | number | 否 | 邀请有效期秒数；不传使用服务端默认 TTL。Body 可省略。 |

请求示例：

```json
{
  "ttl_seconds": 86400
}
```

成功输出：

```json
{
  "invite_token": "invite-token",
  "expires_at": 1710086400,
  "join_url": "https://bcn.alipay.com/groups/join/invite-token"
}
```

常见错误：

- `403 forbidden`：调用方不是 driver/originator 或其 owner；DM group 不支持 invite。
- `404 not_found`：group 不存在。
- `409 conflict`：group 非 active。

#### `POST /sessions/{sid}/invite-link`

为 session 创建邀请链接。

鉴权方式：

- 先查 session，再查 parent group。
- 创建权限与 group invite 相同：group driver、originator，或拥有 driver/originator Bot 的人类。
- parent group 是 DM group 时不支持邀请链接。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Header | `X-BCS-Bot-Token` | string | 是* | Bot runtime token。与人类登录态二选一。 |
| Header | `Authorization` | string | 是* | `Bearer <bot_runtime_token>`。 |
| Path | `sid` | string | 是 | Session ID。 |
| Body | `ttl_seconds` | number | 否 | 邀请有效期秒数；Body 可省略。 |

成功输出：

```json
{
  "invite_token": "invite-token",
  "expires_at": 1710086400,
  "join_url": "https://bcn.alipay.com/sessions/join/invite-token"
}
```

常见错误：

- `403 forbidden`：调用方无创建权限；DM group 不支持 invite。
- `404 not_found`：session 或 parent group 不存在。

#### `POST /groups/join/{token}`

通过邀请链接加入 group。

鉴权方式：

- 必须有人类登录态，并能解析出 `staff_no`。
- 不支持 Bot 直接通过 invite link 加入。
- 当前实现会确保 `human_{staff_no}` actor 存在。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `token` | string | 是 | `POST /groups/{id}/invite-link` 返回的 invite token。 |
| Body | - | - | 否 | 当前不读取请求体。 |

成功输出：首次加入

```json
{
  "joined": true,
  "already_member": false,
  "target_type": "group",
  "target_id": "group_abc",
  "actor_id": "human_123456"
}
```

成功输出：已经是成员

```json
{
  "joined": false,
  "already_member": true,
  "target_type": "group",
  "target_id": "group_abc",
  "actor_id": "human_123456"
}
```

加入后的参与者属性：

- `role`: `consultant`
- `actor_kind`: `human`
- `mode`: `present`
- `bot_name`: 当前登录态 `nick_name`，为空时使用 staff_no。

#### `POST /sessions/join/{token}`

通过邀请链接加入 session。

鉴权方式：

- 必须有人类登录态，并能解析出 `staff_no`。
- 不支持 Bot 直接通过 invite link 加入。
- 当前实现会确保 `human_{staff_no}` actor 存在。

接受参数：

| 位置 | 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|------|
| Path | `token` | string | 是 | `POST /sessions/{sid}/invite-link` 返回的 invite token。 |
| Body | - | - | 否 | 当前不读取请求体。 |

成功输出：首次加入

```json
{
  "joined": true,
  "already_member": false,
  "target_type": "session",
  "target_id": "group_abc:abcdef12",
  "actor_id": "human_123456"
}
```

成功输出：已经是成员

```json
{
  "joined": false,
  "already_member": true,
  "target_type": "session",
  "target_id": "group_abc:abcdef12",
  "actor_id": "human_123456"
}
```

加入后的参与者属性：

- `role`: `consultant`
- `actor_kind`: `human`
- `mode`: `present`
- 成功加入 session 后会发送 `HumanJoined` 系统消息。

## OpenAPI 建议暴露范围

建议 OpenAPI 优先覆盖以下稳定业务能力：

- 前端 Workbench WebSocket：`GET /ws` 及 `connect`、`chat.send`、`chat.abort` 帧协议
- Bot 目录查询：`/bots`、`/bots/my`、`/bots/query`、`/bots/{id}`、好友/群组/可见性查询与设置；`/bots/paged` 并入 `/bots` 后不进入 OpenAPI
- Provider 管理：`/providers/{provider_id}`、`/providers/{provider_id}/bots`
- Actor 目录：`/actors/list`、`/actors/search`、`/actors/{aid}/status`
- 好友关系：`/friends/*`
- 群组管理：`/groups`、`/groups/{id}`、members、visibility、settings、participant mode
- 协作模板：`/collaboration/templates`
- Session：`/groups/{id}/sessions`、`/sessions/*`
- 服务调用：`/services/{group_id}/sessions`
- 邀请：`/groups/{id}/invite-link`、`/sessions/{sid}/invite-link`、join 接口

以下接口不建议放入 OpenAPI：

- 系统运维：health、metrics、manifest、admin secret
- Bot runtime 通信：`/ws/bot` 及 Bot WS 帧协议
- Bot 内部生命周期/消息：connect、discover、onboard、status、legacy blocking chat。其中 `/bots/connect` 建议待删除，`/bots/status` 建议待废弃，`/bots/{id}/chat` 建议并入 async 后废弃
- 单 Bot 异步调用：`POST /bots/{id}/chat-async`、`GET /chat/runs/{run_id}`、`POST /chat/runs/{run_id}/cancel` 作为单独一组；当前不放入主 OpenAPI，若未来对外暴露单 Bot invocation，应三件套一起暴露
- Provider 内部/灰度/回调：注册、agentpass resolve、stream gray、enable/disable、delivery switch、bot events
- 身份/OAuth/Register 内部流程
- 群组内部控制：collaboration definition、routing policy、status、terminate、label、workspace
- 群消息/回调/fuse 整类废弃，待删除
- 状态机运行接口
