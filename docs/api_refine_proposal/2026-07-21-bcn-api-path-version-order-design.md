# BCN New APIs 路径版本顺序设计

## 决策

`new_apis` 的版本段统一放在 API 类型之后、BCN 命名空间之前：

- OpenAPI：`/openapi/v1/bcn/...`
- Internal API：`/api/v1/bcn/...`

不再使用 `/openapi/bcn/v1/...` 或 `/api/bcn/v1/...`。

## 范围

- 修改全部 35 条 OpenAPI path。
- 修改全部 6 条 Internal API path。
- 同步修改 `serve_api_docs.py` 的路径前缀校验。
- 同步修改 `new_apis/README.md` 中的路径规范。
- 不修改历史 API 文档、当前 Rust 路由实现或数据库。

## 兼容性

这是目标态 API 文档的路径规范调整。`new_apis` 尚未替换现网接口，因此不在目标文档中同时保留旧路径或重定向定义。实际落地时如需兼容，应由 HTTP Gateway 或运行时单独提供迁移期重定向。

## 验证

- `new_apis` 中不存在旧的 `/openapi/bcn/v1` 和 `/api/bcn/v1`。
- 所有 OpenAPI operation 都以 `/openapi/v1/bcn/` 开头。
- 所有 Internal API operation 都以 `/api/v1/bcn/` 开头。
- All、OpenAPI、Internal API 和 Domain Models 四类文档 bundle 均可成功构建。
