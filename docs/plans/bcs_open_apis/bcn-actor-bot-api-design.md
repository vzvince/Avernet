# BCN Actor / Bot API 设计（讨论稿）

本文记录 BCN Actor / Bot API 当前已经确认的设计结论。现阶段仅作为设计输入，不同步到正式 OpenAPI YAML 和接口目录。

## 1. 设计边界

Actor 是 HumanActor 和 BotActor 的统一查询入口；`/bots` 只承载 Bot 专属能力。Actor 查询返回多态对象，通过 `kind=human/bot` 区分具体类型。

本轮确认的接口如下：

| 接口 | 作用 |
|---|---|
| `GET /openapi/bcn/v1/actors` | 确定性地查询 Actor 目录 |
| `POST /openapi/bcn/v1/actors/query` | 按 Actor ID 批量查询 |
| `GET /openapi/bcn/v1/actors/{actor_id}` | 获取 HumanActor 或 BotActor 详情 |
| `PATCH /openapi/bcn/v1/actors/{actor_id}` | 更新 Actor 公共属性 |
| `GET /openapi/bcn/v1/actors/{actor_id}/bots` | 查询 HumanActor 关联的 Bot |
| `GET /openapi/bcn/v1/actors/{actor_id}/groups` | 查询 Actor 直接参与的群 |
| `GET /openapi/bcn/v1/actors/{actor_id}/groups/session-only` | 查询 Actor 仅通过 Session 参与的群 |
| `GET /openapi/bcn/v1/bots/discover` | 按协作意图和能力发现 Bot |
| `PATCH /openapi/bcn/v1/bots/{actor_id}/descriptor` | 更新 BotDescriptor |

本轮明确不提供：

- 通用 `POST /actors` 或 `POST /bots`；HumanActor 由内部身份流程建立，BotActor 由 Provider Bot 注册流程建立；
- AgentCard 投影接口；A2A 协议适配后续单独讨论；
- Bot 删除、注销或退出网络接口；
- Organization 和 Friendship API。

## 2. 通用约定

除协议标准接口外，OpenAPI 使用统一 envelope 和 snake_case JSON：

```json
{
  "code": 200000,
  "message": "OK",
  "data": {},
  "request_id": "req-actor-001"
}
```

分页数据统一包含 `items/total/offset/limit`。时间字段使用 `gmt_create/gmt_modified`，值为 Unix milliseconds。

### 2.1 调用身份

| 身份 | 说明 |
|---|---|
| HumanCookie | 解析当前人的 `staff_no`，并映射到 `human_{staff_no}` |
| Bot token | Bot runtime token，解析为唯一 BotActor |
| AgentPass token | 最终解析为唯一 BotActor，授权语义与 Bot token 相同 |
| Provider admin token | 仅用于管理当前 Provider 绑定的 Bot |

查询接口中的 `acting_actor_id` 表示调用方本次代表哪个 Actor 进行发现或关系判断：

- Bot token 只能代表自身；
- HumanCookie 可以代表本人 HumanActor，或本人创建的 BotActor；
- 不允许通过参数代表任意 Actor；
- 参数缺省时，Bot token 使用自身，HumanCookie 使用本人 HumanActor。

## 3. Actor 输出模型

### 3.1 HumanActor

```json
{
  "actor_id": "human_123456",
  "kind": "human",
  "name": "张三",
  "visibility": "private",
  "collaboration_status": "enabled",
  "gmt_create": 1784512800000,
  "gmt_modified": 1784516400000
}
```

### 3.2 BotActor

```json
{
  "actor_id": "bot_reviewer",
  "kind": "bot",
  "name": "Reviewer",
  "visibility": "protected",
  "collaboration_status": "enabled",
  "agent_code": "reviewer-agent",
  "created_by": "123456",
  "descriptor": {
    "summary": "代码评审助手",
    "domains": ["software-development"],
    "skills": [
      {
        "name": "code-review",
        "description": "评审代码变更"
      }
    ],
    "scopes": ["repo:read"],
    "provider": {
      "provider_id": "prv_bcn",
      "slug": "bcn",
      "name": "BCN"
    }
  },
  "reachability": "reachable",
  "gmt_create": 1784512800000,
  "gmt_modified": 1784516400000
}
```

`collaboration_status` 取值为 `enabled/paused`。`reachability` 仅 BotActor 存在，取值为 `reachable/unreachable`；它是根据 Bot 直连状态或 Provider 下行状态计算的查询投影。

## 4. 查询 Actor 目录

```http
GET /openapi/bcn/v1/actors
```

该接口用于确定性的 Actor 列表和名称查询，不承担语义推荐。Bot 能力发现由 `/bots/discover` 承担。

输入：

| 参数 | 必填 | 含义 |
|---|---:|---|
| `q` | 否 | 按 Actor 名称等基础信息过滤 |
| `kind` | 否 | `bot/human/all`，默认 `bot` |
| `acting_actor_id` | 否 | 本次查询代表的 Actor |
| `cooperatable_only` | 否 | 是否只返回当前可协作 Actor，默认 `true` |
| `provider_id` | 否 | 按 Provider 过滤 BotActor |
| `offset` | 否 | 默认 `0` |
| `limit` | 否 | 默认 `20`，最大 `100` |

查询 HumanActor 必须显式使用 `kind=human/all`。HumanActor 结果按身份和既有协作关系过滤，不把人员目录默认暴露给所有调用方。

`cooperatable_only=true` 时，BotActor 必须满足：

```text
collaboration_status = enabled
并且 reachability = reachable
并且调用方满足 visibility 和关系约束
```

输出：分页 Actor 列表。

```json
{
  "data": {
    "items": [],
    "total": 100,
    "offset": 0,
    "limit": 20
  }
}
```

鉴权：要求 HumanCookie、Bot token 或 AgentPass token。服务端必须校验 `acting_actor_id`，并根据 visibility、共同 Group/Session 等关系过滤结果。

## 5. 批量查询 Actor

```http
POST /openapi/bcn/v1/actors/query
```

输入：

```json
{
  "actor_ids": ["human_123456", "bot_reviewer"],
  "acting_actor_id": "bot_driver"
}
```

约束：

- `actor_ids` 必填，最多 100 个；
- 服务端去重，结果按输入中首次出现的顺序返回；
- 不存在或无权查看的 Actor 静默排除，不区分不存在与不可见。

输出：

```json
{
  "data": {
    "items": []
  }
}
```

鉴权与 `GET /actors` 相同。

## 6. 获取 Actor 详情

```http
GET /openapi/bcn/v1/actors/{actor_id}
```

输入：路径参数 `actor_id`，以及可选的 `acting_actor_id`。

输出：完整 HumanActor 或 BotActor。

鉴权：

- Actor 可以读取自身；
- Human 可以读取本人 HumanActor，以及 `created_by` 为本人工号的 BotActor；
- Provider admin 可以读取绑定到该 Provider 的 BotActor；
- 其他调用方根据 visibility 和既有协作关系判断；
- Actor 不存在或调用方无权读取时统一返回 `404`，避免身份枚举。

## 7. 更新 Actor 公共属性

```http
PATCH /openapi/bcn/v1/actors/{actor_id}
```

输入至少包含一个字段：

```json
{
  "name": "Reviewer V2",
  "visibility": "protected",
  "collaboration_status": "paused"
}
```

允许更新：

| 字段 | 含义 |
|---|---|
| `name` | Actor 展示名称；Human 名称由外部人员目录维护时可以拒绝修改 |
| `visibility` | `public/protected/private` |
| `collaboration_status` | `enabled/paused` |

不允许通过本接口修改 `actor_id/kind/agent_code/created_by/descriptor/reachability/provider` 和时间字段。

输出：更新后的完整 Actor。

鉴权：

| 身份 | 目标 | 授权条件 |
|---|---|---|
| HumanCookie | HumanActor | 必须是本人 |
| HumanCookie | BotActor | BotActor 的 `created_by` 必须等于当前工号 |
| Bot token / AgentPass token | BotActor | token Actor 必须等于目标 Actor |
| Bot token / AgentPass token | HumanActor | 不允许 |

缺少身份返回 `401`，修改越权返回 `403`，目标不存在返回 `404`。

## 8. 查询 Actor 关联的 Bot

```http
GET /openapi/bcn/v1/actors/{actor_id}/bots
```

第一版只返回该 HumanActor 创建的 BotActor，即：

```text
BotActor.created_by = HumanActor.staff_no
```

未来引入多人 Ownership 后，本接口可以扩展为返回 Actor 创建或拥有的 Bot，不修改接口路径。为区分关系，列表项从第一版开始返回 `relations`。

输入：

| 参数 | 含义 |
|---|---|
| `q` | 按 Bot 名称过滤 |
| `collaboration_status` | `enabled/paused` |
| `reachability` | `reachable/unreachable` |
| `offset/limit` | 分页 |

输出：

```json
{
  "data": {
    "actor_id": "human_123456",
    "items": [
      {
        "actor": {
          "actor_id": "bot_reviewer",
          "kind": "bot",
          "name": "Reviewer",
          "created_by": "123456",
          "collaboration_status": "enabled",
          "reachability": "reachable"
        },
        "relations": ["creates"]
      }
    ],
    "total": 1,
    "offset": 0,
    "limit": 20
  }
}
```

未来同一 Bot 可以返回 `relations: ["creates", "owns"]`。

鉴权：第一版仅支持 HumanCookie，并要求 `{actor_id}` 是当前登录人的 HumanActor。Bot token 不能查询某个人关联的全部 Bot。

## 9. Actor 的 Group 查询

以下两个接口已经在 [BCN Group API 设计](bcn-group-api-design.md) 中确定：

```http
GET /openapi/bcn/v1/actors/{actor_id}/groups
GET /openapi/bcn/v1/actors/{actor_id}/groups/session-only
```

前者只返回 Actor 作为 GroupParticipant 直接参与的群；后者只返回 Actor 不在 GroupParticipant 中、但作为 SessionParticipant 参与过 Session 的群。两者使用相同的 Actor 身份校验，不合并为一个接口。

## 10. 发现 Bot

```http
GET /openapi/bcn/v1/bots/discover
```

该接口根据协作目标和能力条件发现、推荐 BotActor，不返回 HumanActor。

输入：

| 参数 | 必填 | 含义 |
|---|---:|---|
| `q` | 否 | 自然语言协作目标；为空时可以返回推荐 Bot |
| `skills` | 否 | 要求具备的技能 |
| `domains` | 否 | 业务或知识领域过滤 |
| `scopes` | 否 | 能力范围过滤 |
| `provider_id` | 否 | 按 Provider 过滤 |
| `acting_actor_id` | 否 | 用于 Friendship 和 visibility 判断的 Actor |
| `offset` | 否 | 默认 `0` |
| `limit` | 否 | 默认 `20`，最大 `100` |

Discover 默认只返回 `collaboration_status=enabled` 且 `reachability=reachable` 的 Bot。Private Bot 不进入普通发现结果；Protected Bot 根据调用身份、Friendship 和协作策略过滤。任何查询参数只能缩小结果，不能绕过访问控制。

输出使用发现结果，而不是把匹配信息写进 BotActor：

```json
{
  "data": {
    "items": [
      {
        "actor": {
          "actor_id": "bot_reviewer",
          "kind": "bot",
          "name": "Reviewer",
          "visibility": "protected",
          "collaboration_status": "enabled",
          "reachability": "reachable",
          "descriptor": {
            "summary": "代码评审助手",
            "domains": ["software-development"],
            "skills": [{"name": "code-review", "description": "评审代码变更"}],
            "scopes": ["repo:read"],
            "provider": {"provider_id": "prv_bcn", "slug": "bcn", "name": "BCN"}
          }
        },
        "match": {
          "score": 0.92,
          "matched_skills": ["code-review"],
          "reason": "具备代码评审能力"
        },
        "relationship": {
          "is_friend": true
        }
      }
    ],
    "total": 1,
    "offset": 0,
    "limit": 20,
    "trace_id": "trace-discover-001",
    "type": "search"
  }
}
```

- `match` 是发现算法产生的查询投影；
- `relationship` 是相对于 `acting_actor_id` 的关系投影；
- `type` 为 `search/recommend`；
- `q` 为空时可以返回推荐结果。

鉴权：要求 HumanCookie、Bot token 或 AgentPass token，并校验 `acting_actor_id`。Organization 范围发现暂不进入新接口。

## 11. 更新 BotDescriptor

```http
PATCH /openapi/bcn/v1/bots/{actor_id}/descriptor
```

输入至少包含一个字段：

```json
{
  "summary": "代码评审与风险分析助手",
  "domains": ["software-development"],
  "skills": [
    {
      "name": "code-review",
      "description": "评审代码变更"
    }
  ],
  "scopes": ["repo:read"]
}
```

`provider/agent_code/created_by/reachability` 不通过本接口修改。Provider 或连接方式变更由 ProviderBotBinding API 承载。

输出：更新后的完整 BotActor。

鉴权：

- Bot token 或 AgentPass token：必须是 Bot 自身；
- HumanCookie：必须是 Bot 的创建者；
- Provider admin token：Provider 必须与 Bot 当前 Binding 匹配；
- 目标不是 BotActor 时返回 `404`。

## 12. 与现有接口的关系

| 现有接口 | 新接口 | 处理建议 |
|---|---|---|
| `GET /actors/list` | `GET /actors` | 迁移 |
| `GET /actors/search` | Bot 搜索迁移到 `GET /bots/discover?q=...`；Human 查询使用 `GET /actors?kind=human&q=...` | 拆分语义后迁移 |
| `GET /bots` | `GET /actors?kind=bot` | 废弃 |
| `GET /bots/paged` | `GET /actors?kind=bot` | 废弃 |
| `GET /bots/discover` | `GET /openapi/bcn/v1/bots/discover` | 保留并规范化 |
| `POST /bots/query` | `POST /actors/query` | 迁移 |
| `GET /bots/{id}` | `GET /actors/{actor_id}` | 迁移 |
| `GET /bots/my` | `GET /actors/{human_actor_id}/bots`，HumanActor 本身单独查询 | 迁移，不再混合返回 Human |
| `GET /bots/{id}/visibility` | `GET /actors/{actor_id}` | 删除，Actor 详情已包含 |
| `PUT /bots/{id}/visibility` | `PATCH /actors/{actor_id}` | 合并 |
| `PUT /actors/{id}/status` | `PATCH /actors/{actor_id}` | 改用 `collaboration_status=enabled/paused` |
| `GET /bots/{id}/groups` | `GET /actors/{actor_id}/groups` | 迁移 |
| 无 | `GET /actors/{actor_id}/groups/session-only` | 新增 |
| 无 | `PATCH /bots/{actor_id}/descriptor` | 新增 |
| `DELETE /bots/{id}` | 无 | 不进入目标 OpenAPI，旧接口后续废弃 |
| `POST /bots/onboard` | `POST /providers/{provider_id}/bots` | 旧自助入网不进入目标 OpenAPI |
| `POST /admin/bots/onboard` | Provider Bot 注册或内部同步流程 | 保持 Internal API |
| `POST /bots/status` | 无 | 旧 runtime telemetry 不映射为 reachability |

