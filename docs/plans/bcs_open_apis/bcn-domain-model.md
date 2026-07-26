# BCN 领域模型

> 状态：讨论稿。本文描述领域边界和核心对象；Actor、Group、Session 部分补充 High Level 字段和数据库映射，不涉及接口或代码实现。

## 1. 核心领域

核心领域名称确定为：

> **BCN（Bot Coordination Network）**

BCN 是业务领域，不是领域对象。BCS（Bot Coordination Service）是承载和实现 BCN 能力的服务。

BCN 的核心作用是：

> **连接 Actor、组织 Group、运行 Session。**

## 2. 核心领域对象

| 对象 | 作用 |
|---|---|
| Actor | BCN 网络中可被唯一识别、授权并参与协作的主体 |
| Group | 组织多个 Actor，定义成员、角色和协作方式 |
| Session | 承载 Actor 在特定上下文中的持续交互过程 |

`Message` 是 Session 的从属对象。第一版不引入独立的 BCN Task 领域对象；ServiceInvocation Session 可以投影为 A2A Task，其输出可以投影为 A2A Artifact。

Provider、Friendship、Invitation 等属于支撑领域；通信协议、认证、监控和存储不属于核心领域模型。

## 3. Group 模型

### 3.1 作用与定义

Group 的作用是组织多个 Actor，在共同上下文中确定参与关系、协作方式和运行规则。

> **Group 是 BCN 中组织 Actor 开展长期协作的空间。它定义参与者、角色和协作规则，但不承载一次具体交互过程。**

具体交互由 Session 承载，Message 归属 Session；A2A Task 和 Artifact 由 ServiceInvocation Session 的标准视图提供。BCN 不新增独立的 Collaboration 聚合；CollaborationDefinition 是 Group 可选的协作能力描述。

### 3.2 结构

```text
Group
├── groupId
├── version
├── label
├── context
├── kind
├── status
├── visibility
├── createdBy
├── originator
├── driver
├── participants: GroupParticipant[]
│   ├── actorId
│   ├── actorKind
│   └── role
├── strategy
├── routingPolicy?
│   ├── mode
│   └── defaultDelivery
├── collaboration?
│   ├── definitionRef
│   │   ├── id
│   │   └── version
│   ├── participantBindings
│   │   └── <slot>
│   │       ├── source
│   │       ├── botIds
│   │       └── extensions
│   └── autoStartOnServiceInvocation
├── serviceSpec?
│   ├── callbackConfig
│   │   └── channels[]
│   ├── timeoutSeconds
│   └── maxConcurrency
├── gmtCreate
└── gmtModified
```

### 3.3 字段含义

| 字段 | 含义 |
|---|---|
| `groupId` | Group 唯一标识 |
| `version` | Group 配置版本；当前实现恒为 `1`，为协作绑定和运行快照预留 |
| `label` | 群名称 |
| `context` | 建群时确定的协作目标、背景和上下文；不再单独保留持久化 `topic` |
| `kind` | `normal` 或 `dm` |
| `status` | `active`、`completed`、`error`、`closed`、`inactive` |
| `visibility` | `private` 或 `public` |
| `createdBy` | 实际创建人的工号，用于审计 |
| `originator` | 群的发起和协调主体，可以是 Human 或 Bot；缺省为 `driver` |
| `driver` | 群的主执行 Bot 和默认消息入口 |
| `participants` | 群内 Actor 的长期成员关系及群级角色 |
| `strategy` | `chat`、`manager_worker` 或 `state_machine` |
| `routingPolicy` | 可选的群级消息路由规则 |
| `collaboration` | 可选的 CollaborationDefinition 绑定，主要用于显式协作定义 |
| `serviceSpec` | 可选的 Service-as-a-Group 配置 |
| `gmtCreate` | 创建时间，对应数据库 `gmt_create` |
| `gmtModified` | 最后修改时间，对应数据库 `gmt_modified` |

嵌套对象的字段含义如下：

| 对象 | 字段 | 含义 |
|---|---|---|
| `GroupParticipant` | `actorId` | Actor 唯一标识；当前数据库沿用历史列名 `bot_uuid` |
|  | `actorKind` | `bot` 或 `human` |
|  | `role` | `driver`、`consultant`、`observer`、`manager`、`worker` |
| `RoutingPolicy` | `mode` | `structured`、`mention` 或 `hybrid` |
|  | `defaultDelivery` | `send_to_driver` 或 `inject_observers` |
| `CollaborationBinding` | `definitionRef` | CollaborationDefinition 的 `id` 和 `version` |
|  | `participantBindings` | Definition 参与者槽位到实际 Bot 的绑定 |
|  | `source` | 当前为 `manual` |
|  | `botIds` | 绑定到槽位的 Bot 列表 |
|  | `extensions` | 可选扩展属性 |
|  | `autoStartOnServiceInvocation` | Service Invocation 创建后是否自动运行定义 |
| `ServiceSpec` | `callbackConfig.channels` | 回调通道；当前支持 AntDing 和 BaaS 配置 |
|  | `timeoutSeconds` | 单次 Service Invocation 超时时间；空表示不限制 |
|  | `maxConcurrency` | 同时运行的 Service Invocation 数量；空表示不限制 |

AntDing 回调通道包含 `type`、`accessKeyId`、`accessKeySecret`、`robotCode`，以及可选的 `userId`、`openConversationId`。BaaS 回调通道包含 `type`、`baseUrl`、`apiKey`、`botId` 和可选的 `metadata`。敏感凭据不应在查询接口中原样返回。

`auto`、`muted`、`present`、`absent` 描述 Actor 在一次具体 Session 中的参与状态，属于 `SessionParticipant.mode`，不属于 GroupParticipant。Session 创建时可以根据 GroupParticipant 生成初始参与者，但之后独立维护 mode，Group 层变化不修改正在运行的 Session。

### 3.4 数据库映射

Group 使用混合持久化：核心字段保存在 `bcs_groups`，参与者和版本化协作绑定使用独立表，小型配置随 Group 以 JSON 保存。

| 领域字段 | 表 | 字段/存储 | 当前情况 |
|---|---|---|---|
| `groupId` | `bcs_groups` | `group_id` | 已接入 |
| `version` | `bcs_groups` | `version` | 已接入，当前恒为 `1` |
| `label` | `bcs_groups` | `label` | 已接入 |
| `context` | `bcs_groups` | `context` | 已接入 |
| `kind` | `bcs_groups` | `group_kind` | 已接入 |
| `status` | `bcs_groups` | `status` | 已接入 |
| `visibility` | `bcs_groups` | `visibility` | 已接入 |
| `createdBy` | `bcs_groups` | `created_by` | 列已存在，当前 Group 实体和 Store 尚未读写 |
| `originator` | `bcs_groups` | `originator` | 已接入；数据库可空，领域上回退到 `driver` |
| `driver` | `bcs_groups` | `driver_bot` | 已接入 |
| `participants` | `bcs_group_participants` | `bot_uuid`、`actor_kind`、`role` | 独立表为权威来源；`mode` 不进入目标 Group 模型 |
| `strategy` | `bcs_groups` | `group_strategy` | 已接入 |
| `routingPolicy` | `bcs_groups` | `routing_policy_json` | JSON 存储 |
| `collaboration.definitionRef` | `bcs_group_runtime_bindings` | `default_definition_id`、`default_definition_version` | 按 Group 版本绑定 |
| `collaboration.participantBindings` | `bcs_group_runtime_bindings` | `participant_bindings_json` | JSON 存储 |
| `collaboration.autoStartOnServiceInvocation` | `bcs_group_runtime_bindings` | `auto_start_on_service_invocation` | 已存在 |
| CollaborationDefinition 内容 | `bcs_collaboration_definitions` | Definition 元数据、版本和内容 | 独立生命周期；大内容可使用 `bcs_collaboration_definition_blobs` |
| `serviceSpec` | `bcs_groups` | `service_spec` | JSON 存储 |
| `gmtCreate` | `bcs_groups` | `gmt_create` | 已存在 |
| `gmtModified` | `bcs_groups` | `gmt_modified` | 已存在 |

目标字段在现有数据库中没有物理字段缺失，第一版不需要新增表。当前需要关注的差异是：

- `created_by` 已存在，但当前 Group Store 没有读写；
- `bcs_groups.participants` 是历史冗余列，参与者应以 `bcs_group_participants` 为准；
- `bcs_group_participants.bot_uuid` 实际承载 Actor ID，包括 HumanActor；
- `bcs_group_participants.mode` 是当前实现的兼容字段；目标领域将 mode 收敛到 SessionParticipant，Group API 不再读写该字段；
- `version` 和版本化 CollaborationBinding 已建模，但 Group 版本演进尚未启用；
- 当前代码实体使用 `created_at`、`updated_at`，目标领域字段保持数据库语义，使用 `gmtCreate`、`gmtModified`；
- `env`、`dm_pair_key`、`service_group_uuid`、`service_mode`、`record_status`、`lifecycle_status` 属于环境隔离、派生索引或兼容字段，不进入 High Level Group 结构；
- Session、Message 和协作运行状态分别保存在 `bcs_group_sessions`、`bcs_messages` 和 `bcs_state_machine_*` 等表中，不属于 Group 字段。

## 4. Session 模型

### 4.1 作用与定义

> **Session 是 Group 中一次相互隔离的协作上下文。**

Session 以 GroupParticipant 为初始参与者快照，之后独立维护 SessionParticipant 和 Message。GroupParticipant 的变化不修改已经存在的 Session。

Session 分为两类：

- `chat`：持续交互会话，主要承载 Participant 和 Message；
- `service_invocation`：一次独立服务调用，在 Session 上附加 Invocation 信息。

第一版遵循“一次服务调用创建一个新 Session”，不向目标 OpenAPI 暴露 Reactivate。需要共享上下文时，应由调用方显式选择同一上下文能力；在出现一个 Session 管理多个独立执行的明确需求前，不新增 Task 聚合和 Task API。

### 4.2 结构

```text
Session
├── sessionId
├── groupId
├── groupVersion
├── title
├── kind
├── status
├── participants: SessionParticipant[]
│   ├── actorId
│   ├── actorKind
│   ├── role
│   └── mode
├── invocation?
│   ├── input
│   ├── output?
│   ├── errorMessage?
│   ├── externalCallerId?
│   ├── callbackStatus?
│   └── metadata?
├── messages: Message[]
├── createdBy
├── gmtCreate
├── gmtModified
└── completedAt?
```

### 4.3 字段含义与约束

| 字段 | 含义 |
|---|---|
| `sessionId` | Session 唯一标识 |
| `groupId` | 所属 Group |
| `groupVersion` | 创建时固定的 Group 配置版本 |
| `title` | 可展示的 Session 标题 |
| `kind` | `chat` 或 `service_invocation` |
| `status` | `running` 或 `completed` |
| `participants` | Session 内实际参与者快照，可独立于 GroupParticipant 演化 |
| `invocation` | 仅 `service_invocation` Session 存在的服务调用信息 |
| `messages` | Session 内按顺序产生的消息；作为从属资源单独查询，不内嵌在 Session 详情中 |
| `createdBy` | 创建 Session 的主体标识；Chat 通常为 Actor，外部服务调用当前可能为服务端生成的 principal |
| `gmtCreate` | 创建时间 |
| `gmtModified` | 最后修改时间 |
| `completedAt` | 完成时间 |

SessionParticipant 字段如下：

| 字段 | 含义 |
|---|---|
| `actorId` | Actor 唯一标识；当前数据库兼容列名为 `bot_uuid` |
| `actorKind` | `bot` 或 `human` |
| `role` | Session 内角色，可独立于 GroupParticipant role 演化 |
| `mode` | Bot 使用 `auto/muted`；Human 使用 `present/absent` |

Invocation 字段如下：

| 字段 | 含义 |
|---|---|
| `input` | 服务调用输入 |
| `output` | 服务调用成功结果 |
| `errorMessage` | 服务调用失败信息 |
| `externalCallerId` | 调用方提供的业务归因标识；当前实现字段为 `caller_id`，不参与鉴权 |
| `callbackStatus` | 回调状态，例如 `pending`、`succeeded`、`partial_failed`、`failed` |
| `metadata` | 调用级扩展 JSON，例如 request ID、来源系统和 callback target；不参与鉴权或核心业务判断 |

领域不变量：

- `kind=chat` 时 `invocation` 必须为空；
- `kind=service_invocation` 时 `invocation` 必须存在；
- `externalCallerId` 是调用方声明的业务标识，不等同于 Actor ID；
- 鉴权和调用隔离使用服务端生成的 `callerPrincipal`，它是内部安全字段，不进入公开 Invocation 模型；
- `activationCount` 属于当前 Reactivate 兼容机制，不进入目标 High Level Session 模型。

### 4.4 数据库映射

Session 的核心字段和 Invocation 字段当前都平铺存储在 `bcs_group_sessions`。`invocation` 是领域分组，不要求新增物理表。

| 领域字段 | 表 | 字段/存储 | 当前情况 |
|---|---|---|---|
| `sessionId` | `bcs_group_sessions` | `session_id` | 已接入 |
| `groupId` | `bcs_group_sessions` | `group_id` | 已接入 |
| `groupVersion` | `bcs_group_sessions` | `group_version` | 已接入 |
| `title` | `bcs_group_sessions` | `session_title` | 已接入 |
| `kind` | `bcs_group_sessions` | `session_kind` | 已接入 |
| `status` | `bcs_group_sessions` | `status` | 已接入 |
| `participants` | `bcs_group_sessions` | `participants` | JSON 快照，包含完整 Participant 信息 |
| Participant 查询关系 | `bcs_session_participants` | `bot_uuid`、`role` | 支持按参与者查询；当前不保存 `actor_kind` 和 `mode` |
| `invocation.input` | `bcs_group_sessions` | `input` | JSON 文本 |
| `invocation.output` | `bcs_group_sessions` | `output` | JSON 文本 |
| `invocation.errorMessage` | `bcs_group_sessions` | `error_message` | 已接入 |
| `invocation.externalCallerId` | `bcs_group_sessions` | `caller_id` | 已接入，不参与鉴权 |
| `invocation.callbackStatus` | `bcs_group_sessions` | `callback_status` | 已接入 |
| `invocation.metadata` | `bcs_group_sessions` | `meta` | JSON 文本 |
| 内部 `callerPrincipal` | `bcs_group_sessions` | `caller_principal` | Service Key/Bot 身份隔离，不进入公开模型 |
| `createdBy` | `bcs_group_sessions` | `created_by` | 已接入；当前 Chat 与服务调用的取值语义不同 |
| `gmtCreate` | `bcs_group_sessions` | `gmt_create` | 已存在 |
| `gmtModified` | `bcs_group_sessions` | `gmt_modified` | 已存在 |
| `completedAt` | `bcs_group_sessions` | `completed_at` | 已存在 |
| `messages` | `bcs_messages` | `session_id`、`session_seq` 等 | 独立表，按 Session 隔离和排序 |

当前数据库不需要新增字段。目标模型与实现的主要差异是：Rust Session 和数据库目前将 Invocation 字段平铺；Chat 兼容路径仍允许写入 `input/output/error_message`；`activation_count` 和 Reactivate 仍存在。目标 API 将这些调用字段限定到 ServiceInvocation Session，并逐步收敛旧兼容路径。

## 5. A2A 与 ARD 对齐

BCN 保留自己的原生领域模型，通过标准视图兼容 A2A 和 ARD，不直接把外部协议 Schema 作为领域对象。

```text
BCN 原生领域对象
    ├── A2A 标准视图
    └── ARD 标准视图
```

| BCN 对象 | 标准对齐 |
|---|---|
| BotActor | 可生成 A2A AgentCard，并作为 ARD 的发现对象 |
| HumanActor | 不生成 AgentCard，不通过外部 ARD 发布 |
| Group | A2A/ARD 没有直接对应，是 BCN 的核心扩展 |
| Chat Session | 对齐 A2A `contextId` 所表达的持续上下文 |
| ServiceInvocation Session | 可以投影为 A2A Task；第一版不新增 BCN Task 领域对象 |
| Message | 对齐 A2A Message |
| Invocation output | 可以投影为 A2A Artifact |

ARD 当前仅用于发现 Actor，不扩展到 MCP Server、Skill、API 或 Workflow 等其他资源。

A2A 的 Task、Message、Part、Artifact、上下文和标准操作通过 BCN 原生模型的标准视图提供，不要求逐一复制为 BCN 核心对象。

## 6. Actor 模型

### 6.1 定义与类型

Actor 的定义确定为：

> **Actor 是 BCN 网络中可被唯一识别、授权并参与协作的主体。**

Actor 是 BCN 的网络身份，不等同于 A2A Agent。

```text
Actor
├── HumanActor implements Actor
└── BotActor implements Actor
```

- HumanActor 表示参与 BCN 协作的人。
- BotActor 表示参与 BCN 协作的软件主体。
- Bot 保持为 BCN 的领域概念，不改名为 Agent。
- 只有 BotActor 具有 BotDescriptor，并可提供 AgentCard 标准视图。
- 当前不扩展 MCPActor、WorkflowActor 等其他 Actor 类型。

领域上 HumanActor 和 BotActor 是 Actor 的具体子类型；具体编程语言如何表达这种关系不属于领域模型。

### 6.2 结构

```text
Actor
├── actorId
├── kind
├── name
├── visibility
├── collaborationStatus
├── gmtCreate
└── gmtModified

HumanActor implements Actor
└── 第一版无额外字段

BotActor implements Actor
├── agentCode?
├── createdBy
├── descriptor: BotDescriptor
│   ├── summary
│   ├── domains[]
│   ├── skills[]
│   ├── scopes[]
│   └── provider: ProviderReference
│       ├── providerId
│       ├── slug
│       └── name
└── reachability
    ├── reachable
    └── unreachable
```

HumanActor 第一版不引入 HumanProfile、HumanSpec 和 ActorIdentityBinding。BotDescriptor 回答“这个 Bot 能做什么、由谁提供”，不承载运行时连接、凭证或内部路由配置。

### 6.3 字段含义

Actor 公共字段如下：

| 字段 | 含义 |
|---|---|
| `actorId` | Actor 在 BCN 中的权威唯一标识 |
| `kind` | `human` 或 `bot` |
| `name` | Actor 展示名称 |
| `visibility` | Actor 的发现和协作可见范围 |
| `collaborationStatus` | `enabled` 或 `paused`；表达是否接受 BCN 协作 |
| `gmtCreate` | 创建时间 |
| `gmtModified` | 最后修改时间 |

BotActor 专有字段如下：

| 字段 | 含义 |
|---|---|
| `agentCode` | Bot 在外部 Agent 平台或安全网关中的身份标识；第一版可选，目标语义为环境内唯一 |
| `createdBy` | 创建 Bot 的 Human 工号；表达不可变的创建来源，不等同于可变的所有权关系 |
| `descriptor` | Bot 的能力与提供方描述 |
| `reachability` | `reachable` 或 `unreachable`；表达 BCN 当前能否将消息送达 Bot |

BotDescriptor 字段如下：

| 字段 | 含义 |
|---|---|
| `summary` | Bot 的简要用途说明 |
| `domains` | Bot 擅长的业务或知识领域 |
| `skills` | Bot 可以完成的能力列表 |
| `scopes` | Bot 可以访问或操作的范围 |
| `provider` | Bot 所属 Provider 的稳定引用；目标模型中每个 Bot 必须关联一个 Provider |

ProviderReference 只包含 `providerId`、稳定可读的 `slug` 和展示用 `name`。Provider 的完整配置、下行地址和凭证不进入 BotDescriptor。

### 6.4 创建与 Provider 关系

第一版 Human 与 Bot 的关系采用创建语义：

```text
HumanActor ── creates ──> BotActor
```

一个 HumanActor 可以创建多个 BotActor；一个 BotActor 记录一个不可变的 `createdBy`。未来如需多人管理，再单独增加可变的 `owners[]`，不改变创建来源。

Provider 表达 Bot 的提供方。所有 Bot 通过 ProviderBotBinding 关联 Provider；BCN 原生 Bot 使用默认 BCN Provider。

```text
Provider
├── providerId
├── slug
├── name
└── supportedConnTypes[]
    ├── plugin
    └── gateway

ProviderBotBinding
├── actorId
├── providerId
├── providerBotRef
└── connType
    ├── plugin
    └── gateway
```

- `plugin` 表示 Bot Runtime 通过 BCN Plugin 直接连接；
- `gateway` 表示 BCN 通过 Provider 下行网关投递；
- 同一 Provider 可以支持两种接入方式，不同 Bot 独立选择；
- 一个 Bot 当前只有一个有效 Binding，并且只选择一种 `connType`。

ProviderBinding 是提供方关联和投递选择，不属于 BotDescriptor。BotDescriptor 中的 ProviderReference 是对该关联的对外描述投影。

### 6.5 与当前实现的映射

| 领域字段 | 当前存储/实现 | 当前差异 |
|---|---|---|
| `actorId` | `bcs_bots.bot_uuid` | Human 与 Bot 当前共用该表 |
| `kind` | `bcs_bots.actor_kind` | 已支持 `human/bot` |
| `name` | `bcs_bots.name` | 已接入 |
| `visibility` | `bcs_bots.visibility` | 已接入 |
| `collaborationStatus` | `bcs_bots.status` | `enabled/paused` 分别映射当前 `online/hidden` |
| `gmtCreate/gmtModified` | `bcs_bots.gmt_create/gmt_modified` | 已存在 |
| `agentCode` | `bcs_bots.agent_code` | 当前可选且数据库未建立环境内唯一约束 |
| `createdBy` | `bcs_bots.created_by` | 当前保存创建人工号 |
| `descriptor` | `bcs_bots.bot_info` | 当前 JSON 已保存 summary、domains、skills、scopes 等能力信息 |
| `descriptor.provider` | `bcs_provider_bot_bindings` + `bcs_providers` | 当前直连 Bot 可以没有 Binding；目标模型使用默认 BCN Provider 补齐 |
| `reachability` | Actor 状态、Bot 连接或 Provider 下行状态的派生结果 | 不新增持久化字段 |
| `ProviderReference.slug` | 当前无对应字段 | 目标 Provider 模型新增 |
| `Provider.supportedConnTypes` | 当前无显式字段 | 现有实现通过有无 Binding/Downlink 隐式判断 |
| `ProviderBotBinding.connType` | 当前无对应字段 | 目标模型新增；不能再以“存在 Binding”等同于 Gateway 接入 |

Organization、Friendship 等关系暂不纳入本节，后续作为独立支撑领域讨论。

## 7. 当前实现基础

| 当前能力 | 与目标模型的关系 |
|---|---|
| Bot Registry / Actor Directory | Actor 注册、描述和发现的基础 |
| Group 与协作策略 | BCN 多 Actor 协作的基础 |
| Session、消息和异步 Run | 持续上下文和 A2A 交互模型的基础 |
| Provider / ProviderBotBinding | Bot 提供方关联与 Plugin/Gateway 投递选择的基础 |

当前实现已具备 Actor Directory、Session 和异步 Run 等基础，但仍需要统一 Message Part、Artifact 标准视图，并补齐 AgentCard、A2A 和 ARD 标准接口。

## 8. 当前结论与后续讨论

当前已确认：

- BCN 核心对象为 Actor、Group、Session。
- Group 保留为核心对象，不新增 Collaboration 聚合。
- Group 使用核心字段平铺、可选能力嵌套的结构；第一版不需要新增存储表。
- `createdBy` 记录实际创建人工号，`originator` 表达发起和协调主体，`driver` 表达主执行 Bot。
- Group 保留 `version`，但当前版本演进尚未启用。
- Session 是 Group 中一次相互隔离的协作上下文，分为 `chat` 和 `service_invocation`。
- ServiceInvocation Session 使用可选 Invocation 值对象组织调用字段；数据库继续使用现有平铺字段。
- 第一版不新增 BCN Task 领域对象；ServiceInvocation Session 通过标准视图兼容 A2A Task。
- Service Invocation 默认每次创建新 Session，目标 API 不暴露 Reactivate。
- Actor 类型为 HumanActor 和 BotActor。
- HumanActor 第一版没有额外字段；BotActor 具有 BotDescriptor、agentCode、createdBy 和 reachability。
- BotDescriptor 描述能力与 Provider，不包含凭证和内部路由配置。
- HumanActor 与 BotActor 使用 `creates/createdBy` 表达不可变创建来源，不使用 Ownership 命名。
- 每个 BotActor 目标上都关联一个 Provider；BCN 原生 Bot 使用默认 BCN Provider。
- 同一 Provider 可以支持 Plugin 和 Gateway 两种接入方式，实际 `connType` 由 ProviderBotBinding 选择。
- 第一版不引入 ActorIdentityBinding。
- AgentCard 和 ARD CatalogEntry 是标准视图，不是独立领域对象。

后续需要逐个讨论：

- BotDescriptor 与 AgentCard 的对应关系；
- Provider 完整模型与 ProviderBotBinding 的生命周期；
- Friendship 与 FriendRequest；
- Group 版本演进和协作策略的进一步约束；
- Message、Part、Artifact 的结构与 A2A 映射；
- Session `createdBy` 在 Human、Bot 和外部服务调用下的统一语义；
- A2A 和 ARD 协议适配接口。

## 9. 参考资料

- [A2A Specification](https://a2a-protocol.org/latest/specification/)
- [Agentic Resource Discovery](https://agenticresourcediscovery.org/)
- `src/bcs-internal/docs/api`
- `src/bcs-internal/docs/bcs-interface-catalog.md`
- `ocb-public/src/bcs`
