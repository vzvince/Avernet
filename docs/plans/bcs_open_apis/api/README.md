# BCN API 文档

更新时间：2026-07-07

本目录为 BCS/BCN 按业务分类拆分 OpenAPI 3.1 文档片段，并同时维护对外 OpenAPI 与内部 Internal API。

## 文件结构

| 文件 | 分类 | 说明 |
| --- | --- | --- |
| `_shared.yaml` | 共享组件 | 统一 `Envelope`、错误响应、鉴权方案、公共参数与基础模型，供 `openapi/` 和 `internalapi/` 共用 |
| `openapi/realtime-messaging.yaml` | 消息会话 | 前端 Workbench WebSocket 连接与帧协议 |
| `openapi/bot-management.yaml` | Bot 管理 | Bot 目录查询、好友关系、Actor 目录 |
| `openapi/provider-management.yaml` | Provider 管理 | Provider 配置、Provider Bot binding |
| `openapi/provider-callbacks.yaml` | Provider 回调 | Provider 上报 Bot runtime 事件 |
| `openapi/group-collaboration.yaml` | 群组管理 | 群组 CRUD、成员、可见性、协作模板 |
| `openapi/sessions.yaml` | Session 相关 | Session CRUD、成员、消息、service invocation |
| `openapi/invitations.yaml` | 邀请 | Group/Session 邀请链接与加入 |
| `internalapi/*.yaml` | Internal API | BCN/BCS 内部接口，路径统一使用 `/api/bcn/v1/...` |
| `serve_api_docs.py` | 本地预览 | 启动 Swagger UI 预览 server，并动态合并拆分后的 OpenAPI/Internal API YAML |

## 路径与接口范围

- `openapi/` 只生成目标态外部 OpenAPI 文档，路径统一使用 `/openapi/bcn/v1/...`。
- `internalapi/` 只生成目标态内部 API 文档，路径统一使用 `/api/bcn/v1/...`。
- 当前打标为 `deprecated` 的接口不进入 `openapi/` 或 `internalapi/` YAML；仍在 `src/bcs/docs/bcs-interface-catalog.md` 中治理。
- 所有 REST 响应统一使用 `Envelope`：`{code, message, data, request_id}`。错误码定义见 `src/bcs/docs/bcn-response-error-spec.md`。

## 本地 Swagger 预览

从仓库根目录启动：

```bash
python3 src/bcs/docs/api/serve_api_docs.py
```

然后打开 `http://127.0.0.1:8765/`。页面右上角可在 `BCN OpenAPI` 与 `BCN Internal API` 之间切换。

可用端点：

| Path | 说明 |
| --- | --- |
| `/` | Swagger UI 页面 |
| `/openapi.yaml` | 合并后的 public OpenAPI YAML |
| `/internalapi.yaml` | 合并后的 Internal API YAML |
| `/openapi.json` | 合并后的 public OpenAPI JSON |
| `/internalapi.json` | 合并后的 Internal API JSON |
| `/healthz` | 文档 server 探活 |

只校验合并结果、不启动 server：

```bash
python3 src/bcs/docs/api/serve_api_docs.py --check
```

## 生成原则

- 一个业务大类一个 YAML，便于后续独立评审和发布。
- 共享 schema 和错误响应集中在 `api/_shared.yaml`，业务 YAML 只定义路径和少量领域模型。
- 与当前代码存在差异时，YAML 描述目标态 wire contract；当前实现路径和接口治理状态仍以 `src/bcs/docs/bcs-interface-catalog.md` 为准。
