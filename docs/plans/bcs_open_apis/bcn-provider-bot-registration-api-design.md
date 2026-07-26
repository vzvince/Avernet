# BCN Provider 与 Bot 注册 API 设计（讨论稿）

本文记录 Provider 管理和 Bot 注册 API 当前已经确认的设计结论。现阶段仅作为设计输入，不同步到正式 OpenAPI YAML 和接口目录。

## 1. 设计边界

BCN 只保留两种 Bot 注册方式：

1. Provider 使用自己的管理凭证注册 Bot；
2. 个人先获取注册令牌，再使用该令牌注册 Bot。

两种方式最终都创建相同的领域对象：

```text
BotActor
+ BotDescriptor
+ ProviderBotBinding
+ creates 关系
+ 可选的 BotRuntimeCredential
```

个人注册的 Bot 自动关联 BCN 默认 Provider，连接方式固定为 `plugin`。Provider 注册时显式指定 Bot 使用的 `conn_type`。

本轮不提供：

- 通用 `POST /actors` 或 `POST /bots`；
- Bot 删除、注销或 ProviderBinding 删除接口；
- ProviderBinding 连接方式切换接口；
- AgentCard 投影接口。

## 2. 接口总览

| 接口 | 类型 | 作用 | 鉴权 |
|---|---|---|---|
| `POST /api/bcn/v1/providers` | Internal API | 创建 Provider 并签发 Provider 凭证 | BCN 管理权限 + HumanCookie |
| `GET /openapi/bcn/v1/providers/{provider_id}` | OpenAPI | 获取 Provider 信息 | Provider Admin Token |
| `PATCH /openapi/bcn/v1/providers/{provider_id}` | OpenAPI | 更新 Provider 可变配置 | Provider Admin Token；敏感修改叠加 HumanCookie |
| `GET /openapi/bcn/v1/providers/{provider_id}/bots` | OpenAPI | 查询 Provider 注册的 Bot | Provider Admin Token |
| `POST /openapi/bcn/v1/providers/{provider_id}/bots` | OpenAPI | 通过 Provider 注册 Bot | Provider Admin Token |
| `POST /openapi/bcn/v1/bot-registration-tokens` | OpenAPI | 为当前 Human 签发个人 Bot 注册令牌 | HumanCookie |
| `POST /openapi/bcn/v1/bot-registrations` | OpenAPI | 使用注册令牌完成个人 Bot 注册 | Registration Token |

所有 REST API 使用统一 response envelope 和 snake_case JSON：

```json
{
  "code": 200000,
  "message": "OK",
  "data": {},
  "request_id": "req_registration_001"
}
```

创建资源成功使用 HTTP `201` 和业务码 `201000`。Token、Cookie 和密钥不得出现在 URL query、日志或错误消息中。

## 3. Provider 管理

### 3.1 创建 Provider

```http
POST /api/bcn/v1/providers
```

该接口负责 Provider 入网和初始凭证签发，属于平台管理能力，不作为普通 OpenAPI 暴露。BCN 默认 Provider 由系统初始化，不通过该接口创建。

输入：

```json
{
  "slug": "example-provider",
  "name": "Example Provider",
  "supported_conn_types": ["plugin", "gateway"],
  "gateway_config": {
    "webhook_url": "https://provider.example.com/bcn",
    "protocol_version": "v1"
  }
}
```

| 字段 | 含义 |
|---|---|
| `slug` | Provider 的稳定可读标识，创建后不可修改 |
| `name` | Provider 展示名称 |
| `supported_conn_types` | 支持的连接方式，可包含 `plugin`、`gateway` |
| `gateway_config` | Gateway 下行配置；仅支持 `gateway` 时需要 |

输出包括创建后的 Provider；Provider Admin Token 仅签发时返回。只有支持 `gateway` 的 Provider 才需要 BCN 到 Provider 的下行凭证。

### 3.2 获取 Provider

```http
GET /openapi/bcn/v1/providers/{provider_id}
```

输出 Provider 的基本信息、支持的连接方式和非敏感连接配置。响应不得返回 Provider Admin Token、下行密钥等凭证。

### 3.3 更新 Provider

```http
PATCH /openapi/bcn/v1/providers/{provider_id}
```

用于更新名称和连接配置。`provider_id`、`slug` 不可修改。移除某种 `supported_conn_types` 前，不得存在仍使用该连接方式的有效 BotBinding。

## 4. Provider Bot

### 4.1 查询 Provider Bot

```http
GET /openapi/bcn/v1/providers/{provider_id}/bots
```

查询该 Provider 注册的 BotActor 和 ProviderBotBinding，用于 Provider 控制台、集成校验和运维排查。

输入：

| 参数 | 含义 |
|---|---|
| `q` | 按 Bot 名称或 `provider_bot_ref` 过滤 |
| `conn_type` | 按 `plugin/gateway` 过滤 |
| `offset/limit` | 分页参数 |

输出为统一分页结构，不返回任何 Bot Runtime Token。

### 4.2 Provider 注册 Bot

```http
POST /openapi/bcn/v1/providers/{provider_id}/bots
Authorization: Bearer <provider_admin_token>
```

输入：

```json
{
  "provider_bot_ref": "reviewer-001",
  "conn_type": "gateway",
  "agent_code": "reviewer-001",
  "created_by": "123456",
  "name": "Code Reviewer",
  "descriptor": {
    "summary": "代码审查 Bot",
    "domains": ["software-engineering"],
    "skills": [
      {
        "name": "code-review",
        "description": "审查代码变更"
      }
    ],
    "scopes": ["group", "session"]
  }
}
```

| 字段 | 含义 |
|---|---|
| `provider_bot_ref` | Bot 在该 Provider 内的唯一标识 |
| `conn_type` | 该 Bot 实际使用的连接方式，只能为 `plugin` 或 `gateway` |
| `agent_code` | Bot 的外部身份标识，可选；存在时在环境内唯一 |
| `created_by` | 创建人的工号，用于建立第一版 `creates` 关系 |
| `name` | BotActor 展示名称 |
| `descriptor` | Bot 的协作描述、领域、技能和 scope |

`conn_type` 必须包含在 Provider 的 `supported_conn_types` 中。同一个 Provider 可以注册不同连接方式的 Bot，但单个 ProviderBotBinding 只有一个当前连接方式。

输出：

```json
{
  "code": 201000,
  "message": "OK",
  "data": {
    "actor": {
      "actor_id": "bot_reviewer",
      "kind": "bot",
      "name": "Code Reviewer"
    },
    "binding": {
      "provider_id": "prv_example",
      "provider_bot_ref": "reviewer-001",
      "conn_type": "gateway"
    }
  },
  "request_id": "req_provider_bot_001"
}
```

凭证规则：

- `plugin` Bot 需要 Bot Runtime Token；该凭证只在首次成功注册时返回；
- `gateway` Bot 由 BCN 通过 Provider 下行，不返回 Bot Runtime Token；
- 幂等重试可以返回已有 Actor 和 Binding，但不得重放历史明文凭证。

BotDescriptor 后续统一通过 `PATCH /openapi/bcn/v1/bots/{actor_id}/descriptor` 修改，Provider Admin Token 只能修改绑定到本 Provider 的 Bot。

## 5. 个人注册 Bot

个人注册属于正式 OpenAPI。它不是第三种 Bot 模型，而是使用 BCN 默认 Provider 的便捷注册流程：

```text
HumanCookie
  -> Registration Token
  -> BotActor
  -> BCN Default ProviderBinding(conn_type=plugin)
  -> Bot Runtime Token
```

### 5.1 签发注册令牌

```http
POST /openapi/bcn/v1/bot-registration-tokens
Cookie: HumanCookie
```

请求体为空对象：

```json
{}
```

输出：

```json
{
  "code": 201000,
  "message": "OK",
  "data": {
    "registration_token": "<registration_token>",
    "expires_at": 1784520000000
  },
  "request_id": "req_registration_token_001"
}
```

注册令牌绑定当前 Human、BCN 默认 Provider、`plugin` 连接方式、注册用途和过期时间，不得作为普通 Bot Runtime Token 使用。

### 5.2 完成个人注册

```http
POST /openapi/bcn/v1/bot-registrations
Authorization: Bearer <registration_token>
Content-Type: application/json
```

输入：

```json
{
  "name": "My Assistant",
  "agent_code": "my-assistant",
  "descriptor": {
    "summary": "个人助理",
    "domains": ["productivity"],
    "skills": [
      {
        "name": "assistant",
        "description": "处理个人协作任务"
      }
    ],
    "scopes": ["group", "session"]
  }
}
```

BCN 根据注册令牌自动确定：

```json
{
  "provider_id": "bcn-default",
  "provider_bot_ref": "<generated_actor_id>",
  "conn_type": "plugin",
  "created_by": "<staff_no_from_registration_token>"
}
```

输出：

```json
{
  "code": 201000,
  "message": "OK",
  "data": {
    "actor": {
      "actor_id": "bot_assistant",
      "kind": "bot",
      "name": "My Assistant"
    },
    "binding": {
      "provider_id": "bcn-default",
      "provider_bot_ref": "bot_assistant",
      "conn_type": "plugin"
    },
    "credentials": {
      "bot_runtime_token": "<only_returned_once>"
    }
  },
  "request_id": "req_registration_001"
}
```

Registration Token 必须通过 Bearer Header 传递，不放入 query 参数。

## 6. 与当前接口的关系

| 当前接口或行为 | 目标设计 |
|---|---|
| `GET /register/token`、`GET /api/bcn/v1/register/token` | 迁移到 `POST /openapi/bcn/v1/bot-registration-tokens`，旧接口标记 deprecated |
| `POST /register`、`POST /api/bcn/v1/register` | 迁移到 `POST /openapi/bcn/v1/bot-registrations`，旧接口标记 deprecated |
| 注册令牌通过 query 传递 | 改为 `Authorization: Bearer` |
| `/bots/onboard`、`/admin/bots/onboard` | 不再作为正式注册方式；仅在迁移期作为兼容或内部实现入口 |
| 个人注册不创建 ProviderBinding | 自动关联 BCN 默认 Provider，`conn_type=plugin` |
| Provider 创建必须配置 webhook | 按 `supported_conn_types` 决定是否需要 Gateway 配置 |
| Provider Bot 注册总是返回 Runtime Token | 仅 `plugin` Bot 首次注册时返回 |
| ProviderBotBinding 没有 `conn_type` | 增加 `plugin/gateway` 连接方式 |

## 7. 待确认项

- Registration Token 是一次只能注册一个 Bot，还是在有效期内允许同一 Human 注册多个 Bot；当前实现为有效期内可重复使用。
- Registration Token 的最终有效期和失败重试幂等规则。
- Provider `supported_conn_types` 和 ProviderBotBinding `conn_type` 的数据库迁移方案。

