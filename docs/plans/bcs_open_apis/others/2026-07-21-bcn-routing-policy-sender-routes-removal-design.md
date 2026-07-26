# BCN RoutingPolicy `sender_routes` 收敛设计

## 决策

新版 BCN 核心领域模型不再包含 `RoutingPolicy.sender_routes`。

`sender_routes` 是现有 BCS 运行时支持的静态 Bot 转发配置，但没有产品侧配置入口，且其流程编排语义已可由结构化消息路由和 `CollaborationDefinition` 更清晰地表达。因此，它不应继续作为新版 Group 核心领域模型的一部分。

## 变更范围

- 从 `new_apis/domain-models.yaml` 的 `RoutingPolicy` 中删除 `sender_routes` 属性及必填声明。
- 同步更新新版领域文档中对 `RoutingPolicy` 的描述。
- 重新校验 Swagger 领域模型投影和全部 API bundle。

## 兼容边界

- 不修改现有 Rust 领域结构和消息路由实现。
- 不修改 `bcs_groups.routing_policy_json` 数据库存储。
- 不修改历史接口及其兼容行为。
- 存量调用方仍可通过旧接口使用 `sender_routes`；新版 OpenAPI 不再将其声明为核心领域能力。

## 验证

- 完整领域 Schema 和 Swagger 投影中的 `RoutingPolicy` 均不存在 `sender_routes`。
- `RoutingPolicy` 的其他字段保持不变。
- All、OpenAPI、Internal API 和 Domain Models 四个文档 bundle 均可成功构建。
