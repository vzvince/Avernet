# BCN Session API 设计（讨论稿）

本文记录 BCN Session API 当前已经讨论的设计结论。现阶段仅作为设计输入，不同步到正式 OpenAPI YAML 和接口目录；各接口的精细鉴权规则仍需逐项确认。

## 1. 接口边界

Session 是 Group 中一次相互隔离的协作上下文，分为：

- `chat`：承载持续会话、参与者和消息；
- `service_invocation`：承载一次独立服务调用及其输入、结果和错误。

第一版不单独引入 Task。一次服务调用创建一个新的 `service_invocation` Session。

当前讨论的目标 OpenAPI 如下：

| 接口 | 作用 |
|---|---|
| `POST /openapi/bcn/v1/groups/{group_id}/sessions` | 在 Group 中创建 Session |
| `GET /openapi/bcn/v1/groups/{group_id}/sessions` | 查询调用方可见的 Group Session |
| `GET /openapi/bcn/v1/sessions/{session_id}` | 获取 Session 详情 |
| `PATCH /openapi/bcn/v1/sessions/{session_id}` | 修改 Session 标题等可变属性 |
| `DELETE /openapi/bcn/v1/sessions/{session_id}` | 删除 Session |
| `POST /openapi/bcn/v1/sessions/{session_id}/participants` | 添加 SessionParticipant |
| `PATCH /openapi/bcn/v1/sessions/{session_id}/participants/{actor_id}` | 修改参与者在当前 Session 中的 mode |
| `DELETE /openapi/bcn/v1/sessions/{session_id}/participants/{actor_id}` | 移除参与者或退出 Session |
| `POST /openapi/bcn/v1/sessions/{session_id}/messages` | 向 Chat Session 发送消息 |
| `GET /openapi/bcn/v1/sessions/{session_id}/messages` | 查询 Session 消息历史 |
| `POST /openapi/bcn/v1/sessions/{session_id}/invite-link` | 创建 Session 邀请链接 |
| `POST /openapi/bcn/v1/sessions/join/{token}` | Human 使用邀请令牌加入 Session |
| `POST /openapi/bcn/v1/services/{group_id}/sessions` | 发起服务调用并创建 ServiceInvocation Session |
| `GET /openapi/bcn/v1/services/{group_id}/sessions/{session_id}` | 查询服务调用状态和结果 |

`POST /sessions/{session_id}/complete` 是现有兼容接口，已标记为 deprecated，不进入目标 OpenAPI。

## 2. Session 基础接口

### 2.1 创建和查询 Group Session

```http
POST /openapi/bcn/v1/groups/{group_id}/sessions
GET  /openapi/bcn/v1/groups/{group_id}/sessions
```

创建接口在指定 Group 中建立一个隔离的 Session；列表接口只返回当前调用方可见的 Session。两者路径归属 Group，但创建和返回的核心资源都是 Session。

与当前实现的映射：

| 目标接口 | 当前接口 |
|---|---|
| `POST /openapi/bcn/v1/groups/{group_id}/sessions` | `POST /groups/{id}/sessions` |
| `GET /openapi/bcn/v1/groups/{group_id}/sessions` | `GET /groups/{id}/sessions` |

### 2.2 Session 详情、更新和删除

```http
GET    /openapi/bcn/v1/sessions/{session_id}
PATCH  /openapi/bcn/v1/sessions/{session_id}
DELETE /openapi/bcn/v1/sessions/{session_id}
```

- `GET` 返回 Session 元数据和 Participant，但不内嵌完整消息历史；
- `PATCH` 只修改标题等允许变化的属性，不直接修改状态、类型和 Invocation 结果；
- `DELETE` 删除 Session 及其从属资源，具体授权范围仍需继续确认。

当前分别对应 `GET/PATCH/DELETE /sessions/{sid}`。

## 3. SessionParticipant 接口

```http
POST   /openapi/bcn/v1/sessions/{session_id}/participants
PATCH  /openapi/bcn/v1/sessions/{session_id}/participants/{actor_id}
DELETE /openapi/bcn/v1/sessions/{session_id}/participants/{actor_id}
```

目标接口统一使用：

- `participants`，替代当前路径中的 `members`；
- `actor_id`，替代当前路径中的 `bot_uuid`；
- `mode` 只属于 SessionParticipant，不属于 GroupParticipant。

### 3.1 ParticipantMode

当前代码实际支持四种 mode：

| ActorKind | mode | 含义 | 默认值 |
|---|---|---|---|
| Bot | `auto` | 正常参与路由，可以被触发响应 | 是 |
| Bot | `muted` | 静默参与，不被触发响应 | 否 |
| Human | `present` | 当前参与 Session，可以发言 | 否 |
| Human | `absent` | 暂离当前 Session | 是 |

合法组合必须限制为：

```text
Bot   -> auto | muted
Human -> present | absent
```

`active` 不是 ParticipantMode。当前 Session 更新接口可以解析上述四个值，但没有严格校验 ActorKind；目标接口需要拒绝 `Bot + present`、`Human + muted` 等非法组合。

## 4. Message 接口

```http
POST /openapi/bcn/v1/sessions/{session_id}/messages
GET  /openapi/bcn/v1/sessions/{session_id}/messages
```

- `POST` 在 Chat Session 中创建消息；
- `GET` 查询 Session 的消息历史，并按 Session 内顺序分页返回；
- 两个接口只适用于 `kind=chat` 的 Session。

发送接口使用 `/messages` 替代当前的 `/sessions/{sid}/chat`，以表达 Message 资源的创建；查询接口对应当前的 `/sessions/{sid}/messages`。

## 5. Session 邀请接口

```http
POST /openapi/bcn/v1/sessions/{session_id}/invite-link
POST /openapi/bcn/v1/sessions/join/{token}
```

- `invite-link` 创建具有有效期和使用约束的 Session 邀请链接；
- `join/{token}` 允许 Human 通过邀请令牌成为 SessionParticipant；
- 加入 Session 不会自动将 Actor 提升为 GroupParticipant。

## 6. Service Invocation 接口

```http
POST /openapi/bcn/v1/services/{group_id}/sessions
GET  /openapi/bcn/v1/services/{group_id}/sessions/{session_id}
```

Service 接口与普通 Chat Session 接口分开，保留服务密钥、调用方隔离和 Invocation 结果查询语义。

### 6.1 发起调用

`POST` 为一次服务调用创建新的 `service_invocation` Session。调用输入、业务调用方标识、回调配置和扩展元数据属于 Session 的 Invocation 信息。

### 6.2 查询调用结果

`GET` 查询调用状态、输出、错误和回调状态。当前主要返回：

```json
{
  "session_id": "session_001",
  "group_id": "group_001",
  "session_title": null,
  "status": "running",
  "session_kind": "service_invocation",
  "participants": [],
  "input": {},
  "output": null,
  "error_message": null,
  "callback_status": null,
  "meta": {},
  "created_at": 1784512800000,
  "updated_at": 1784512800000,
  "completed_at": null
}
```

当前底层已经实现以下裸路径，BCS CLI 的 `service invoke/status/wait` 正在使用：

```http
POST /services/{group_id}/sessions
GET  /services/{group_id}/sessions/{session_id}
```

BCS Router 尚未直接注册 `/openapi/bcn/v1` 前缀；目标路径需要通过 OpenAPI 路由映射或网关转发暴露。

Service 接口支持 `X-BCS-Service-Key` 或 Bot token。查询时还必须满足：

- Session 属于路径中的 Group；
- Session 的内部 `callerPrincipal` 与当前调用方一致。

`callerPrincipal` 是服务端生成的安全隔离字段，不进入公开 Invocation 领域模型。

## 7. 初步鉴权原则

普通 Session 接口建议基于协作关系授权：

- GroupManager 可以访问 Group 下全部 Session；
- GroupParticipant 可以查看 Group 下全部 Session；
- 仅作为 SessionParticipant 加入的 Actor，只能访问自己参与的 Session；
- HumanCookie 可以代表 Human 自己，或者自己创建的 Bot；
- Bot token 解析出的 Bot 身份必须与请求 Actor 匹配；
- 不允许匿名访问。

以上是 Session 可见性的初步原则。创建、修改、删除、Participant 管理、消息发送和邀请链接管理的写权限仍需逐接口确认。

## 8. 当前实现与目标设计的主要差异

| 项目 | 当前实现 | 目标设计 |
|---|---|---|
| OpenAPI 路径 | BCS 裸路径 | 统一使用 `/openapi/bcn/v1` |
| Participant 路径 | `members/{bot_uuid}` | `participants/{actor_id}` |
| 发送消息 | `POST /sessions/{sid}/chat` | `POST /sessions/{session_id}/messages` |
| ParticipantMode 校验 | 接受四个字符串，Session 路径未严格校验 ActorKind | 严格校验 Bot/Human 合法组合 |
| Chat 完成接口 | 存在 `/complete` 兼容接口 | deprecated，不进入目标 OpenAPI |
| Service Invocation | 裸路径和业务能力已实现 | 增加目标 OpenAPI 路径映射 |
| 普通 Session 鉴权 | 当前部分接口仍较宽松 | 统一基于 Group/Session 协作关系授权 |
