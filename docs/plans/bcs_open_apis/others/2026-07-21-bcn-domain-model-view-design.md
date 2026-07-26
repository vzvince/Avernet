# BCN Domain Models 展示投影设计

## 目标

Swagger 的 `BCN Domain Models` 页面优先展示 Actor、Group、Session，再展示 Provider 等支撑对象；枚举、值对象和嵌套对象不再作为独立 Model 出现在页面底部。

## 设计

`src/bcs-internal/docs/new_apis/domain-models.yaml` 继续作为完整、权威的领域 Schema 来源，供所有 API bundle 通过 `$ref` 使用，不删除任何 Schema。

文档 Server 在构建 `domain-models` 视图时生成展示投影：

1. 按固定顺序选择 Actor、Group、Session、Provider、ProviderBotBinding、Friendship、FriendRequest、Invitation、CollaborationTemplate、StateMachineRun；
2. 递归解析这些对象内部指向 `#/components/schemas/...` 的 `$ref`；
3. 将枚举、值对象和嵌套对象内联到上层字段；
4. 投影最终只保留上述 10 个顶层 Schema，并保证没有未解析 `$ref`；`Message` 继续保留在权威 Schema 中供 Session 消息 API 引用，但不在领域对象页面单独展示；
5. OpenAPI、Internal API 和 All APIs 仍合并完整 Schema，不受展示投影影响。

## 验证

`serve_api_docs.py --check` 校验领域视图的 Schema 名称和顺序、确保隐藏对象不再单独出现、确保投影没有 `$ref`，并继续校验其他三个 API bundle。
