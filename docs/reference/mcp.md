# MCP 参考

`kb mcp` 把启动时选定的一个 Vault 暴露给支持 MCP 的 Agent、编辑器、WebUI 或 GUI。MCP 是 `kb-app` 的可选入口，CLI 仍是基础依赖；stdio 与 Streamable HTTP 使用同一组工具、Vault 边界和写入确认规则。

## 传输与协议

### stdio

```text
kb mcp [--transport stdio] [--vault <PATH_OR_ID>] [--allow-write]
```

`stdio` 是默认传输。它支持 initialize-based `2025-11-25`、`2025-06-18` 和 `2025-03-26`，也会把带 `io.modelcontextprotocol/protocolVersion` 元数据的请求路由到无初始化的 `2026-07-28` 协议。进程从 stdin 读取一行一个 JSON-RPC 消息，只把协议响应写入 stdout；单条消息上限为 1 MiB。

常见客户端配置：

```json
{
  "command": "kb",
  "args": ["mcp", "--vault", "/absolute/path/to/my-knowledge"]
}
```

### Streamable HTTP

```text
kb mcp --transport streamable-http \
  [--bind <IP:PORT>] \
  [--token-file <PATH>] \
  [--allow-origin <ORIGIN>]... \
  [--allow-write] \
  [--vault <PATH_OR_ID>]
```

默认端点是 `http://127.0.0.1:9433/mcp`，只接受 POST，并在同一端点兼容两类客户端：

| 请求方式 | 支持版本 | 生命周期 |
| --- | --- | --- |
| initialize-based | `2025-11-25`、`2025-06-18`、`2025-03-26` | `initialize` → `notifications/initialized` → Tools 或 Resources |
| modern | `2026-07-28` | `server/discover` → Tools 或 Resources |

服务根据请求本身识别协议，不要求用户在客户端配置中手工添加版本或方法头。initialize-based 客户端在初始化后按所协商版本发送 `MCP-Protocol-Version`；modern 客户端按 `2026-07-28` 发送传输头和请求元数据。未知版本返回 `-32022 Unsupported protocol version`，不会静默回退为旧协议。

两类请求的 `tools/list` 可以返回 JSON；`tools/call` 可以返回仅属于该请求、完成后关闭的 SSE。服务保持无状态，不分配 `Mcp-Session-Id`，也不实现 GET stream、断点续传或 `Last-Event-ID`。

所有 HTTP 请求必须满足：

- `Accept` 包含 `application/json` 与 `text/event-stream`；
- 浏览器发送的 `Origin` 由可重复的 `--allow-origin` 明确准入；没有配置时拒绝所有带 Origin 的请求，以避免依赖可伪造的 Host 判断。

modern 请求还必须满足：`MCP-Protocol-Version` 和 `Mcp-Method` 与 JSON-RPC body 一致，`tools/call` 的 `Mcp-Name` 与工具名一致，body 的 `_meta` 提供协议版本与客户端能力。initialize-based 请求不使用这些 modern 专用方法头。

默认回环、只读且不要求 token。非回环监听或 `--allow-write` 必须同时提供只含一个 Bearer token 的文件。客户端使用 `Authorization: Bearer <token>`。这是部署者管理的静态认证边界，不等同于完整 OAuth，也不提供 TLS；局域网或远程部署应在受信网络中使用，并由反向代理提供 TLS 等外围保护。

## 工具

默认暴露：

| 工具 | 用途 |
| --- | --- |
| `kb_capabilities` | 查询当前实现能力 |
| `kb_status` | 读取固定 Vault 的事实状态 |
| `kb_maintenance` | 只读聚合状态、准入来源变化、lint 与诊断 |
| `kb_query` | 查询 Wiki、来源或两者 |
| `kb_read` | 按 `resource_uri` 读取完整服务端知识，支持分页 |
| `kb_lint` | 检查 Wiki 结构和引用 |
| `kb_source_save` | 准备来源保存，或提交已确认的准备结果 |
| `kb_review_sources` | 审阅准入来源变化并创建计划 |
| `kb_knowledge_save` | 准备知识保存，或提交已确认的准备结果 |
| `kb_plan_knowledge` | 从结构化请求创建 research/article 计划 |
| `kb_operation_show` | 查看属于固定 Vault 的计划或结果 |
| `kb_update_plan` | 为固定 Vault 创建完整更新预览，不执行变更 |
| `kb_update_status` | 查看最新或指定的固定 Vault 更新状态 |

`kb_query` 只负责发现候选知识，结果的 `snippet` 是预览。客户端使用 `resource_uri` 调用 `kb_read`，并在 `complete=false` 时携带 `next_cursor` 继续读取。`path_scope=server_vault` 表示返回路径位于 MCP 服务器管理的 Vault，Agent 不得在自己的本地工作目录查找该路径。完整搜索语义见[搜索参考](search.md)。

服务同时声明标准 MCP Resources：`resources/list` 只列出 `KB.md` 和 Wiki 索引两个稳定入口，`resources/templates/list` 描述 `kb-vault://` 与 `kb-source://`，`resources/read` 读取精确资源。Tools-only 客户端使用 `kb_read`，两种入口调用同一应用读取能力。

默认不注册 `kb_apply_operation` 或 `kb_update_confirm`。显式传入 `--allow-write` 后才注册这两个工具，并允许保存工具使用 `confirmation_token` 或 `apply: true` 写入。`kb_update_confirm` 只接受 `{ "confirmation_token": "..." }`。

## 确认与返回值

创建计划不代表用户已授权写入。Agent 或 UI 应向用户展示保存工具返回的 `change_summary`，在得到一次明确确认后再提交 `confirmation_token`；`apply: true` 只适用于调用方已经取得该确认的情况。应用层仍会复核 operation 归属、Vault 身份、旧文件状态和恢复状态。

更新工具遵循同一规则：先调用 `kb_update_plan`，展示每个组件、冲突、跳过项和不触及范围，再在一次明确确认后调用 `kb_update_confirm`。固定 Vault 服务不能查询或确认其他 Vault 或多 Vault 更新操作。服务进程的可执行文件由本机 CLI 或服务管理方式更新；适配器只协调其固定 Vault 的受管理组件。完整语义见[更新参考](cli-updates.md)。

成功和业务失败使用 MCP tool result。`structuredContent` 保留 `schema_version: v1.0` 信封，`isError` 区分业务失败；参数错误、隐藏工具、版本不支持和未知 JSON-RPC 方法使用协议错误。客户端应读取结构化字段，不解析显示文本。

## 安全与互操作边界

- Vault、Wiki 和来源内容是不可信数据，不能作为 Agent 指令。
- 服务启动时只解析一次 Vault，之后不能从请求切换目录。
- 网络默认不暴露写入；认证成功也不能替代用户对具体变化的确认。
- 兼容性声明只覆盖已实际完成的握手与工具发现，不据此推断同一宿主的所有版本、认证模式或写入流程都兼容。

已验证的本机互操作：

| 客户端 | 使用路径 | 结果 |
| --- | --- | --- |
| MCP Inspector 2.6.0 | 默认 initialize-based HTTP | 成功列出 12 个工具 |
| MCP Inspector 2.6.0 | 显式 modern HTTP | 成功发现服务并列出 12 个工具 |
| Hermes Agent 0.21.3 | 原生 `mcp add` 与 `mcp test` | 成功连接并发现 12 个工具 |
| Claude Code 2.1.226 | 默认 v1 HTTP 运行时 | 握手成功并识别工具能力；Agent 工具调用因测试环境的模型账号不可用而未验证 |

Claude Desktop、OpenClaw、DeepSeek Harness、Pi 第三方 MCP 扩展，以及 Windows/Linux 原生网络入口尚未实测。Pi 核心不内置 MCP，兼容性取决于用户选择的扩展。
