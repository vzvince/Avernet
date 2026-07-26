# BCS 接口目录与打标

更新时间：2026-07-07

本文档按新的业务目录结构整理 BCS 当前接口，用于 OpenAPI 规划和内部接口治理。详细鉴权、参数和返回格式仍以 `bcs-interface-classification.md` 为准。

打标说明：

| 打标 | 含义 |
|------|------|
| `openapi` | 适合作为稳定 OpenAPI 对外暴露，或建议进入 OpenAPI 设计范围 |
| `internalapi` | BCS/BCN/Workbench/Provider/Bot Runtime 内部接口，不建议作为外部 OpenAPI |
| `deprecated` | 已无实际用途、已有替代方案、或已明确待下线/待删除 |

路径作用域与前缀：

本文档表格中的接口路径默认使用 BCS 服务当前实现的原始路径，例如 `/bots`、`/groups/{id}`。生成 BCN 对外 OpenAPI 或内部网关路由时，按接口打标套用下面的 canonical 前缀。

| 作用域 | Canonical 前缀 | 适用接口 | 示例 | 说明 |
|--------|----------------|----------|------|------|
| BCN OpenAPI REST | `/openapi/bcn/v1` | 打标为 `openapi` 的稳定 REST 能力 | `GET /bots` -> `GET /openapi/bcn/v1/bots` | 面向外部/伙伴/三方接入的稳定 BCN API；不使用 `/bcnproxy`、`/api/v1/engine` 或 BCS 裸路径作为 OpenAPI 路径 |
| BCN OpenAPI WebSocket | `/openapi/bcn/v1/ws` | 需要纳入 OpenAPI/协议文档的前端实时通道 | `GET /ws` -> `WS /openapi/bcn/v1/ws` | WS 协议不使用 REST 响应 envelope；仅记录连接地址、鉴权和帧协议 |
| BCN Internal REST | `/api/bcn/v1` | 打标为 `internalapi` 的内部 HTTP 能力 | `POST /providers` -> `POST /api/bcn/v1/providers` | 面向 Workbench、Provider、Bot Runtime、Backend Gateway 或内部运维调用，不进入外部 OpenAPI |
| BCN Internal WebSocket | `/api/bcn/v1/ws/*` | Bot Runtime 或内部实时通道 | `GET /ws/bot` -> `WS /api/bcn/v1/ws/bot` | Bot Runtime 协议、Provider/内部控制通道等只作为内部协议维护 |
| Deprecated/兼容路径 | 保留原路径或历史代理路径 | 打标为 `deprecated` 的接口 | `GET /bots/paged` | 不分配新的 OpenAPI canonical 路径；如需保留兼容，应带弃用说明和下线计划 |

历史/兼容代理路径说明：

- Workbench 前端同源代理 `/bcnproxy/...`、Backend Gateway 透明转发 `/api/v1/engine/...`、`/api/v1/admin/...`、以及 BCS 服务裸路径 `/bots`、`/groups` 等，均视为内部或兼容入口。
- 新的外部 OpenAPI 文档只使用 `/openapi/bcn/v1/...`；新的内部 API 文档只使用 `/api/bcn/v1/...`。
- 部署探活类路径如 `/health`、`/metrics` 可继续作为基础设施路径存在，但不作为外部 OpenAPI surface。

身份模型说明：

| 身份模型 | 凭证/来源 | 当前校验逻辑 |
|----------|-----------|--------------|
| `public` | 无凭证 | 路由层不要求身份，最多由 service 根据资源可见性或参数做业务校验 |
| `human(cookie)` | `IAM_TOKEN` cookie；OAuth 模式下也可能是 `bcs_session` cookie；本地调试可用 mock staff | 通过 auth chain 提取 `user_id/user_name`，映射为 `staff_no/nick_name`，业务层常转成 `human_{staff_no}` actor 并校验所有权/参与关系 |
| `agent token` | `X-BCS-Bot-Token` 或非 JWT `Authorization: Bearer <token>` | Session token 插件查 Bot Registry，解析为 `bot_uuid`；部分接口继续校验容器头 `x-agentclaw-bolt-id` 是否与调用 Bot 匹配 |
| `AgentPass token` | JWT 形式 `Authorization: Bearer <jwt>` | AgentPass 插件解析 tcauthmng/ACM 身份并映射到 `bot_uuid` 或 provider bot binding；JWT Bearer 不会走普通 session token 逻辑 |
| `agent static token` | `Authorization: Bearer <static_agent_token>` | Provider Bot/agent 配置的静态 Bearer 凭证；用于 Provider 回调等场景，按 provider id、provider bot ref 和 token 绑定关系校验 |
| `provider admin token` | `Authorization: Bearer <provider_admin_token>` | Provider service 校验 token 与 `provider_id`、启停状态、允许操作范围 |
| `service key` | `X-BCS-Service-Key` | 对 key 做 SHA-256 后查 service key registry，校验 `bound_groups`；registry 为空时当前实现接受任意非空 key 并派生 `svc-key:*` principal |
| `signed token` | URL path/query 中的 proposal/register/invite token | 校验签名、过期时间和 token 内资源绑定；是否还需要 cookie 取决于具体接口 |
| `loopback` | 请求 peer IP | 仅允许本机 loopback 地址访问 |

通用 auth chain 当前默认顺序：debug 默认 `local`；release 默认 `agentpass -> cookie -> session`，配置项 `[auth].chain` 会完全覆盖默认链。链路中第一个成功解析的插件胜出；插件返回错误会中断；全部返回空则视为匿名。

## 1. 系统/运维

| 接口 | 接口描述 | 功能作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------|----------|
| `GET /health` | 健康检查 | 供部署、探活和本地调试确认服务存活 | `public` | `internalapi` |
| `GET /metrics` | Prometheus 指标，按配置启用 | 供监控系统采集运行指标 | `public`/内部网络 | `internalapi` |
| `GET /manifest` | 前端 bundle manifest | 供前端静态资源加载和版本定位 | `public` | `internalapi` |
| `GET /admin/secret/{name}` | 拉取服务密钥 | 供 loopback/admin 内部流程读取密钥 | `loopback` | `internalapi` |

## 2. 身份与接入

| 接口 | 接口描述 | 功能作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------|----------|
| `GET /me` | 当前用户身份 | 获取 Workbench 当前登录用户上下文 | `human(cookie)` 可选 | `internalapi` |
| `GET /me/repair-info` | 身份修复信息 | 辅助排查或修复人类身份与 Bot 绑定关系 | `human(cookie)` 可选 | `internalapi` |
| `POST /me/ensure-human` | 确保人类 actor 存在 | 为当前登录人创建或补齐 `human_{staff_no}` actor | `human(cookie)` | `internalapi` |
| `GET /onboard/url` | 生成 onboard URL | 旧 Bot 入网/注册入口 URL，待下线 | `public` | `deprecated` |
| `GET /register/token` | 获取人工注册 token | 支持人工注册 Bot 的临时凭证流程 | `human(cookie)` | `internalapi` |
| `POST /register` | 人工注册 Bot | 通过注册 token 完成人工 Bot 注册 | `signed token` | `internalapi` |

## 3. 实时通信通道

### 3.1 前端 WS / Workbench

| 接口 | 接口描述 | 功能作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------|----------|
| `GET /ws` | 前端客户端 WebSocket 入口 | 建立 Workbench 与 BCS 的实时连接 | `public` upgrade | `openapi` |
| `WS /ws#connect` | 连接并订阅 group/session | 让前端进入指定 group 或 session 的消息上下文 | Workbench 连接上下文 | `openapi` |
| `WS /ws#chat.send` | 前端发送消息 | 从 Workbench 向 group/session 投递用户消息 | 已连接 Workbench actor | `openapi` |
| `WS /ws#chat.abort` | 取消前端 chat run | 取消前端发起的正在运行消息流程 | 已连接 Workbench actor | `openapi` |

### 3.2 Bot Runtime 通信

| 接口 | 接口描述 | 功能作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------|----------|
| `GET /ws/bot` | Bot WebSocket 入口 | 建立 Bot Runtime 到 BCS 的长连接 | `agent token` 可选 | `internalapi` |
| `WS /ws/bot#bot.connect` | Bot 连接握手 | 注册或恢复 Bot runtime 连接身份 | `agent token` 可选 | `internalapi` |
| `WS /ws/bot#bot.status` | Bot 状态心跳 | 上报 Bot runtime 在线状态 | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#task.dispatch` | 派发任务 | Manager Bot 向 Worker Bot 派发任务 | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#task.message` | 任务消息回传 | Worker Bot 回传任务过程消息 | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#route.resolve` | 路由目标解析 | Bot 侧请求 BCS 解析协作路由 | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#task.complete` | 任务完成 | Worker Bot 上报任务完成状态 | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#session.complete` | Session 完成 | Bot 侧通知 BCS 完成 session | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#chat.send` | Bot 侧 chat 发送，当前未实现 | 保留帧方法，当前不作为有效能力使用 | 已注册 agent 连接 | `internalapi` |
| `WS /ws/bot#chat.abort` | Bot 侧 chat 取消，当前未实现 | 保留帧方法，当前不作为有效能力使用 | 已注册 agent 连接 | `internalapi` |

## 4. Bot 管理

### 4.1 Bot 目录查询

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /bots` | 列出 Bot | 查询 Bot 目录，建议作为 canonical 列表接口并统一分页返回 | 前端 GroupChat 通过 `/bcnproxy/bots?onboarded=true` 拉取已入网 Bot，用于创建群/加成员候选与 bot_uuid 到名称缓存；`bcs-cli list` 也用于查看网络内 Bot；BCS 集成测试覆盖列表能力。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `GET /bots/my` | 我的 Bot 列表 | 查询当前人拥有或关联的 Bot | 前端 GroupChat 通过 `/bcnproxy/bots/my` 获取当前人在 BCN 网络中已注册的 Bot/Human Actor，并与前端已有用户 Bot 列表合并，用于顶部 Bot Tabs、driver bot 选择、在线状态展示和入网状态判断；BCS 测试覆盖返回字段。 | `human(cookie)` | `openapi` |
| `GET /bots/paged` | 分页列出 Bot | 历史分页列表接口；建议并入 canonical `GET /bots` 并统一返回格式 | 未发现前端或 CLI 产品调用；当前主要存在于 BCS route/service 契约测试中，用于保护旧分页返回结构。建议继续标记 deprecated，并迁移到 `GET /bots`。 | `public` | `deprecated` |
| `POST /bots/query` | 批量查询 Bot | 按 Bot UUID 批量获取目录信息 | 前端 GroupChat 用于批量补查 Human Actor、`active_only` 模式下未命中的本地 Bot，以及为好友请求补齐 Bot 名称；bcs-cli client 有封装但未暴露独立命令；BCS 测试覆盖批量状态字段。 | `public` | `openapi` |
| `GET /bots/{id}` | 获取 Bot 详情 | 查询单个 Bot 的能力、状态和元数据；`id` 实际为 bot_uuid | 前端 GroupChat 通过 `/bcnproxy/bots/{bot_uuid}` 在群组流程中检查 Bot 是否已入网、是否 hidden 以及 visibility；`bcs-cli get` 用于查看单个 Bot 详情；BCS 测试覆盖详情字段和 ownership 行为。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `DELETE /bots/{id}` | Bot 退出网络 | 让目标 Bot 从 BCS 目录中退出或注销 | 未发现前端或 CLI 直接调用；BCS 测试验证 human owner 可让 Bot 退出网络。注意前端删除 Bot 走 backend `/api/bots/{bot_id}`，backend 当前同步的是 Provider Bot 删除 `/providers/{provider_id}/bots/{provider_bot_ref}`，不是该接口。 | `human(cookie)` | `openapi` |
| `GET /bots/{id}/friends` | Bot 好友列表 | 查询目标 Bot 的好友关系 | 前端 GroupChat 的好友列表、创建群、群设置加成员、Session 加成员面板都会通过 `/bcnproxy/bots/{bot_uuid}/friends` 获取可协作好友候选；`bcs-cli friends` 也使用；BCS 友好关系/可见性测试覆盖。 | `agent token`/`AgentPass token` 或 `human(cookie)` owner | `openapi` |
| `GET /bots/{id}/groups` | Bot 所属群组 | 查询目标 Bot/actor 参与的群组 | 前端 GroupChat 通过 `/bcnproxy/bots/{bot_uuid}/groups` 加载当前 driver bot 的群列表，支持分页、搜索以及 normal/dm/all 过滤；BCS 回归测试覆盖 group_kind、label、absent Human 过滤等行为。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `GET /bots/{id}/visibility` | 查询 Bot 可见性 | 获取 Bot 对协作发现的可见性策略 | 前端仅有 wrapper，未发现产品代码实际调用；`bcs-cli visibility get` 与 BCS 可见性测试在用。由于 Bot 列表/详情已携带 visibility，后续并入 `GET /bots/{id}`。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |
| `PUT /bots/{id}/visibility` | 设置 Bot 可见性 | 更新 Bot 的 public/protected/private 可见性策略 | 前端 GroupChat 的 BotTab/TopNavBar 可见性切换、入网后默认 protected 设置，以及“允许被添加为好友/是否需要确认”开关都会调用；`bcs-cli visibility set` 也使用；BCS 测试覆盖 public/protected/private 与权限规则。 | `human(cookie)` owner 或 `agent token`/`AgentPass token` self | `openapi` |

Bot 目录查询返回格式治理：

- 当前实现中，`GET /bots` 返回裸数组，`GET /bots/my` 和 `GET /bots/paged` 返回裸分页对象；item 内部仍保留历史 `capabilities` 包装，`skills` 在部分接口中仍会降级为字符串数组，且 `dynamic_status.status` 同时混合了 actor 是否 hidden 与 runtime 是否可达。
- OpenAPI 目标态统一使用 envelope + 分页结构，并把 item 定义为扁平 `BotInfo`：`bot_uuid/name/summary/domains/skills/scopes/visibility/status/connection_status/created_by/actor_kind/env`。其中 `skills` 固定为 `{name, description}` 结构，不暴露历史单字符串格式。
- `status` 保留 Bot/Actor 展示态语义，仅表达 `online` / `hidden`；`connection_status` 表达 Bot Runtime 与 BCN 的连接/投递可达状态，取值为 `connected` / `disconnected`，不受 hidden 影响。当前代码仍需从现有 `dynamic_status.status` 迁移到该目标输出形态。
- `GET /bots/{id}/groups` 的 OpenAPI 目标态使用专用 `BotGroupListItem`，不复用通用 `Group`；`participants` 使用专用 `GroupParticipant`，保留 `mode` 表达群内参与模式（Bot: `auto`/`muted`，Human: `present`/`absent`），不输出 participant `status`，避免和 Bot/Actor 展示态、连接态混淆。

### 4.2 Bot 内部生命周期/消息

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /bots/connect` | Bot 连接握手兼容接口 | 旧 HTTP 连接模型兼容入口；WebSocket 连接模型下不需要 | `bcs-cli connect` 仍可用它做 HTTP 握手、获取或复用 bot token；BCS route/回归测试覆盖该兼容行为。未发现前端或 backend 产品路径调用；正式 Bot Runtime 连接应走 WebSocket `/ws/bot` 的 `bot.connect`。 | `public` + 可选 caller | `deprecated` |
| `GET /bots/discover` | Bot 发现接口 | 内部按能力发现可协作 Bot | `bcs-cli discover` 和 BCS discover/visibility 测试仍使用；frontend 仅保留 `BcnController.discoverBots` 包装和历史设计文档，当前 GroupChat 发现/推荐主流程已改走 Actor API 的 `loadActors/searchActors`。未发现 backend 产品代码直接调用；backend gateway contract 中保留透明转发能力。 | `public` | `internalapi` |
| `POST /bots/onboard` | Bot 注册能力 | 旧 Bot Runtime 自助入网入口，待下线 | `bcs-cli onboard` 的 direct API mode 使用；前端 `BcnRegister` 页面通过 `/bcnproxy/bots/onboard` 做 token 表单入网；BCS onboarding/e2e 测试覆盖。GroupChat 常规入网按钮不走该接口，而是走 admin onboard。 | `agent token`/`AgentPass token`，可选 `human(cookie)` | `deprecated` |
| `POST /admin/bots/onboard` | Admin/前端代理 Bot 入网 | 管理侧创建或更新 Bot 入网信息 | backend `BotService` 在 Bot 创建/更新后通过 `BcnService` 同步到 BCN；前端 GroupChat 的入网/移出协作网络入口通过 `/bcnproxy/admin/bots/onboard` 调用；backend gateway `/api/v1/admin/bots/onboard` contract 保留透明转发。 | `human(cookie)` 可选 | `internalapi` |
| `POST /bots/status` | Bot 动态状态更新 | 更新 dynamic status；当前基本无实际业务用途 | `bcs-cli update-status` 和 BCS ownership/route 测试仍调用；未发现前端或 backend 产品路径调用。dynamic status 当前不承载关键业务判断，后续删除主要影响 CLI 调试能力和相关测试。 | `agent token`/`AgentPass token` | `deprecated` |
| `POST /bots/{id}/chat` | 单 Bot 同步消息 | Legacy blocking 1:1 Bot chat，建议并入 async 调用后删除 | BCS contract/e2e/可达性测试和 `bcs-cli` client 老方法仍覆盖；当前 `bcs-cli chat` 已切到 `/bots/{id}/chat-async` + run 查询，未发现前端或 backend 产品路径调用。建议作为 legacy blocking shim 保留到测试/旧 client 迁移完成。 | `agent token`/`AgentPass token`，可选 `human(cookie)` | `deprecated` |

### 4.3 单 Bot 异步调用 / Bot A2A

| 接口 | 接口描述 | 功能作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------|----------|
| `POST /bots/{id}/chat-async` | 提交单 Bot 异步调用 | 创建单 Bot invocation run，返回 run 信息 | `agent token`/`AgentPass token` | `internalapi` |
| `GET /chat/runs/{run_id}` | 查询异步调用结果 | 获取 run 状态、结果，支持长轮询 | `agent token`/`AgentPass token` | `internalapi` |
| `POST /chat/runs/{run_id}/cancel` | 取消异步调用 | 取消正在运行的单 Bot invocation run | `agent token`/`AgentPass token` | `internalapi` |

### 4.4 好友关系

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /friends/request` | 发送好友请求 | Bot 之间建立协作好友关系的申请入口 | 前端 GroupChat 的添加好友弹窗通过 `useFriends.sendFriendRequest` 调用 `/bcnproxy/friends/request`，用于从推荐好友列表向目标 Bot 发起好友申请；`bcs-cli friend request` 也使用。backend 侧未发现产品服务直接调用，主要保留 `/api/v1/engine/friends/request` 透明转发契约测试；BCS e2e/route/可见性测试覆盖 public 自动通过、protected 待确认、private 拒绝等行为。 | `agent token`/`AgentPass token` 或 `human(cookie)` owner | `openapi` |
| `GET /friends/requests` | 好友请求列表 | 查询待处理或历史好友请求 | 前端 GroupChat 在 driver bot 切换时加载收到的好友请求，用于左侧未读红点；好友弹窗/好友申请 Tab 也通过 `useFriends.loadRequests` 拉取 pending 请求并按收到/发出分组展示；`bcs-cli friend requests` 也使用。backend 侧主要是透明转发契约测试，BCS e2e/route 测试覆盖 direction/status 过滤。 | `agent token`/`AgentPass token` 或 `human(cookie)` owner | `openapi` |
| `POST /friends/requests/{id}/accept` | 接受好友请求 | 确认好友关系并建立协作权限 | 前端 GroupChat 好友申请 Tab 通过 `useFriends.acceptRequest` 接受 pending 请求，成功后更新请求状态并刷新好友列表；`bcs-cli friend accept` 也使用。backend 侧主要是透明转发契约测试，BCS e2e/route/可见性测试覆盖 receiver 鉴权、建立双向好友关系以及反向 pending 请求联动。 | `agent token`/`AgentPass token` 或 `human(cookie)` owner | `openapi` |
| `POST /friends/requests/{id}/reject` | 拒绝好友请求 | 拒绝待处理好友申请 | 前端 GroupChat 好友申请 Tab 通过 `useFriends.rejectRequest` 拒绝 pending 请求，成功后更新请求状态并扣减未读数；`bcs-cli friend reject` 也使用。backend 侧主要是透明转发契约测试，BCS e2e/route 测试覆盖 receiver 鉴权、拒绝后不建立好友关系。 | `agent token`/`AgentPass token` 或 `human(cookie)` owner | `openapi` |

### 4.5 Actor 目录

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /actors/list` | 列出 actors | 查询 Bot/Human actor 目录 | 前端通过 `ActorController.getActorList` 调用 `/bcnproxy/actors/list`，是 GroupChat 当前替代旧 `/bots/discover` 的普通列表入口：添加好友推荐列表使用 `cooperatable_only=false`，创建群、群设置加成员、Session 加成员和名称搜索使用 `cooperatable_only=true` 查询可协作 Bot；支持分页和 name 过滤。未发现 bcs-cli 或 backend 产品服务直接调用；BCS route/integration 测试覆盖分页、可协作过滤、status 和 tags 返回。 | `public` | `openapi` |
| `GET /actors/search` | 搜索 actors | 按关键词搜索可协作 actor | 前端通过 `ActorController.searchActors` 调用 `/bcnproxy/actors/search`，用于 GroupChat 创建群智能搜索、智能推荐、带关键词的可协作 Bot 搜索，以及相关推荐埋点 trace_id；搜索结果由语义推荐和兜底匹配合并。未发现 bcs-cli 或 backend 产品服务直接调用；BCS route/integration 测试覆盖 q 查询、status、tags、去重和降级场景。 | `public` | `openapi` |
| `PUT /actors/{aid}/status` | 更新 actor 状态 | 更新 actor 的在线/隐藏状态 | 前端 GroupChat 的 BotInfoCard/GroupListPanel 协作状态开关通过 `BcnController.updateActorStatus` 调用 `/bcnproxy/actors/{actor_id}/status`，用于当前 driver bot 在 `online` 与 `hidden` 之间切换，并同步更新本地 Bot Store 的在线/暂停协作展示。未发现 bcs-cli 或 backend 产品服务直接调用；BCS route/integration 测试覆盖 caller 鉴权、hidden 状态对列表/搜索/详情的影响。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |

## 5. Provider 管理

### 5.1 Provider 管理

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /providers/{provider_id}` | 获取 provider | 查询 Provider 配置和集成元数据 | Provider 下行调试控制台 `provider_downlink_console.py` 用于查看已注册 Provider 的 name、webhook、auth mode、coordination 等元数据；`test_provider_e2e.sh` 在注册 Provider 后调用它校验不会泄露 `provider_admin_token` / `bcs_to_provider_token`；BCS route contract 覆盖 token 校验和响应字段。未发现前端或 backend 产品服务直接调用。 | `provider admin token` | `openapi` |
| `PATCH /providers/{provider_id}` | 更新 provider | 更新 Provider 配置、鉴权或协作模式 | Provider 下行调试控制台用于调整 Provider name/webhook；`test_provider_e2e.sh` 用它更新 name/webhook_url 并验证 auth.mode 不能被 PATCH 修改；BCS route contract 覆盖需要 provider admin token + 人身份、owner 校验、coordination 配置更新。未发现前端或 backend 产品服务直接调用。 | `provider admin token` + `human(cookie)` | `openapi` |
| `GET /providers/{provider_id}/bots` | Provider Bot 列表 | 查询某个 Provider 下注册的 Bot | Provider 下行调试控制台和 passive mock provider 用于查看 Provider 下已注册 Bot 绑定；`test_provider_e2e.sh` 在注册 Provider Bot 后调用它校验列表不泄露 `bot_runtime_token`；BCS route contract 覆盖 provider admin token 校验和绑定列表返回。未发现前端或 backend 产品服务直接调用。 | `provider admin token` | `openapi` |
| `POST /providers/{provider_id}/bots` | 注册 Provider Bot | 将 Provider 侧 Bot 映射注册到 BCS | backend `BcnService.register_provider_bot` 在 TeamClaw Bot 创建/启动时被 `BotService` 触发，用于把 `claude_code normalCC`、`teclaw`、`openclaw service` 等 Bot 注册为 Provider Bot；调用受 DRM 开关和 provider 凭据配置控制，失败不阻塞主流程。Provider 下行调试控制台、passive mock provider、`test_provider_e2e.sh` 也用它注册测试 Bot；BCS contract/integration 测试覆盖幂等、owners、skills/domains/scopes、runtime token 返回和跨 Provider 冲突。 | `provider admin token` | `openapi` |
| `DELETE /providers/{provider_id}/bots/{provider_bot_ref}` | 删除 Provider Bot | 删除 Provider Bot 与 BCS Bot 的映射 | backend `BotService.delete_bot` 在本地 Bot 逻辑删除成功后 best-effort 调用 `BcnService.delete_provider_bot`，同步删除 BCN Provider Bot 绑定；`delete_provider_bots.py` 用它做批量清理和验证 SQL 输出；BCS route contract 覆盖软删除绑定 Bot、删除 runtime token、目标不存在时幂等返回。未发现前端或 bcs-cli 产品命令直接调用。 | `provider admin token` | `openapi` |

### 5.2 Provider 内部/灰度

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------------------|----------|----------|
| `POST /providers` | 注册 provider | 内部初始化 Provider 并返回 admin/runtime token | Provider 下行调试控制台 `provider_downlink_console.py` 用于初始化 Provider 并保存 `provider_admin_token` / `bcs_to_provider_token`；`test_provider_e2e.sh` 和 BCS route contract 覆盖注册、鉴权模式和 coordination 元数据。未发现前端或 backend 产品服务直接调用。 | `human(cookie)` | `internalapi` |
| `POST /providers/agentpass/resolve` | 解析 AgentPass Bot | Provider 兼容解析接口，已不建议继续扩展 | 目前主要由 BCS `bot_events_contract.rs` 覆盖 AgentPass token 到 `agent_code`、Provider Bot binding、Bot 信息的解析行为；未发现前端、backend 或脚本直接调用。实际回调链路更依赖 `/bot/events` 内部解析 AgentPass token。 | `AgentPass token` + provider header | `deprecated` |
| `GET /providers/stream-gray` | 查询 stream 灰度名单 | 查看 Provider stream 灰度配置 | 作为 Provider stream 灰度开关的运维查询接口保留，相关设计文档和 route contract 覆盖 `enabled` / `created_by` 返回及 `X-BCS-Service-Key` 场景；未发现前端、backend 或脚本直接调用。 | `public` | `deprecated` |
| `PUT /providers/stream-gray` | 更新 stream 灰度名单 | 修改 Provider stream 灰度配置 | 作为 Provider stream 灰度开关的运维更新接口保留，route contract 覆盖启停灰度、成员列表归一化、保留旧名单等行为；未发现前端、backend 或脚本直接调用。 | `public` | `deprecated` |
| `POST /providers/{provider_id}/disable` | 停用 provider | 内部管理侧停用 Provider | Provider 下行调试控制台 `provider disable` 调用，用于临时停用某个 Provider；BCS route contract 覆盖 provider owner 身份校验。未发现前端或 backend 产品服务直接调用。 | `provider admin token` + `human(cookie)` | `internalapi` |
| `POST /providers/{provider_id}/enable` | 启用 provider | 内部管理侧启用 Provider | Provider 下行调试控制台 `provider enable` 调用，用于恢复已停用 Provider；BCS route contract 覆盖 provider owner 身份校验。未发现前端或 backend 产品服务直接调用。 | `provider admin token` + `human(cookie)` | `internalapi` |
| `POST /providers/{provider_id}/delivery/switch-bot` | 切换 Bot 投递方式 | 内部切换 Provider Bot 的消息投递通道 | backend `BcnService.switch_bot` 用于把 TeamClaw Bot 的 BCN 绑定切到 Provider delivery bot，`switch_provider_bots.py` 也可批量切换个人 Bot；BCS `bot_delivery_switch_contract.rs` 覆盖请求契约、幂等和 token 返回。 | `provider admin token` | `internalapi` |

### 5.3 Provider 回调

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------------------|----------|----------|
| `POST /bot/events` | Provider 上报 Bot 事件 | 接收 Provider 侧 Bot runtime 事件回调 | Provider runtime 在收到 BCS 下发的 `chat.send` 后异步回调该接口回传 delta/final/error 等结果；mock provider、passive provider、bridge、downlink console 和 `test_provider_downlink.sh` 都使用它验证 static bearer、AgentPass、provider_admin 三种回调鉴权和 run-context 收敛。 | `provider admin token` 或 `AgentPass token` 或 `agent static token` | `openapi` |
| `POST /bot/events/coordination` | Provider 上报协作事件 | 旧 Provider 协作/工具调用事件回调，待下线 | 用于 Provider 回传协作/工具调用事件，例如 `mcporter_mcp` tool result 或 native MCP coordination intent；当前主要由 BCS `bot_events_contract.rs` 覆盖匹配 run context、去重、模式校验和派发任务。未发现前端或 backend 产品服务直接调用。 | `provider admin token` 或 `AgentPass token` 或 `agent static token` | `deprecated` |

## 6. 群组协作

### 6.1 群组管理

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /groups` | 列出群组 | 查询当前可见或相关群组 | 前端 ExpertMarket 群广场通过 `getPublicGroups` 查询 `visibility=public` 的协作群，支持搜索、分页加载和群卡片展示；BCS CLI `list-groups` 和 route/contract tests 也覆盖该接口。GroupChat “我的群”主路径主要走 `GET /bots/{id}/groups`。 | `public` | `openapi` |
| `POST /groups` | 创建群组 | 创建多 Bot 协作群组 | 前端 GroupChat `CreateGroupModal` 和 `useGroups.createGroup` 用于创建普通群、manager-worker 群和 state_machine 群，state_machine 场景会携带 `collaboration_definition_yaml`；BCS CLI `create-group`、provider e2e/workbench 脚本和 BCS contract tests 也使用。 | `public` + 可选 `human(cookie)`/`agent token`/`AgentPass token` | `openapi` |
| `GET /groups/{id}` | 群组详情 | 获取群组配置、成员和状态 | 前端 GroupChat 进入群详情时通过 `loadGroupDetail` 拉取群配置、成员和可见性；ExpertMarket `GroupMembersDialog` 用于群卡片成员详情；BCS CLI `get-group`、backend gateway 透明转发测试和 BCS contract tests 也使用。 | `public` | `openapi` |
| `DELETE /groups/{id}` | 删除群组 | 删除或退出指定群组 | 前端 GroupChat `useGroups.deleteGroup` 用于删除或退出协作群，并通过 `bot_id` 表达当前操作者；backend gateway contract 和 BCS groups contract 覆盖该路径。 | query `bot_id` | `openapi` |
| `POST /groups/{id}/members` | 添加群组成员 | 向群组添加 Bot/Human participant | 前端 GroupChat `useGroupMembers.addGroupMembersBatch` 在群设置/成员管理中批量添加 Bot 成员，并用接口返回的 role 回写本地 store；BCS CLI `add-member` 和 route tests 覆盖默认角色、权限和成员写入。 | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | `openapi` |
| `DELETE /groups/{id}/members/{bot_uuid}` | 移除群组成员 | 从群组移除指定成员 | 前端 GroupChat `useGroupMembers.removeGroupMember` 在群设置/成员管理中移除成员，并同步更新本地 store；BCS route tests 覆盖成员移除和权限边界。 | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` | `openapi` |
| `PUT /groups/{id}/visibility` | 更新群组可见性 | 调整群组发现和访问策略 | 前端 GroupChat `GroupVisibilitySection` 用于 owner 将群公开到协作广场或取消公开，公开前会处理“成员 Bot 不满足公开条件”的错误；该接口直接影响 ExpertMarket 群广场是否能发现群。 | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | `openapi` |
| `PATCH /groups/{id}/settings` | 更新群设置 | 修改群组标题、描述等设置 | 当前未发现前端、backend proxy 或 CLI 产品调用；BCS groups contract 覆盖 service spec patch。更像预留/内部配置接口，因此不进入外部 OpenAPI。 | `public` | `internalapi` |
| `PUT /groups/{gid}/participants/{aid}/mode` | 设置参与者模式 | 旧 Bot/Human participant 模式更新入口，待下线 | 前端 GroupChat 用于加入/离开协作以及 Bot auto/muted 模式切换；backend gateway contract 和 BCS contract tests 覆盖透明转发、参与者模式校验和状态回写。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |

### 6.2 群组提案/确认页（deprecated，待删除）

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /groups/request` | 发起群组提案 | 旧群组提案创建入口，待下线 | BCS coordination skill 和 bcs-cli `request-group-help` 使用，用于 Bot 根据 gap/description 生成拉群提案和确认 URL；当前未发现前端产品调用，BCS `group_request_contract.rs` 覆盖 legacy JSON 兼容和 proposal 创建。 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `GET /groups/{token}/confirm` | 提案确认页 | 旧群组提案确认页，待下线 | 用户打开 proposal 中的确认链接时使用，返回 HTML 确认页；BCS contract tests 覆盖页面渲染。前端 Workbench 未直接调用。 | `signed token` | `deprecated` |
| `POST /groups/{token}/confirm` | 确认并创建群组 | 旧群组提案确认入口，待下线 | 确认页提交或 bcs-cli `confirm-group-help` 使用，用于把 proposal 落成真实群组；BCS scripts/test.sh 和 `group_request_contract.rs` 覆盖确认建群流程。 | `signed token` | `deprecated` |

### 6.3 群组内部控制

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /groups/{id}/collaboration-definition` | 获取协作定义 | 查看群组内部协作定义 | 当前未发现前端、backend proxy 或 CLI 产品调用；主要作为 state_machine 群的内部调试/运维读取接口，BCS collaboration runtime tests 覆盖读取已持久化的源 YAML 和定义结构。 | `public` | `internalapi` |
| `PATCH /groups/{id}/collaboration-definition` | 更新协作定义 | 修改群组内部协作定义 | 当前未发现前端、backend proxy 或 CLI 产品调用；用于内部修正 state_machine 群的协作定义 YAML/绑定关系，BCS runtime tests 覆盖更新后保留 source YAML 和版本信息。 | `public` | `internalapi` |
| `POST /groups/{id}/collaboration-definition/upgrade` | 升级协作定义 | 将群组协作定义升级到新结构 | 当前未发现前端、backend proxy 或 CLI 产品调用；用于内部迁移/升级旧协作定义到新结构，route 层委托 `upgrade_group_collaboration_definition`。 | `public` | `internalapi` |
| `PUT /groups/{id}/routing-policy` | 更新路由策略 | 旧群组消息路由策略更新入口，待下线 | 结构化路由测试脚本 `test_sender_routes.py` 使用，用于写入 `sender_routes`、默认投递策略等路由配置；BCS route/groups contracts 覆盖该内部控制面。未发现前端产品调用。 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `PUT /groups/{id}/status` | 更新群状态 | 内部更新群组生命周期状态 | deprecated；仅发现 bcs-cli `group-status` 和 BCS route/groups contracts 使用，且 bcs-cli 不需要继续保留该能力。未发现前端或 backend 产品调用。 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `POST /groups/{id}/terminate` | 终止群组 | 内部终止群组协作流程 | deprecated；仅发现 bcs-cli `terminate-group`、BCS route/groups contracts 和少量脚本使用，且 bcs-cli 不需要继续保留该能力。未发现前端或 backend 产品调用。 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `PUT /groups/{id}/label` | 更新群标签 | 内部维护群组标签 | deprecated；未发现前端、backend 或 CLI 产品调用，仅 BCS e2e group 脚本和 route/groups contracts 使用，用于内部打标或测试群标签写入。 | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | `deprecated` |
| `GET /groups/{id}/workspace` | 获取 workspace | 旧 workspace 能力，已计划删除 | deprecated 旧 workspace 查询能力；当前未发现前端产品调用，主要由 BCS contract tests 覆盖。后续应由 session/collaboration state 相关接口替代。 | `public` | `deprecated` |
| `PUT /groups/{id}/workspace` | 更新 workspace | 旧 workspace 能力，已计划删除 | deprecated 旧 workspace 写入能力；当前未发现前端产品调用，主要由 BCS route/groups contracts 覆盖。后续应由 session/collaboration state 相关接口替代。 | `public` | `deprecated` |

### 6.4 协作模板

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /collaboration/templates` | 列出协作模板 | 获取可用于建群/协作编排的模板列表 | 前端 GroupChat `CreateGroupModal` 在 state_machine/自定义协作建群时拉取模板列表，按 priority 排序并自动选择默认模板；BCS template service tests 覆盖语言、优先级和可用性。 | `public` | `openapi` |
| `GET /collaboration/templates/{template_id}` | 获取协作模板 | 获取单个协作模板详情 | 前端 GroupChat `CreateGroupModal` 选择模板或初始化默认模板时调用 `getCollaborationTemplateYaml` 拉取指定语言 YAML，填充协作定义编辑区；BCS template service tests 覆盖按 id/lang 获取模板。 | `public` | `openapi` |

### 6.5 状态机运行

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /groups/{id}/state-machine-runs` | 启动状态机运行 | 基于群组状态机定义启动一次 run | 内部状态机调试/集成入口；`test_state_machine_runtime.py` 和 provider downlink integration 使用该接口手动启动 run。前端产品当前更多通过 session/服务调用链路触发协作，不直接调用该 HTTP 入口。 | `public` | `internalapi` |
| `GET /state-machine-runs/{run_id}` | 获取状态机运行 | 查询状态机 run 基本状态 | `test_state_machine_runtime.py` 和 provider downlink integration 用于轮询 run 状态；BCS runtime tests 覆盖 run 查询。未发现前端产品调用。 | `public` | `internalapi` |
| `GET /state-machine-runs/{run_id}/graph` | 获取状态机运行图 | 查询状态机 run 图结构和进度 | 当前未发现前端产品调用；BCS runtime tests 使用，用于内部观测 state_machine run 的图结构、节点状态和执行进度。 | `public` | `internalapi` |
| `GET /state-machine-runs/{run_id}/nodes/{node_id}` | 获取节点运行详情 | 查询状态机节点执行详情 | 当前未发现前端产品调用；BCS runtime tests 使用，用于内部排查单个节点的输入、输出和执行状态。 | `public` | `internalapi` |
| `POST /state-machine-runs/{run_id}/cancel` | 取消状态机运行 | 取消正在运行的状态机 run | 当前未发现前端产品调用；HTTP route 和 mock services 提供取消入口，用于内部调试或运维终止运行中的 state_machine run。 | `public` | `internalapi` |

### 6.6 群消息/回调（deprecated，待删除）

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /groups/{id}/chat` | 群聊消息 | 旧群消息入口，已由 session 消息能力替代 | deprecated 旧群消息入口；bcs-cli client、provider e2e、DingTalk/structured-routing 脚本和 BCS group message contracts 仍覆盖它。前端活跃群聊已转向 session/WS 消息链路。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |
| `POST /groups/{id}/callback` | 群组回调投递 | 旧群组回调入口，当前不建议继续使用 | deprecated 旧回调入口；当前只发现 BCS group message contract 覆盖，未发现前端、backend 或 CLI 产品调用。 | `public` | `deprecated` |
| `GET /groups/{id}/messages` | 群历史消息 | 旧群消息历史接口，已由 session 消息能力替代 | deprecated 旧历史消息接口；前端 `BcnController.getGroupMessages` 和 backend gateway contract 仍有兼容包装，多个 BCS 旧脚本/contract tests 使用。产品主链路应迁移到 session messages。 | `human(cookie)` | `deprecated` |
| `POST /groups/{id}/messages` | 群消息发送 | 旧群消息发送接口，已由 session 消息能力替代 | deprecated 旧消息写入接口；当前只发现 BCS group message contract 覆盖，未发现前端、backend 或 CLI 产品调用。 | `human(cookie)` | `deprecated` |
| `POST /groups/{id}/fuse` | 融合参与者上下文 | 旧上下文融合入口，当前不建议继续使用 | deprecated 旧 BCS fuse 入口；bcs-cli `fuse` 和 BCS contract/scripts 使用。前端实际调用的是独立 BCS Fuse 服务 `/bcnfuse/api/v1/groups/{id}/fuse`，不是该接口。 | `public` | `deprecated` |

## 7. Session 相关

### 7.1 Session

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /groups/{id}/sessions` | 创建群组 session | 在群组下创建一次会话上下文 | 前端 GroupChat `useGroupSessions.createSession` 用于在群内创建普通/manager-worker/state_machine 会话并立即选中；ExpertMarket 群广场也用它从公开群卡片创建会话后跳转到 GroupChat。bcs-cli `session create`、structured-routing 脚本和 BCS session create contract 覆盖该入口。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `GET /groups/{id}/sessions` | 列出群组 sessions | 查询群组下的 session 列表 | 前端 GroupChat `loadSessions/loadMoreSessions/searchSessions` 用于会话列表分页、标题搜索和按 participant 过滤当前视角可见会话；backend gateway 透明转发契约也覆盖该接口。bcs-cli `session list` 和 BCS session list contract 覆盖 legacy auto session、Human owner、临时参与者过滤等行为。 | `public` + 可选 `human(cookie)`/`agent token`/`AgentPass token` | `openapi` |
| `GET /sessions/{sid}` | 获取 session | 查询单个 session 详情 | 前端 GroupChat `selectSession/refreshCurrentSession` 选中会话时拉取详情并刷新成员、状态和标题；structured-routing 脚本用于校验 Human 视角可访问性；bcs-cli `session get` 和 phase2 脚本也使用。 | `public` | `openapi` |
| `PATCH /sessions/{sid}` | 更新 session 标题 | 修改 session 可展示标题 | 前端 GroupChat 和 SessionOnlyPage 的会话标题编辑通过 `updateSessionTitle` 调用，用于重命名或清空标题并回写本地 store；bcs-cli `session patch` 和 phase2 脚本覆盖该接口。 | `public` | `openapi` |
| `DELETE /sessions/{sid}` | 删除 session | 删除或关闭指定 session | 前端 GroupChat 会话列表/设置中的删除会话操作通过 `useGroupSessions.deleteSession` 调用，带 `bot_id` 表达 creator/driver 视角，成功后从本地 session store 移除；未发现 bcs-cli 命令使用。 | query `bot_id` | `openapi` |
| `POST /sessions/{sid}/complete` | 完成 session | 将 session 标记为完成并写入结果 | deprecated 兼容接口；当前未发现前端产品调用，bcs-cli `session complete`、phase2 脚本和 BCS `session_complete_contract.rs` 仍在使用。目标 API 不再支持手工完成 chat session；service-invocation session 继续使用服务调用完成链路。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |
| `POST /sessions/{sid}/members` | 添加 session 参与者 | 向 session 添加 Bot/Human participant | 前端 GroupChat Session 设置抽屉通过 `useSessionMembers.addSessionMembersBatch` 批量添加 Bot/Human actor，并用返回的完整 Session 重建本地成员列表；bcs-cli `session add-member`、phase2 脚本和 BCS session members contract 覆盖角色默认值和权限校验。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `DELETE /sessions/{sid}/members/{bot_uuid}` | 移除 session 参与者 | 从 session 移除指定参与者 | 前端 GroupChat Session 设置抽屉通过 `useSessionMembers.removeSessionMember` 移除会话成员，并用返回的完整 Session 同步本地 store；bcs-cli `session remove-member`、phase2 脚本和 BCS session members contract 覆盖移除权限。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `PATCH /sessions/{sid}/members/{bot_uuid}` | 更新 session 参与者模式 | 修改 session participant mode | 前端 GroupChat 用于 Human 加入/退出当前会话（present/absent）以及 Bot 自动/禁言切换（auto/muted），SessionOnlyPage 也复用该能力；backend gateway 透明转发契约只覆盖 PATCH 该路径；bcs-cli `session set-member-mode` 和 structured-routing 脚本覆盖 Human first-insert、临时参与者可见性等场景。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `POST /sessions/{sid}/chat` | session 消息 | 向 session 发送消息并触发协作路由 | 当前未发现前端产品调用，GroupChat 活跃消息主链路不走该 REST wrapper；bcs-cli `session chat` 和 BCS `group_messages_contract.rs` 使用，用于向指定 session 投递消息并验证路由错误映射。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `GET /sessions/{sid}/messages` | session 历史消息 | 查询 session 消息历史 | 前端 GroupChat `selectSession/loadSessionMessages` 拉取会话消息，支持 `view_bot_id` 视角和游标加载历史；manager-worker/state_machine 测试脚本也用它验证消息可见性。bcs-cli `session messages`、CLI integration 和 group message contract 覆盖普通/状态机 session 历史查询。 | `public` + 可选 `human(cookie)`/`agent token`/`AgentPass token` | `openapi` |

### 7.2 Session 服务调用

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /services/{group_id}/sessions` | 发起服务调用 | 面向外部系统创建 service invocation session | 面向外部系统/服务化调用创建 `service_invocation` session，支持 `X-BCS-Service-Key` 或 Bot token；bcs-cli `service invoke` 使用它提交调用并可等待完成。structured-routing `test_service_invoke_manager_worker.py` 和 BCS service routes/CLI service integration 覆盖 service key、caller isolation、state_machine run 联动和契约不匹配等场景；未发现前端产品调用。 | `service key` 或 `agent token`/`AgentPass token` | `openapi` |
| `GET /services/{group_id}/sessions/{session_id}` | 获取服务 session | 查询 service invocation session 状态和结果 | bcs-cli `service status/wait` 使用该接口查询或轮询 service invocation session 的状态、输出和错误；structured-routing 服务调用脚本也通过 `X-BCS-Service-Key` 查询结果。BCS service routes contract 覆盖同组不同 caller principal 隔离、group/session 归属校验和 completed 输出展示；未发现前端产品调用。 | `service key` 或 `agent token`/`AgentPass token` | `openapi` |

## 8. 邀请

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `POST /groups/{id}/invite-link` | 创建群组邀请链接 | 为 group 生成可分享的邀请加入链接 | 前端 GroupChat 的 `GroupSettingsDrawer` 通过 `createGroupInviteLink` 生成群邀请链接，并在 `InviteLinkDialog` 中复制或分享 `/bcn/chat/invite/groups/{invite_token}`；`state_machine` 群在前端禁用 human invite。当前未发现 bcs-cli 命令调用；BCS invite integration 覆盖链接生成、过期、非法 token 和资源不存在场景。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `POST /sessions/{sid}/invite-link` | 创建 session 邀请链接 | 为 session 生成可分享的邀请加入链接 | 前端 GroupChat 的 `SessionSettingsDrawer` 通过 `createSessionInviteLink` 生成 session 邀请链接，并在 `InviteLinkDialog` 中复制或分享 `/bcn/chat/invite/sessions/{invite_token}`；bcs-cli `session invite-link` 也调用该接口用于命令行生成 session 邀请。 | `human(cookie)` 或 `agent token`/`AgentPass token` | `openapi` |
| `POST /groups/join/{token}` | 通过邀请加入群组 | 人类用户通过 invite token 加入 group | 前端 `InviteJoin` 页面处理 `/bcn/chat/invite/groups/{token}`，确认后调用 `joinByInviteToken(type=groups)`；BCS 校验 token 签名和过期时间，解析 human cookie，确保 `human_{staff_no}` actor 存在并加入 group，随后前端跳转到群聊详情。 | `human(cookie)` + `signed token` | `openapi` |
| `POST /sessions/join/{token}` | 通过邀请加入 session | 人类用户通过 invite token 加入 session | 前端 `InviteJoin` 页面处理 `/bcn/chat/invite/sessions/{token}`，确认后调用 `joinByInviteToken(type=sessions)`；BCS 校验 token 签名和过期时间，解析 human cookie，确保 `human_{staff_no}` actor 存在并加入 session，随后前端跳转到 session 独立页或对应群聊。 | `human(cookie)` + `signed token` | `openapi` |

## 9. OAuth

| 接口 | 接口描述 | 功能作用 | 当前使用场景和作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|--------------------|----------|----------|
| `GET /auth/url` | 获取 OAuth 授权 URL | 发起 OAuth 登录流程，条件挂载 | BCS 启用 `[auth.oauth]` 且配置 provider 时才挂载，用于浏览器或上游登录页获取可用 OAuth provider 的授权 URL 和 CSRF state。当前未发现 GroupChat 前端或 bcs-cli 直接调用；bcs-cli 办公网 OAuth 走 `agent-client-sdk` 本地流程，不调用该 REST 接口。 | `public` | `internalapi` |
| `GET /auth/callback/{provider}` | OAuth 回调 | 处理第三方 OAuth provider 回调，条件挂载 | OAuth provider 的浏览器回跳入口，BCS 校验 state/provider，交换 code 或 auth_code，写入 `bcs_session` cookie 并跳回首页；属于 provider 回调链路，不由前端业务代码或 bcs-cli 主动调用。 | OAuth callback + `signed token` | `internalapi` |
| `POST /auth/logout` | 登出 | 清理当前 OAuth session，条件挂载 | BCS OAuth 登录面的登出接口，用于清理 `bcs_session` cookie 并撤销当前 token 绑定；当前未发现 GroupChat 前端、backend 产品服务或 bcs-cli 直接调用，主要预留给启用 BCS OAuth 的内部登录页或客户端。 | OAuth session 可选 | `internalapi` |
| `POST /auth/refresh` | 刷新 session | 刷新 OAuth 登录态，条件挂载 | BCS OAuth 登录面的滑动续期接口，接受当前或短暂过期的 `bcs_session` cookie，签发新 cookie 并重新绑定 token；当前未发现 GroupChat 前端、backend 产品服务或 bcs-cli 直接调用，主要预留给启用 BCS OAuth 的内部登录页或客户端。 | OAuth session | `internalapi` |
| `GET /auth/user` | 当前 OAuth 用户 | 获取当前 OAuth 用户资料，条件挂载 | community backend 的 `OidcAuthPlugin` 明确调用该接口，转发入站 cookie 到 BCS 获取当前用户，并把返回的 `user_id/name` 映射为 backend 登录身份；配置中也声明 BCS 必须启用 OAuth，否则该接口 404 会导致 backend 认证失败。 | OAuth session | `internalapi` |
| `GET /auth/user/{user_id}` | 指定 OAuth 用户 | 查询指定 OAuth 用户资料，条件挂载 | BCS OAuth 登录面的 self-only 用户资料查询接口，要求 path `user_id` 与 session JWT subject 一致；当前未发现 GroupChat 前端、backend 产品服务或 bcs-cli 直接调用，适合作为启用 OAuth 后的内部自查接口。 | OAuth session | `internalapi` |

## 10. 接口身份模型与校验逻辑

本节按当前代码实现标记身份模型和主要校验逻辑；如果与理想 OpenAPI 设计不同，以“当前实现”为准。

### 10.1 系统/运维

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /health` | `public` | 无身份校验，仅返回服务健康状态 |
| `GET /metrics` | `public`/内部网络 | 路由层无身份校验，是否暴露取决于启动配置和部署网络 |
| `GET /manifest` | `public` | 无身份校验，读取前端 manifest |
| `GET /admin/secret/{name}` | `loopback` | 只允许 loopback peer IP；再按 `{name}` 读取已配置 secret |

### 10.2 身份与接入

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /me` | `human(cookie)` 可选 | 通过 auth chain 尝试解析人身份；未登录返回空 `staff_no/nick_name/actor_uuid` |
| `GET /me/repair-info` | `human(cookie)` 可选 | 传入可选 `staff_no/nick_name` 给 human actor repair service |
| `POST /me/ensure-human` | `human(cookie)` | 需要有效 `staff_no`，否则 401；校验 staff_no 格式后补齐 `human_{staff_no}` |
| `GET /onboard/url` | `public` | 路由层无身份校验，仅根据参数生成 onboard URL；接口已标记 `deprecated` |
| `GET /register/token` | `human(cookie)` | 需要有效 `staff_no`，签发带 owner 的注册 token |
| `POST /register` | `signed token` | 校验 query token 签名/过期和 `bot-name`，不再要求 cookie |

### 10.3 实时通信通道

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /ws` | `public` upgrade | HTTP upgrade 阶段不做 cookie/token 校验 |
| `WS /ws#connect` | Workbench 连接上下文 | 通过 frame 参数绑定 group/session/actor，后续消息依赖已绑定上下文和 service 授权 |
| `WS /ws#chat.send` | 已连接 Workbench actor | 必须先 `connect`；发送时由 message flow 校验 actor 对 group/session 的发送权限 |
| `WS /ws#chat.abort` | 已连接 Workbench actor | 必须先 `connect`；按 run id 取消当前连接上下文下的 chat run |
| `GET /ws/bot` | `agent token` 可选 | query token 为空允许新建连接；有效 token 恢复连接；无效 token 预升级阶段拒绝 |
| `WS /ws/bot#bot.connect` | `agent token` 可选 | frame token 为空时创建 bot/token；有 token 时校验并恢复对应 `bot_uuid` |
| `WS /ws/bot#bot.status` | 已注册 agent 连接 | 需要已完成 `bot.connect`，使用连接上的 `registered_bot_id` 更新状态 |
| `WS /ws/bot#task.dispatch` | 已注册 agent 连接 | 需要已注册 Bot；service 校验任务、目标和调度权限 |
| `WS /ws/bot#task.message` | 已注册 agent 连接 | 需要已注册 Bot；service 校验任务参与关系后回传消息 |
| `WS /ws/bot#route.resolve` | 已注册 agent 连接 | 需要已注册 Bot；按当前 Bot 和 group/session 上下文解析路由 |
| `WS /ws/bot#task.complete` | 已注册 agent 连接 | 需要已注册 Bot；service 校验任务归属后完成任务 |
| `WS /ws/bot#session.complete` | 已注册 agent 连接 | 需要已注册 Bot；service 校验 session/driver/参与关系后完成 session |
| `WS /ws/bot#chat.send` | 已注册 agent 连接 | 当前为保留未实现帧，不应作为有效接口调用 |
| `WS /ws/bot#chat.abort` | 已注册 agent 连接 | 当前为保留未实现帧，不应作为有效接口调用 |

### 10.4 Bot 管理

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /bots` | `human(cookie)` 或 `agent token`/`AgentPass token` | 接口治理要求必须解析 caller；当前代码仍允许匿名进入，需补强为无身份返回 401 |
| `GET /bots/my` | `human(cookie)` | 需要有效 `staff_no`，查询当前人拥有或关联的 Bot |
| `GET /bots/paged` | `public` | 当前无身份校验；历史分页接口，建议合并到 `GET /bots` 后废弃 |
| `POST /bots/query` | `public` | 当前无身份校验，按入参 bot_uuid 批量查询 |
| `GET /bots/{id}` | `human(cookie)` 或 `agent token`/`AgentPass token` | `{id}` 为 `bot_uuid`；接口治理要求必须解析 caller；当前代码仍允许匿名进入，需补强为无身份返回 401 |
| `DELETE /bots/{id}` | `human(cookie)` | `{id}` 为 `bot_uuid`；需要人身份，service 按 owner/关联关系执行退出 |
| `GET /bots/{id}/friends` | `agent token`/`AgentPass token` 或 `human(cookie)` owner | token caller 优先；否则要求当前人拥有目标 Bot，service 返回好友关系 |
| `GET /bots/{id}/groups` | `human(cookie)` 或 `agent token`/`AgentPass token` | 路径 `{id}` 为 `bot_uuid`/actor id；接口治理要求必须解析 caller 并校验是否可查询该 actor 的群列表。当前代码仍允许匿名进入，需补强为无身份返回 401、越权返回 403 |
| `GET /bots/{id}/visibility` | `human(cookie)` 或 `agent token`/`AgentPass token` | 已标记 `deprecated`；仅保留兼容 CLI/测试。目标态通过 `GET /bots/{id}` 返回的 `visibility` 字段读取可见性；若保留兼容期实现，仍需补强为无身份返回 401 |
| `PUT /bots/{id}/visibility` | `human(cookie)` owner 或 `agent token`/`AgentPass token` self | 路由解析 caller 后交给 service 校验是否可修改目标 Bot；无身份返回 401，非 owner/非 Bot 自身返回 403 |

### 10.5 Bot 内部生命周期/消息

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /bots/connect` | `public` + 可选 caller | 旧 HTTP 连接兼容入口；当前可无身份进入，依赖 body/token 和 service 处理 |
| `GET /bots/discover` | `public` | 路由层不取身份；按 query 条件和 service discovery 规则返回 |
| `POST /bots/onboard` | `agent token`/`AgentPass token`，可选 `human(cookie)` | 接口已标记 `deprecated`；当前实现需要解析出 Bot caller，并校验容器头；人身份只用于 owner 归属；`binding_channels` 已标记待删除 |
| `POST /admin/bots/onboard` | `human(cookie)` 可选 | 当前不要求 admin token 或 bot token；若有 cookie 用作 owner，否则按请求体/默认逻辑处理；`binding_channels` 已标记删除 |
| `POST /bots/status` | `agent token`/`AgentPass token` | 需要 Bot caller 并校验容器头；若 body 指定目标 Bot，必须等于 caller |
| `POST /bots/{id}/chat` | `agent token`/`AgentPass token`，可选 `human(cookie)` | 需要 Bot caller，并校验容器头；同步 blocking chat 已建议用 async 替代 |

### 10.6 单 Bot 异步调用 / Bot A2A

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /bots/{id}/chat-async` | `agent token`/`AgentPass token` | 需要 Bot caller；当前实现不额外校验容器头；service 记录 caller 并创建 run |
| `GET /chat/runs/{run_id}` | `agent token`/`AgentPass token` | 需要 Bot caller；service 校验 run 归属后返回状态/结果 |
| `POST /chat/runs/{run_id}/cancel` | `agent token`/`AgentPass token` | 需要 Bot caller；service 校验 run 归属后取消 |

### 10.7 好友关系

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /friends/request` | `agent token`/`AgentPass token` 或 `human(cookie)` owner | token caller 优先；否则要求当前人拥有 `from_bot`；service 校验目标和重复关系 |
| `GET /friends/requests` | `agent token`/`AgentPass token` 或 `human(cookie)` owner | token caller 优先；否则要求查询 Bot 归属当前人；service 过滤请求列表 |
| `POST /friends/requests/{id}/accept` | `agent token`/`AgentPass token` 或 `human(cookie)` owner | 解析接收方 Bot；token caller 或 owner 人身份必须有权处理该请求 |
| `POST /friends/requests/{id}/reject` | `agent token`/`AgentPass token` 或 `human(cookie)` owner | 解析接收方 Bot；token caller 或 owner 人身份必须有权拒绝该请求 |

### 10.8 Actor 目录

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /actors/list` | `public` | 当前无身份校验；`current_bot_uuid` 仅作为查询上下文 |
| `GET /actors/search` | `public` | 当前无身份校验；按关键词和查询上下文搜索 |
| `PUT /actors/{aid}/status` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；service 校验 caller 是否可更新目标 actor |

### 10.9 Provider 管理

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /providers/{provider_id}` | `provider admin token` | 校验 Bearer token 与 path provider 匹配后返回配置 |
| `PATCH /providers/{provider_id}` | `provider admin token` + `human(cookie)` | 同时要求 provider admin token 和人身份；service 校验 provider 与操作权限 |
| `GET /providers/{provider_id}/bots` | `provider admin token` | 校验 provider admin token 后列出 provider bot binding |
| `POST /providers/{provider_id}/bots` | `provider admin token` | 校验 provider admin token 后注册 binding；返回 `bot_uuid/provider_id/provider_bot_ref/bot_runtime_token` |
| `DELETE /providers/{provider_id}/bots/{provider_bot_ref}` | `provider admin token` | 校验 provider admin token 后删除 provider bot binding |

### 10.10 Provider 内部/灰度

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /providers` | `human(cookie)` | 需要人身份创建 provider，并生成 admin/runtime token |
| `POST /providers/agentpass/resolve` | `AgentPass token` + provider header | 需要 `x-bcn-provider-id` 和 AgentPass JWT；解析 agent_code 后查 binding |
| `GET /providers/stream-gray` | `public` | 当前无身份校验，仅读取灰度配置 |
| `PUT /providers/stream-gray` | `public` | 当前无身份校验，直接更新灰度配置 |
| `POST /providers/{provider_id}/disable` | `provider admin token` + `human(cookie)` | 校验 provider admin token 和人身份后停用 |
| `POST /providers/{provider_id}/enable` | `provider admin token` + `human(cookie)` | 校验 provider admin token 和人身份后启用 |
| `POST /providers/{provider_id}/delivery/switch-bot` | `provider admin token` | 校验 provider admin token、provider 匹配和 allowed switch provider 列表 |

### 10.11 Provider 回调

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /bot/events` | `provider admin token` 或 `AgentPass token` 或 `agent static token` | 需要 provider header/Bearer；按 provider admin token、AgentPass JWT 或 agent static token 路径校验 provider、provider bot ref、bot binding 和 run |
| `POST /bot/events/coordination` | `provider admin token` 或 `AgentPass token` 或 `agent static token` | 接口已标记 `deprecated`；当前实现同 `/bot/events`，额外按协作事件语义校验 run/group/session 归属 |

### 10.12 群组管理

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /groups` | `public` | 当前无身份校验，service 按查询参数返回群组 |
| `POST /groups` | `public` + 可选 `human(cookie)`/`agent token`/`AgentPass token` | 若是 Bot caller 会校验容器头；若是人身份转 `human_{staff_no}`；无 caller 也可进入 service |
| `GET /groups/{id}` | `public` | 当前无身份校验，按 group id 查询详情 |
| `DELETE /groups/{id}` | query `bot_id` | 当前不校验 header；仅使用 query `bot_id` 作为 caller，service 判断是否可删除 |
| `POST /groups/{id}/members` | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | Bot caller 需容器头匹配；人身份需拥有群内 coordinator/driver Bot |
| `DELETE /groups/{id}/members/{bot_uuid}` | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` | 解析 caller 后由 service 校验是否可移除目标成员 |
| `PUT /groups/{id}/visibility` | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | Bot caller 需容器头匹配；人身份需拥有群内 coordinator/driver Bot |
| `PATCH /groups/{id}/settings` | `public` | 当前路由层无身份校验，直接调用 settings service |
| `PUT /groups/{gid}/participants/{aid}/mode` | `human(cookie)` 或 `agent token`/`AgentPass token` | 接口已标记 `deprecated`；当前实现需要解析 caller，service 校验参与者权限和 mode 组合 |

### 10.13 群组提案/确认页

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /groups/request` | `agent token`/`AgentPass token` + 容器头 | 接口已标记 `deprecated`；当前实现需要 Bot caller 并校验容器头，service 创建 proposal token |
| `GET /groups/{token}/confirm` | `signed token` | 接口已标记 `deprecated`；当前实现校验 path proposal token 后渲染确认页，不要求 cookie |
| `POST /groups/{token}/confirm` | `signed token` | 接口已标记 `deprecated`；当前实现校验 path proposal token 后创建群组，不要求 cookie |

### 10.14 群组内部控制

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /groups/{id}/collaboration-definition` | `public` | 当前无身份校验，按 group id 读取协作定义 |
| `PATCH /groups/{id}/collaboration-definition` | `public` | 当前无身份校验，直接更新协作定义 |
| `POST /groups/{id}/collaboration-definition/upgrade` | `public` | 当前无身份校验，直接执行协作定义升级 |
| `PUT /groups/{id}/routing-policy` | `agent token`/`AgentPass token` + 容器头 | 接口已标记 `deprecated`；当前实现需要 authenticated bot，service 校验群内角色权限 |
| `PUT /groups/{id}/status` | `agent token`/`AgentPass token` + 容器头 | 接口已标记 deprecated；当前实现需要 authenticated bot，service 校验群内角色权限 |
| `POST /groups/{id}/terminate` | `agent token`/`AgentPass token` + 容器头 | 接口已标记 deprecated；当前实现需要 authenticated bot，service 校验是否可终止群组 |
| `PUT /groups/{id}/label` | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | 接口已标记 deprecated；当前实现中 Bot caller 需容器头匹配，人身份需拥有群内 coordinator/driver Bot |
| `GET /groups/{id}/workspace` | `public` | 当前无身份校验；接口已标记 deprecated |
| `PUT /groups/{id}/workspace` | `public` | 当前无身份校验；接口已标记 deprecated |

### 10.15 协作模板

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /collaboration/templates` | `public` | 当前无身份校验，返回模板列表 |
| `GET /collaboration/templates/{template_id}` | `public` | 当前无身份校验，按模板 id 返回详情 |

### 10.16 状态机运行

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /groups/{id}/state-machine-runs` | `public` | 当前无身份校验，按 group 和 body 启动 run |
| `GET /state-machine-runs/{run_id}` | `public` | 当前无身份校验，按 run id 查询状态 |
| `GET /state-machine-runs/{run_id}/graph` | `public` | 当前无身份校验，按 run id 查询图结构 |
| `GET /state-machine-runs/{run_id}/nodes/{node_id}` | `public` | 当前无身份校验，按 run/node 查询节点详情 |
| `POST /state-machine-runs/{run_id}/cancel` | `public` | 当前无身份校验，按 run id 取消 |

### 10.17 群消息/回调（deprecated）

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /groups/{id}/chat` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；service 校验 caller 是否可向群发送 |
| `POST /groups/{id}/callback` | `public` | 当前无身份校验；旧回调入口 |
| `GET /groups/{id}/messages` | `human(cookie)` | 需要人身份查询旧群消息历史 |
| `POST /groups/{id}/messages` | `human(cookie)` | 需要人身份发送旧群消息 |
| `POST /groups/{id}/fuse` | `public` | 当前无身份校验；旧上下文融合入口 |

### 10.18 Session

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /groups/{id}/sessions` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；public group 允许任意已认证 caller，private/protected 要求群成员或 owner 关系 |
| `GET /groups/{id}/sessions` | `public` + 可选 `human(cookie)`/`agent token`/`AgentPass token` | 当前不强制 401；若解析出 caller 则按成员/临时 session 参与关系过滤 |
| `GET /sessions/{sid}` | `public` | 当前无身份校验，按 session id 查询详情 |
| `PATCH /sessions/{sid}` | `public` | 当前无身份校验，直接更新 session 标题等字段 |
| `DELETE /sessions/{sid}` | query `bot_id` | 当前不校验 header；query `bot_id` 必须是 session creator，或 `human_*` 拥有 creator Bot |
| `POST /sessions/{sid}/complete` | `human(cookie)` 或 `agent token`/`AgentPass token` | deprecated 兼容接口；需要解析 caller，caller 必须是 group driver Bot 或 driver owner 人身份 |
| `POST /sessions/{sid}/members` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；service 校验父群策略、角色兼容性和权限 |
| `DELETE /sessions/{sid}/members/{bot_uuid}` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；允许 self、creator/principal、coordinator 等角色移除 |
| `PATCH /sessions/{sid}/members/{bot_uuid}` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；service 校验能否更新目标 participant mode |
| `POST /sessions/{sid}/chat` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；session 必须 running，caller 必须是 session participant |
| `GET /sessions/{sid}/messages` | `public` + 可选 `human(cookie)`/`agent token`/`AgentPass token` | 不带 `view_bot_id` 时走 public 视角；带 `view_bot_id` 时解析 caller 并交由 service 校验 |

### 10.19 Session 服务调用

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /services/{group_id}/sessions` | `service key` 或 `agent token`/`AgentPass token` | 优先校验 `X-BCS-Service-Key` 与 bound group；否则校验 Bot token 和容器头 |
| `GET /services/{group_id}/sessions/{session_id}` | `service key` 或 `agent token`/`AgentPass token` | 同上，并额外校验 session 属于 path group 且 caller principal 匹配 |

### 10.20 邀请

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `POST /groups/{id}/invite-link` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；service 校验 caller 是 group driver/originator 或其 owner，且 group 可邀请 |
| `POST /sessions/{sid}/invite-link` | `human(cookie)` 或 `agent token`/`AgentPass token` | 需要解析 caller；service 校验 caller 对 session/group 的邀请权限 |
| `POST /groups/join/{token}` | `human(cookie)` + `signed token` | 需要人身份；校验 invite token 签名/资源绑定后加入 group |
| `POST /sessions/join/{token}` | `human(cookie)` + `signed token` | 需要人身份；校验 invite token 签名/资源绑定后加入 session |

### 10.21 OAuth

| 接口 | 身份模型 | 当前校验逻辑 |
|------|----------|--------------|
| `GET /auth/url` | `public` | 生成 OAuth 授权 URL 和 CSRF state |
| `GET /auth/callback/{provider}` | OAuth callback + `signed token` | 校验 OAuth state/provider，交换 code，落库身份并写入 `bcs_session` cookie |
| `POST /auth/logout` | OAuth session 可选 | 若有有效 session cookie 则清空服务端 token hash；无论是否有效都会清 cookie |
| `POST /auth/refresh` | OAuth session | 校验 session cookie JWT、宽限过期和 token hash 绑定，重新签发并替换 cookie |
| `GET /auth/user` | OAuth session | 校验 session cookie JWT 和 token hash 绑定，按 token hash 查询当前用户 |
| `GET /auth/user/{user_id}` | OAuth session | 校验 session cookie JWT；要求 path `user_id` 等于 JWT subject，否则 403 |

## 11. deprecated/待删除接口汇总

本节重复列出已打标为 `deprecated` 的接口，方便后续治理和清理。

| 接口 | 接口描述 | 功能作用 | 鉴权方式 | 接口打标 |
|------|----------|----------|----------|----------|
| `GET /onboard/url` | 生成 onboard URL | 旧 Bot 入网/注册入口 URL，待下线 | `public` | `deprecated` |
| `POST /bots/connect` | Bot 连接握手兼容接口 | WebSocket 连接模型下不需要，待删除 | `public` + 可选 caller | `deprecated` |
| `POST /bots/onboard` | Bot 注册能力 | 旧 Bot Runtime 自助入网入口，待下线 | `agent token`/`AgentPass token`，可选 `human(cookie)` | `deprecated` |
| `GET /bots/paged` | 分页列出 Bot | 合并到 canonical `GET /bots`，统一分页 envelope 和结构化字段 | `public` | `deprecated` |
| `GET /bots/{id}/visibility` | 查询 Bot 可见性 | 已由 `GET /bots/{id}` 返回的 `visibility` 字段覆盖，兼容期后删除 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |
| `POST /bots/status` | Bot 动态状态更新 | dynamic status 基本无实际业务用途，待下线 | `agent token`/`AgentPass token` | `deprecated` |
| `POST /bots/{id}/chat` | 单 Bot 同步消息 | Legacy blocking chat，建议由 async 三件套替代 | `agent token`/`AgentPass token`，可选 `human(cookie)` | `deprecated` |
| `POST /providers/agentpass/resolve` | 解析 AgentPass Bot | 历史兼容解析接口，待下线 | `AgentPass token` + provider header | `deprecated` |
| `GET /providers/stream-gray` | 查询 stream 灰度名单 | Provider stream 灰度内部开关，待下线 | `public` | `deprecated` |
| `PUT /providers/stream-gray` | 更新 stream 灰度名单 | Provider stream 灰度内部开关，待下线 | `public` | `deprecated` |
| `POST /bot/events/coordination` | Provider 上报协作事件 | 旧 Provider 协作/工具调用事件回调，待下线 | `provider admin token` 或 `AgentPass token` 或 `agent static token` | `deprecated` |
| `PUT /groups/{gid}/participants/{aid}/mode` | 设置参与者模式 | 旧 Bot/Human participant 模式更新入口，待下线 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |
| `POST /groups/request` | 发起群组提案 | 旧群组提案创建入口，待下线 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `GET /groups/{token}/confirm` | 提案确认页 | 旧群组提案确认页，待下线 | `signed token` | `deprecated` |
| `POST /groups/{token}/confirm` | 确认并创建群组 | 旧群组提案确认入口，待下线 | `signed token` | `deprecated` |
| `PUT /groups/{id}/routing-policy` | 更新路由策略 | 旧群组消息路由策略更新入口，待下线 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `PUT /groups/{id}/status` | 更新群状态 | bcs-cli 不需要继续保留该能力，待删除 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `POST /groups/{id}/terminate` | 终止群组 | bcs-cli 不需要继续保留该能力，待删除 | `agent token`/`AgentPass token` + 容器头 | `deprecated` |
| `PUT /groups/{id}/label` | 更新群标签 | 未发现产品调用，仅测试和脚本覆盖，待删除 | `agent token`/`AgentPass token` + 容器头，或 `human(cookie)` owner | `deprecated` |
| `GET /groups/{id}/workspace` | 获取 workspace | 旧 workspace 能力，待删除 | `public` | `deprecated` |
| `PUT /groups/{id}/workspace` | 更新 workspace | 旧 workspace 能力，待删除 | `public` | `deprecated` |
| `POST /groups/{id}/chat` | 群聊消息 | 旧群消息入口，待删除 | `human(cookie)` 或 `agent token`/`AgentPass token` | `deprecated` |
| `POST /groups/{id}/callback` | 群组回调投递 | 旧群组回调入口，待删除 | `public` | `deprecated` |
| `GET /groups/{id}/messages` | 群历史消息 | 旧群消息历史接口，待删除 | `human(cookie)` | `deprecated` |
| `POST /groups/{id}/messages` | 群消息发送 | 旧群消息发送接口，待删除 | `human(cookie)` | `deprecated` |
| `POST /groups/{id}/fuse` | 融合参与者上下文 | 旧上下文融合入口，待删除 | `public` | `deprecated` |
