# BCN Group API 设计（讨论稿）

本文记录 BCN Group API 当前已经确认的设计结论。现阶段仅作为设计输入，不同步到正式 OpenAPI YAML 和接口目录。

## 1. 接口边界

Group 列表从通用查询收敛为 Actor 维度查询，暂不提供通用 `GET /groups`。

当前确认的接口如下。除状态机运行接口外，其余接口均为 OpenAPI：

| 类型 | 接口 | 作用 |
|---|---|---|
| OpenAPI | `GET /openapi/bcn/v1/actors/{actor_id}/groups` | 查询 Actor 作为 GroupParticipant 直接参与的群 |
| OpenAPI | `GET /openapi/bcn/v1/actors/{actor_id}/groups/session-only` | 查询 Actor 不在群中，但作为 SessionParticipant 参与了群内 Session 的群 |
| OpenAPI | `POST /openapi/bcn/v1/groups` | 创建 Group |
| OpenAPI | `GET /openapi/bcn/v1/groups/{group_id}` | 获取 Group 详情 |
| OpenAPI | `PATCH /openapi/bcn/v1/groups/{group_id}` | 更新 Group 的可变属性 |
| OpenAPI | `DELETE /openapi/bcn/v1/groups/{group_id}` | 删除整个 Group |
| OpenAPI | `POST /openapi/bcn/v1/groups/{group_id}/participants` | 添加 GroupParticipant |
| OpenAPI | `PATCH /openapi/bcn/v1/groups/{group_id}/participants/{actor_id}` | 修改 GroupParticipant 的群级角色 |
| OpenAPI | `DELETE /openapi/bcn/v1/groups/{group_id}/participants/{actor_id}` | 移除 GroupParticipant 或退出 Group |
| OpenAPI | `POST /openapi/bcn/v1/groups/{group_id}/sessions` | 在 Group 中创建 Session |
| OpenAPI | `GET /openapi/bcn/v1/groups/{group_id}/sessions` | 查询调用方可见的 Group Session |
| OpenAPI | `POST /openapi/bcn/v1/groups/{group_id}/invite-link` | 创建 Group 邀请链接 |
| OpenAPI | `POST /openapi/bcn/v1/groups/join/{token}` | Human 通过邀请链接加入 Group |
| OpenAPI | `GET /openapi/bcn/v1/collaboration/templates` | 查询可用于建群和协作编排的模板 |
| OpenAPI | `GET /openapi/bcn/v1/collaboration/templates/{template_id}` | 获取单个协作模板详情 |
| Internal API | `POST /api/bcn/v1/groups/{group_id}/state-machine-runs` | 基于 Group 协作定义启动状态机运行 |
| Internal API | `GET /api/bcn/v1/state-machine-runs/{run_id}` | 查询状态机运行状态和结果 |
| Internal API | `GET /api/bcn/v1/state-machine-runs/{run_id}/graph` | 查询运行图结构和节点进度 |
| Internal API | `GET /api/bcn/v1/state-machine-runs/{run_id}/nodes/{node_id}` | 查询节点运行详情 |
| Internal API | `POST /api/bcn/v1/state-machine-runs/{run_id}/cancel` | 取消状态机运行 |

两个 Actor 查询接口都返回 Group 列表，但表达不同的领域关系，不合并为一个接口。GroupParticipant 不包含 `mode`；`auto/muted/present/absent` 属于 SessionParticipant。

### 1.1 暂不提供的 Group API

- 不提供独立的 Group 生命周期接口。Group 的关闭、终止和任意状态修改暂不进入 OpenAPI。
- 不提供 Group 级 CollaborationDefinition 获取、绑定或升级接口。当前前端没有调用这些接口；自定义协作群通过 Collaboration 模板构造配置，并在创建 Group 时一次性提交可选的 `collaboration`。
- Collaboration 模板查询属于 Collaboration 资源，不属于 Group 聚合，但作为建群依赖的 OpenAPI 在本文一并记录。

## 2. 直接参与的群

```http
GET /openapi/bcn/v1/actors/{actor_id}/groups
```

仅返回 Actor 作为 `GroupParticipant` 直接参与的群，不包含仅由 Session 参与关系产生的群。

逻辑条件：

```text
存在 GroupParticipant(actor_id)
```

该接口用于产品中的“我的群组”等长期协作入口。

## 3. 仅通过 Session 参与的群

```http
GET /openapi/bcn/v1/actors/{actor_id}/groups/session-only
```

返回 Actor 不是 `GroupParticipant`，但至少作为一个 `SessionParticipant` 参与了该群某个 Session 的群。

逻辑条件：

```text
不存在 GroupParticipant(actor_id)
并且
至少存在一个属于该群的 SessionParticipant(actor_id)
```

补充约束：

- 结果按 `group_id` 去重，一个群只返回一次；
- Actor 已是 GroupParticipant 时，该群只由直接群接口返回；
- `session-only` 表达会话级参与关系，不限定必须通过邀请链接加入；
- 返回该群不代表 Actor 成为群成员，也不授予群级管理权限；
- Actor 进入群后，只能访问自己实际参与的 Session。

该接口用于产品中的“临时参与的群聊”等入口。

## 4. 鉴权

两个接口使用相同的鉴权规则，不允许匿名访问，也不提供任意 `participant_id` 查询。

| 调用身份 | 目标 Actor | 授权条件 |
|---|---|---|
| HumanCookie | HumanActor | Cookie 中的 `staff_no` 必须对应 Actor 本人 |
| HumanCookie | BotActor | BotActor 的 `created_by` 必须等于当前 `staff_no` |
| Bot token | BotActor | Token 解析出的 Bot 身份必须与 `{actor_id}` 完全一致 |
| Bot token | HumanActor | 不允许 |

Bot token 包括普通 Bot runtime token，以及最终能够解析为 BCN Bot 身份的 AgentPass token。

建议的错误语义：

- `401`：缺少身份或 Cookie/token 无效；
- `404`：Actor 不存在，或者调用方无权查询该 Actor，避免通过接口枚举 Actor。

Group 管理接口使用 `GroupManager` 权限。调用方满足以下任一条件即可视为 GroupManager：

- HumanCookie 的 `staff_no` 等于 Group 的 `created_by`；
- 调用方 Actor 是 Group 的 `originator` 或 `driver`；
- HumanCookie 对应的人是 originator/driver Bot 的创建者。

GroupParticipant 自身或其 Human creator 可以代表该 Participant 退出 Group，但不能修改 Group 配置或管理其他 Participant。

## 5. 查询与返回

两个接口使用相同的查询能力：

| 参数 | 含义 |
|---|---|
| `kind` | 按 `normal`、`dm` 或 `all` 过滤 |
| `status` | 按 Group 状态过滤 |
| `visibility` | 按 Group 可见性过滤 |
| `strategy` | 按 `chat`、`manager_worker` 或 `state_machine` 过滤 |
| `q` | 按群名称模糊查询 |
| `offset` / `limit` | 分页参数 |

两个接口返回相同的分页 `GroupSummary`。列表不返回完整 participants，详细成员关系由 Group 详情接口承载。

```json
{
  "code": 200000,
  "message": "OK",
  "data": {
    "actor_id": "bot_reviewer",
    "items": [
      {
        "group_id": "group_001",
        "version": 1,
        "label": "代码评审",
        "context": "完成版本发布前的代码评审",
        "kind": "normal",
        "status": "active",
        "visibility": "private",
        "created_by": "123456",
        "originator": "human_123456",
        "driver": "bot_reviewer",
        "participant_count": 3,
        "message_count": 24,
        "strategy": "chat",
        "gmt_create": 1784512800000,
        "gmt_modified": 1784516400000
      }
    ],
    "total": 1,
    "offset": 0,
    "limit": 20
  },
  "request_id": "req-group-001"
}
```

## 6. Group 输出模型

Group 详情和创建、更新接口统一返回完整 Group。GroupParticipant 只表达群级成员关系和角色，不包含 `mode`。

```json
{
  "group_id": "group_001",
  "version": 1,
  "label": "代码评审",
  "context": "完成版本发布前的代码评审",
  "kind": "normal",
  "status": "active",
  "visibility": "private",
  "created_by": "123456",
  "originator": "human_123456",
  "driver": "bot_reviewer",
  "participants": [
    {
      "actor_id": "bot_reviewer",
      "actor_kind": "bot",
      "role": "driver"
    },
    {
      "actor_id": "human_123456",
      "actor_kind": "human",
      "role": "observer"
    }
  ],
  "strategy": "chat",
  "routing_policy": null,
  "collaboration": null,
  "service_spec": null,
  "gmt_create": 1784512800000,
  "gmt_modified": 1784516400000
}
```

`routing_policy`、`collaboration` 和 `service_spec` 是可选配置；查询时只能返回安全视图，不原样返回凭据。`status/version/created_by/gmt_create/gmt_modified` 由服务端维护，不能由普通更新接口直接修改。

## 7. Group 管理接口

### 7.1 创建 Group

```http
POST /openapi/bcn/v1/groups
```

输入：

| 字段 | 必填 | 含义 |
|---|---|---|
| `label` | 是 | 群名称 |
| `context` | 否 | 协作目标、背景和上下文 |
| `kind` | 否 | `normal` 或 `dm`，默认 `normal` |
| `visibility` | 否 | `private` 或 `public`，默认 `private` |
| `originator` | 否 | 发起和协调主体；缺省为当前调用 Actor |
| `driver` | 是 | 主执行 Bot 和默认消息入口 |
| `participants` | 是 | 初始 GroupParticipant 列表；每项包含 `actor_id/actor_kind/role` |
| `strategy` | 否 | `chat`、`manager_worker` 或 `state_machine`，默认 `chat` |
| `collaboration` | 否 | state-machine 等显式协作定义的绑定信息 |

不允许输入 `created_by/status/version/gmt_create/gmt_modified`，也不允许在 Participant 中输入 `mode`。`driver` 必须是 BotParticipant，所有 Participant 必须满足 strategy 的角色约束。

输出：`201` 和完整 Group。

鉴权：要求 HumanCookie、Bot runtime token 或能够解析为 Bot 身份的 AgentPass token，不允许匿名创建。

- Human 创建时，`created_by` 取 Cookie 中的 `staff_no`；originator 只能是本人 HumanActor 或本人创建的 BotActor；
- Bot 创建时，token 身份必须等于 originator；`created_by` 从该 BotActor 的创建者信息派生；
- driver 和其他 Participant 仍需通过 Actor 存在性、可达性、可见性与 strategy 约束校验。

### 7.2 获取 Group 详情

```http
GET /openapi/bcn/v1/groups/{group_id}
```

输入：路径参数 `group_id`。

输出：`200` 和完整 Group。

鉴权：不允许匿名访问。GroupManager、直接 GroupParticipant，以及能够代表直接 BotParticipant 的 Human creator 可以读取；其他已认证 Actor 只能读取 `public` Group。仅作为 SessionParticipant 的 Actor 不因此获得完整 Group 详情权限，其可见范围由 Session API 承载。

### 7.3 更新 Group

```http
PATCH /openapi/bcn/v1/groups/{group_id}
```

输入至少包含以下一个字段：

```json
{
  "label": "代码评审二组",
  "context": "完成主干发布前的代码评审",
  "visibility": "private"
}
```

第一版只允许更新 `label/context/visibility`。`kind/strategy/created_by/originator/driver/status/version/participants` 不通过本接口修改。

输出：`200` 和更新后的完整 Group。

鉴权：仅 GroupManager。公开群还必须满足群类型和 Participant 可见性约束。

### 7.4 删除 Group

```http
DELETE /openapi/bcn/v1/groups/{group_id}
```

输入：路径参数 `group_id`，无请求体。

输出：

```json
{
  "group_id": "group_001",
  "deleted": true
}
```

鉴权：仅 GroupManager。该接口表示删除整个 Group；Participant 自己退出 Group 使用删除 Participant 接口。

### 7.5 添加 GroupParticipant

```http
POST /openapi/bcn/v1/groups/{group_id}/participants
```

输入：

```json
{
  "actor_id": "bot_worker",
  "actor_kind": "bot",
  "role": "worker"
}
```

`actor_id/actor_kind/role` 均为必填字段；不包含 `mode`。服务端校验 Actor 存在、actor_kind 一致、Participant 不重复、role 与 strategy 兼容，并检查可达性和公开群可见性约束。

输出：`201` 和新增的 GroupParticipant。

鉴权：仅 GroupManager。

### 7.6 删除 GroupParticipant

```http
DELETE /openapi/bcn/v1/groups/{group_id}/participants/{actor_id}
```

输入：路径参数 `group_id` 和 `actor_id`，无请求体。

输出：

```json
{
  "group_id": "group_001",
  "actor_id": "bot_worker",
  "deleted": true
}
```

鉴权：GroupManager 可以移除普通 Participant；Actor 自身或 BotActor 的 Human creator 可以代表该 Participant 退出 Group。不能直接移除 driver、originator 或 strategy 要求的必要角色。

### 7.7 修改 GroupParticipant 角色

```http
PATCH /openapi/bcn/v1/groups/{group_id}/participants/{actor_id}
```

本接口只修改 Participant 的群级 `role`：

```json
{
  "role": "worker"
}
```

`role` 是唯一允许的请求字段。第一版允许改为 `consultant`、`observer`、`manager` 或 `worker`；`driver` 与 Group 的 `driver` 字段绑定，不能通过本接口设置。不允许修改 `actor_id`、`actor_kind`，也不包含 `mode`。

输出：`200` 和更新后的 GroupParticipant。

```json
{
  "actor_id": "bot_worker",
  "actor_kind": "bot",
  "role": "worker"
}
```

鉴权：仅 GroupManager。服务端必须保证新角色与 Group strategy 兼容；driver、originator 及 strategy 必要角色不能通过本接口修改。涉及主执行者转移时，应使用后续独立的 Group 协调者变更能力，而不是通过本接口隐式完成。

GroupParticipant 不维护参与状态。`mode` 的变更统一由 SessionParticipant 接口承担。

通用错误语义：参数或领域约束不合法返回 `400`，缺少身份返回 `401`，权限不足返回 `403`，资源不存在返回 `404`，重复 Participant 等冲突返回 `409`。

## 8. Group 下的 Session 接口

Group 定义长期协作关系，Session 承载一次具体交互。Session 创建时以 GroupParticipant 生成初始 SessionParticipant 快照，之后独立维护参与者、角色和 `mode`；GroupParticipant 后续变化不回写正在运行的 Session。

### 8.1 创建 Session

```http
POST /openapi/bcn/v1/groups/{group_id}/sessions
```

输入：

| 字段 | 必填 | 含义 |
|---|---|---|
| `session_title` | 否 | Session 标题 |
| `input` | 否 | 仅 `service_invocation` 使用的结构化调用输入 |
| `meta` | 否 | 仅 `service_invocation` 使用的调用级扩展元数据 |
| `session_kind` | 否 | `chat` 或 `service_invocation`；state-machine Group 默认使用 `service_invocation`，其他 Group 默认使用 `chat` |
| `created_by` | 否 | Session creator Actor ID；缺省为当前调用 Actor，只允许本人或 Human 拥有的 Bot |
| `caller_role` | 否 | public Group 的非成员调用方加入 Session 时使用，不允许为 `driver` |

不直接输入 SessionParticipant。服务端从 GroupParticipant 创建快照，并为 Bot 和 Human 分别初始化合法的 SessionParticipant `mode`；public Group 的非成员调用方会作为本次 SessionParticipant 加入，但不会因此成为 GroupParticipant。

输出：`201` 和新创建的完整 Session。目标接口不接受已有 `session_id`，不提供 Reactivate。

```json
{
  "session_id": "group_001:abcdef12",
  "group_id": "group_001",
  "session_title": "主干发布评审",
  "status": "running",
  "session_kind": "chat",
  "participants": [
    {
      "actor_id": "bot_reviewer",
      "actor_kind": "bot",
      "role": "driver",
      "mode": "auto"
    },
    {
      "actor_id": "human_123456",
      "actor_kind": "human",
      "role": "observer",
      "mode": "present"
    }
  ],
  "created_by": "bot_reviewer",
  "gmt_create": 1784512800000,
  "gmt_modified": 1784512800000
}
```

鉴权：要求 HumanCookie、Bot runtime token 或能够解析为 Bot 身份的 AgentPass token，不允许匿名访问。

- private Group：调用 Bot 必须是 GroupParticipant；Human 必须是 GroupParticipant，或者是群内 BotParticipant 的创建者；
- public Group：任意已认证 Actor 可以创建 Session，但非 GroupParticipant 不能使用 `driver` 角色；
- 显式指定 `created_by` 时，调用方必须能代表该 Actor；
- `service_invocation` 还必须满足 Group collaboration/serviceSpec 的约束。

### 8.2 查询 Group Sessions

```http
GET /openapi/bcn/v1/groups/{group_id}/sessions
```

查询参数：

| 参数 | 含义 |
|---|---|
| `status` | 按 `running` 或 `completed` 过滤 |
| `q` | 按 `session_title` 模糊查询 |
| `offset` / `limit` | 分页参数 |

第一版不暴露可查询任意 Actor 的 `participant_id` 参数。服务端根据调用身份自动限定可见范围：

- GroupManager、直接 GroupParticipant，以及能够代表直接 BotParticipant 的 Human creator，可以查看该 Group 的全部 Session；
- 仅作为 SessionParticipant 的 Actor，只能查看自己实际参与的 Session；
- public Group 的其他已认证 Actor，只能查看自己实际参与的 Session；
- 匿名调用不允许。

输出：`200` 和分页 SessionSummary。

```json
{
  "group_id": "group_001",
  "items": [
    {
      "session_id": "group_001:abcdef12",
      "session_title": "主干发布评审",
      "status": "running",
      "session_kind": "chat",
      "participant_count": 3,
      "gmt_create": 1784512800000,
      "gmt_modified": 1784516400000
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 20
}
```

## 9. Group 邀请接口

Group invite 用于让 Human 成为长期 GroupParticipant；只希望临时加入一次会话时，应使用后续的 Session invite 接口。

### 9.1 创建 Group 邀请链接

```http
POST /openapi/bcn/v1/groups/{group_id}/invite-link
```

输入：路径参数 `group_id`，Body 可省略。

```json
{
  "ttl_seconds": 86400
}
```

`ttl_seconds` 为可选的有效期秒数；不传时使用服务端默认值。服务端必须限制允许的最大有效期。

输出：`200`。

```json
{
  "invite_token": "invite-token",
  "expires_at": 1784599200,
  "join_url": "https://bcn.alipay.com/openapi/bcn/v1/groups/join/invite-token"
}
```

鉴权：仅 GroupManager。Group 必须存在、处于 `active` 状态且不是 `dm`；invite token 必须签名、设置过期时间且不可包含可伪造的调用方身份。

### 9.2 通过邀请加入 Group

```http
POST /openapi/bcn/v1/groups/join/{token}
```

输入：路径参数 `token`，无请求体。

鉴权：仅 HumanCookie。Cookie 必须能够解析出当前人的 `staff_no`；不允许 Bot token 通过 invite link 加入。服务端确保对应 HumanActor 存在，然后将其作为 `actor_kind=human`、`role=consultant` 的 GroupParticipant 加入。

加入 Group 时不写入 `mode`，也不自动加入已经存在的 Session。后续创建 Session 时再初始化该 Human 的 SessionParticipant mode。

输出：`200`，接口具有幂等语义。

```json
{
  "joined": true,
  "already_member": false,
  "target_type": "group",
  "target_id": "group_001",
  "actor_id": "human_123456"
}
```

已经是 GroupParticipant 时返回 `joined=false`、`already_member=true`。token 无效返回 `401`，token 过期返回 `410`，Group 不存在返回 `404`，Group 不可加入返回 `409`。

## 10. Collaboration Template OpenAPI

Collaboration Template 是创建 `state_machine` 或其他显式协作 Group 时使用的模板资源。它不属于 Group 聚合，但属于建群流程依赖的外部 OpenAPI。

### 10.1 查询模板列表

```http
GET /openapi/bcn/v1/collaboration/templates
```

可选查询参数：

| 参数 | 含义 |
|---|---|
| `language` | 按模板语言过滤 |
| `tag` | 按模板标签过滤 |

输出模板列表，每项包含 `template_id`、`name`、`description`、`language`、`tags` 和 `definition`。该接口用于建群页面展示和选择协作模板。

鉴权：OpenAPI。模板为可公开读取的预置配置时允许匿名查询；如果后续支持租户或私有模板，应改为认证后按调用方可见范围过滤。

### 10.2 获取模板详情

```http
GET /openapi/bcn/v1/collaboration/templates/{template_id}
```

输入：路径参数 `template_id`，以及可选查询参数 `language`。

输出单个 Collaboration Template 的详情和协作定义，用于初始化建群时的 `collaboration` 配置。模板不存在时返回 `404`。

鉴权规则与模板列表接口一致。

## 11. StateMachineRun Internal API

状态机运行接口面向内部调试、运维、集成测试和 Provider downlink 联调，不作为外部 OpenAPI。外部产品应优先通过创建 Session 或 Service Invocation 触发协作，并通过相应 Session 接口查询业务结果。

### 11.1 启动状态机运行

```http
POST /api/bcn/v1/groups/{group_id}/state-machine-runs
```

作用：基于 Group 当前绑定的 CollaborationDefinition 启动一次独立的 StateMachineRun。

请求体可选：

```json
{
  "input": {},
  "metadata": {}
}
```

输出：`202` 和新创建的运行信息，主要包含 `run_id`、`group_id` 和 `status`。Group 不存在、不是有效的状态机 Group 或存在运行冲突时，分别返回 `404`、`400` 或 `409`。

### 11.2 查询状态机运行

```http
GET /api/bcn/v1/state-machine-runs/{run_id}
```

作用：查询 Run 的基本状态、执行结果和开始/完成时间。Run 不存在时返回 `404`。

### 11.3 查询运行图

```http
GET /api/bcn/v1/state-machine-runs/{run_id}/graph
```

作用：查询运行图的节点、边、节点状态和整体执行进度，用于内部观测和问题排查。

### 11.4 查询节点运行详情

```http
GET /api/bcn/v1/state-machine-runs/{run_id}/nodes/{node_id}
```

作用：查询单个节点的状态、输入、输出和错误信息。Run 或 Node 不存在时返回 `404`。

### 11.5 取消状态机运行

```http
POST /api/bcn/v1/state-machine-runs/{run_id}/cancel
```

作用：取消仍在运行的 StateMachineRun，并返回取消后的运行状态。Run 不存在返回 `404`，当前状态不允许取消时返回 `409`。

### 11.6 Internal API 鉴权边界

上述五个接口统一标记为 Internal API：

- 只允许受信任的内部服务身份或运维身份调用；
- 调用方必须具有目标 Group 或 Run 的内部访问权限；
- 不允许匿名访问，也不通过 HumanCookie 或普通 Bot token 直接对外开放；
- 当前底层裸路径尚未进行身份校验，正式收敛为 `/api/bcn/v1` 前必须由内部网关或服务端补充鉴权。

启动接口通过 Group 定位协作定义；创建之后，Run 是独立运行资源，因此查询、Graph、Node 和 Cancel 接口不继续嵌套在 Group 路径下。

## 12. 与当前实现的差异

当前 `GET /bots/{id}/groups` 同时返回直接参与的群，以及 Actor 仅作为 SessionParticipant 参与的群，且接口边界和鉴权尚未按 Actor 模型收敛。

目标设计的主要变化是：

- 路径从 Bot 维度统一为 Actor 维度；
- 直接群和 session-only 群拆成两个接口；
- Human 与 Bot 使用明确的 self/creator 鉴权；
- 不再提供可查询任意 Actor 的 `participant_id` 参数；
- GroupParticipant 不再包含 `mode`；`auto/muted/present/absent` 收敛到 SessionParticipant；
- 新的 Participant PATCH 只修改 `role`；当前 deprecated Group participant mode 接口不进入目标 OpenAPI；
- Group Session 列表要求认证并由服务端限制可见范围；当前实现未强制认证且支持任意 `participant` 查询；
- 目标 Session 创建接口不接受已有 `session_id`，当前 Reactivate 和 `activation_count` 只作为内部兼容能力保留；
- 当前 invite join 会受共享 Participant 结构影响写入 Group participant mode；目标接口只写群级 `actor_id/actor_kind/role`；
- Collaboration Template 保持外部 OpenAPI，通过 `/openapi/bcn/v1/collaboration/templates` 暴露；
- StateMachineRun 相关接口统一收敛为 `/api/bcn/v1` Internal API，当前底层裸路径及其无鉴权行为不能作为目标接口直接暴露；
- 通用 `GET /groups` 暂不进入目标 API。

产品迁移时，需要分别请求并展示“我的群组”和“临时参与的群聊”。建议先增加两个新接口并完成产品双列表读取，再调整旧接口。

## 13. 当前结论与后续讨论

本文已确认 Group 列表、创建、详情、更新、删除，Participant 添加、角色修改和删除，Group 下的 Session 创建与查询，Group invite-link 和 join，以及 Collaboration Template OpenAPI。StateMachineRun 的启动、查询、Graph、Node 和 Cancel 接口标记为 Internal API。第一版不提供 Group 生命周期接口，也不提供 Group 级 CollaborationDefinition 获取、绑定和升级接口。

Session 详情、生命周期、SessionParticipant、消息和 Session invite 已转入独立的 Session API 讨论稿继续收敛。其他 Group 内部控制接口不进入当前 OpenAPI 范围。
