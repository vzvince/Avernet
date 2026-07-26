# BCN New APIs 错误码规范设计

## 目标

`src/bcs-internal/docs/new_apis` 下的全部 OpenAPI 和 Internal API 使用统一的五位业务码，并明确描述每一种输入或业务条件对应的 HTTP Status、业务码和安全错误消息。

## 编码规则

- 业务码固定为五位整数：前三位等于 HTTP Status，后两位为该状态下的细分码。
- `xx00` 表示该 HTTP Status 的通用结果；`xx01` 至 `xx99` 表示可稳定识别的具体原因。
- 成功响应使用 `20000`、`20100`、`20200`。
- 同一个业务码在所有 API 中只能对应同一个默认 `message`。
- `message` 必须具体、稳定并可安全返回，不包含内部堆栈、凭证或会导致资源枚举的信息。
- 错误响应的 `data` 固定为 `null`，`request_id` 与 `X-Trace-Id` 一致。

## 源文档表达

每个 operation 使用 `x-bcn-error-codes` 声明自身可能返回的错误：

```yaml
x-bcn-error-codes:
  "403":
    - code: 40301
      message: caller is not allowed to act as the specified actor
      condition: 调用身份无权代表 acting_actor_id。
```

`condition` 只用于说明触发条件，不进入响应体。每一个已声明的非 2xx response 都必须在 `x-bcn-error-codes` 中至少有一项，扩展中也不能声明 operation 没有的 HTTP Status。

## Swagger 生成

`serve_api_docs.py` 在合并文档时：

1. 校验业务码格式、HTTP 前缀、message、condition 和全局 code-message 一致性。
2. 将共享错误 Response 展开为当前 operation 的具体响应。
3. 单个具体错误使用带 `const code`、`const message` 的 Schema；多个错误使用 `oneOf`。
4. 为每个错误生成可直接查看的 JSON example。
5. 将成功响应的 `code` 和 `message`约束为对应的 `20000/20100/20200` 与 `OK/Created/Accepted`。

这样源 YAML 保持紧凑，生成后的 All、OpenAPI 和 Internal API Swagger 页面仍能直接看到具体错误。

## 兼容边界

- 本次修改的是目标 API 规范和文档构建器，不修改现有 BCS Rust HTTP 实现。
- 错误码代表目标态契约；实际接口迁移时必须按该契约映射现有错误。
- `404` 可以继续用于同时隐藏“资源不存在”和“无权查看”，避免身份或资源枚举。

## 验证

- 所有 52 个 operation 的非 2xx response 都具有错误码声明。
- 所有业务码都是五位，且前三位与 HTTP Status 一致。
- 同一码值不存在不同 message。
- 生成后的错误响应具有具体 Schema 和 example。
- 四类 Swagger bundle 均能成功构建。
