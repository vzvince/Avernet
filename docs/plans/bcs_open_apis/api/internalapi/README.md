# BCN Internal API

更新时间：2026-07-06

本目录存放 BCN/BCS 内部 API 的 OpenAPI 3.1 文档片段。内部 API 与外部 OpenAPI 使用同一套公共规范：

- 共享组件统一引用 `../_shared.yaml`
- REST JSON 响应统一使用 `Envelope`：`{code, message, data, request_id}`
- 错误响应统一引用 `../_shared.yaml#/components/responses/...`
- 废弃接口不进入本目录，继续在 `src/bcs/docs/bcs-interface-catalog.md` 中治理
- Internal API 路径统一使用 `/api/bcn/v1/...`

## 文件结构

| 文件 | 分类 |
| --- | --- |
| `system-ops.yaml` | 系统/运维 |
| `identity-access.yaml` | 身份与接入 |
| `bot-runtime.yaml` | Bot Runtime、Bot 内部生命周期、Bot A2A |
| `provider-internal.yaml` | Provider 内部 |
| `group-internal.yaml` | 群组内部控制 |
| `state-machine.yaml` | 状态机运行 |
| `oauth.yaml` | OAuth |
