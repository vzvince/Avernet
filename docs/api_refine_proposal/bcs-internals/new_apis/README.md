# BCN New APIs

本目录描述 BCN 已确认的目标态领域对象和 API，不修改或替代 `../api` 中的历史接口文档。

## 目录

| 文件 | 作用 |
|---|---|
| `domain-models.yaml` | 领域对象的唯一 OpenAPI Schema 来源，可在 Swagger 中单独查看 |
| `_shared.yaml` | Envelope、鉴权方案、分页、公共参数和通用错误 |
| `openapi/*.yaml` | 面向产品、Bot、Provider 和外部集成的 OpenAPI |
| `internalapi/*.yaml` | 面向受信任内部服务和运维工具的 Internal API |
| `serve_api_docs.py` | 校验、合并并通过 Swagger UI 展示文档 |

## API 范围

- OpenAPI 路径统一为 `/openapi/v1/bcn/...`；
- Internal API 路径统一为 `/api/v1/bcn/...`；
- 所有 REST 响应统一使用 `{code, message, data, request_id}`；
- `code` 固定为五位整数，前三位等于 HTTP status，后两位为细分码；成功响应使用
  `20000`、`20100`、`20200`；
- JSON 字段使用 `snake_case`，时间字段使用 Unix milliseconds；
- 每个 operation 的 `security` 描述认证入口，`description` 继续描述资源级授权条件；
- 必须始终提供的字段列入 Schema 的 `required`；没有列入 `required` 的字段为可选字段，其缺省语义写在 `description`；
- 条件输入和条件输出使用 `oneOf`，并在 operation 的响应说明中给出输入与输出的对应关系。

每个 operation 必须通过 `x-bcn-error-codes` 为所有非 2xx response 声明具体错误。每项包含
`code`、稳定且安全的 `message`，以及只用于文档的 `condition`。文档构建器会校验 HTTP
前缀和全局 code-message 一致性，并在 Swagger 中生成对应的严格 Schema 和示例。

## Swagger 视图

```bash
python3 src/bcs-internal/docs/new_apis/serve_api_docs.py --check
python3 src/bcs-internal/docs/new_apis/serve_api_docs.py
```

启动后访问 `http://127.0.0.1:8766/`。默认页面为 `BCN All APIs`，下拉框支持：

- `BCN All APIs`：同时查看 OpenAPI 和 Internal API；
- `BCN OpenAPI`：只查看 OpenAPI；
- `BCN Internal API`：只查看 Internal API；
- `BCN Domain Models`：只查看领域对象。该视图优先展示 `Actor`、`Group`、`Session`，
  其次展示 `Provider` 等次级对象；枚举和从属结构内联到引用它们的对象中，不再单独占据模型列表。
  `Message` 作为 Session API 使用的从属 Schema 保留在权威定义中，但不在该页面单独展示。

直接访问 `http://127.0.0.1:8766/domain-models/` 可打开领域对象页面。

`domain-models.yaml` 仍是包含完整领域 Schema 的权威来源，OpenAPI、Internal API 和 All APIs
合并文档也继续使用完整 Schema；上述收敛仅影响独立的 `BCN Domain Models` 展示视图。

## 鉴权约定

| 名称 | 用途 |
|---|---|
| `humanCookie` | 当前登录 Human，服务端从 Cookie 解析 `staff_no` |
| `botRuntimeBearer` | Bot Runtime Token，必须解析为唯一 BotActor |
| `agentPassBearer` | AgentPass 身份，最终必须映射为唯一 BotActor |
| `providerAdminBearer` | Provider 管理凭证，只能管理 token 所属 Provider |
| `registrationBearer` | 个人 Bot 注册令牌，只允许调用 Bot Registration |
| `serviceKey` | Service Invocation 凭证，必须绑定目标 Group |
| `internalServiceBearer` | 受信任内部服务或运维身份 |

`security` 中的多个对象表示“任一方式均可”，同一对象中的多个 scheme 表示“必须同时满足”。
