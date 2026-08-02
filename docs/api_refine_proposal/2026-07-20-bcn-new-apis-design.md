# BCN New APIs 文档设计

## 目标

在 `src/bcs-internal/docs/new_apis` 中建立独立于历史 `docs/api` 的目标态 OpenAPI 3.1 文档，覆盖已经确认的 Actor/Bot、Friendship、Group、Session、Invitation、Provider、Bot Registration、Collaboration Template 和 StateMachineRun 接口。

## 文档结构

```text
new_apis/
├── README.md
├── _shared.yaml
├── domain-models.yaml
├── openapi/
│   ├── actors-bots.yaml
│   ├── friendships.yaml
│   ├── groups.yaml
│   ├── sessions.yaml
│   ├── invitations.yaml
│   ├── providers.yaml
│   └── bot-registration.yaml
├── internalapi/
│   ├── providers.yaml
│   └── state-machine-runs.yaml
└── serve_api_docs.py
```

`domain-models.yaml` 是领域对象 wire schema 的唯一来源。`_shared.yaml` 只维护 Envelope、分页、安全方案、公共参数和通用错误。业务 YAML 维护 operation、输入 DTO、输出 DTO 和业务错误。

## 领域模型

领域模型文档单独作为完整 OpenAPI 文档发布，`paths` 为空，`components.schemas` 定义：

- Actor、HumanActor、BotActor、BotDescriptor、ProviderReference；
- Provider、ProviderBotBinding；
- Friendship、FriendRequest；
- Group、GroupSummary、GroupParticipant、RoutingPolicy、CollaborationBinding、ServiceSpec；
- Session、SessionSummary、SessionParticipant、Invocation、Message；
- CollaborationTemplate、StateMachineRun、StateMachineGraph、StateMachineNodeRun。

所有属性包含类型和中文 `description`。必须始终存在的字段列入 `required`；条件字段通过 `oneOf`、枚举和说明表达。HumanActor/BotActor 使用 discriminator；Chat/ServiceInvocation Session 使用 `oneOf` 表达 `invocation` 的存在条件。

## API 分类

OpenAPI 使用 `/openapi/bcn/v1`，Internal API 使用 `/api/bcn/v1`。两个范围使用相同业务 tag，因此合并视图中同一类目同时显示外部和内部接口。每个 operation 添加 `x-api-type: openapi` 或 `x-api-type: internal`。

Friendship 第一版仅允许 BotActor 之间建立关系，包含好友列表、申请、申请查询、接受、拒绝和解除好友。目标 Bot 为 public 时自动建交，protected 时产生 pending 请求，private 时返回 403。

## Swagger 视图

文档 Server 默认显示 `BCN All APIs`，并支持四种视图：

- `/all-apis.yaml`：OpenAPI 与 Internal API 合并；
- `/openapi.yaml`：只包含 OpenAPI；
- `/internalapi.yaml`：只包含 Internal API；
- `/domain-models.yaml`：只展示领域对象。

Swagger 首页通过下拉框切换视图，`/domain-models/` 直接打开领域模型视图。合并器解析 `_shared.yaml` 和 `domain-models.yaml` 的跨文件 `$ref`，校验 path、operationId、tag 和 component 冲突，并确保输出不残留外部引用。

## 契约完整性

每个 operation 明确：

- 业务作用；
- path、query、header、cookie 和 body 输入；
- 每个字段的必填性、类型、枚举、默认值和含义；
- 鉴权方式、代表 Actor 的约束和资源级授权；
- 成功及常见失败响应；
- 因输入或资源状态而变化的返回结构及对应示例。

所有 REST 响应使用 `{code, message, data, request_id}`。凭证只允许在 Header 或创建成功响应中出现，不进入 URL query、日志或普通查询响应。

