# MCP 参考

`kb mcp` 把启动时选定的一个 Vault 暴露给支持 MCP 的 Agent、编辑器、WebUI 或 GUI。MCP 是 `kb-app` 的可选入口，CLI 仍是基础依赖；stdio 与 Streamable HTTP 使用同一组工具、Vault 边界和写入确认规则。

## 传输与协议

### stdio

```text
kb mcp [--transport stdio] [--vault <PATH_OR_ID>] [--allow-write]
```

`stdio` 是默认传输。它兼容现有 initialize-based `2025-06-18` 请求，也会把带 `io.modelcontextprotocol/protocolVersion` 元数据的请求路由到无初始化的 `2026-07-28` 协议。进程从 stdin 读取一行一个 JSON-RPC 消息，只把协议响应写入 stdout；单条消息上限为 1 MiB。

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

默认端点是 `http://127.0.0.1:9433/mcp`，只接受 POST，使用现代 `2026-07-28` 协议。`server/discover` 和 `tools/list` 返回 JSON；`tools/call` 可以返回仅属于该请求、完成后关闭的 SSE。该版本不使用 initialize、GET stream、协议 session、`Mcp-Session-Id`、断点续传或 `Last-Event-ID`。

请求必须同时满足：

- `Accept` 包含 `application/json` 与 `text/event-stream`；
- `MCP-Protocol-Version` 和 `Mcp-Method` 与 JSON-RPC body 一致；
- `tools/call` 的 `Mcp-Name` 与工具名一致；
- body 的 `_meta` 提供协议版本与客户端能力；
- 浏览器发送的 `Origin` 由可重复的 `--allow-origin` 明确准入；没有配置时拒绝所有带 Origin 的请求，以避免依赖可伪造的 Host 判断。

默认回环、只读且不要求 token。非回环监听或 `--allow-write` 必须同时提供只含一个 Bearer token 的文件。客户端使用 `Authorization: Bearer <token>`。这是部署者管理的静态认证边界，不等同于完整 OAuth，也不提供 TLS；局域网或远程部署应在受信网络中使用，并由反向代理提供 TLS 等外围保护。

## 工具

默认暴露：

| 工具 | 用途 |
| --- | --- |
| `kb_capabilities` | 查询当前实现能力 |
| `kb_status` | 读取固定 Vault 的事实状态 |
| `kb_maintenance` | 只读聚合状态、准入来源变化、lint 与诊断 |
| `kb_query` | 查询 Wiki、来源或两者 |
| `kb_lint` | 检查 Wiki 结构和引用 |
| `kb_source_save` | 准备来源保存，或提交已确认的准备结果 |
| `kb_review_sources` | 审阅准入来源变化并创建计划 |
| `kb_knowledge_save` | 准备知识保存，或提交已确认的准备结果 |
| `kb_plan_knowledge` | 从结构化请求创建 research/article 计划 |
| `kb_operation_show` | 查看属于固定 Vault 的计划或结果 |

`kb_query` 的完整参数与跨入口语义见[搜索参考](search.md)。默认不注册 `kb_apply_operation`。显式传入 `--allow-write` 后才注册该工具，并允许保存工具使用 `confirmation_token` 或 `apply: true` 写入。

## 确认与返回值

创建计划不代表用户已授权写入。Agent 或 UI 应向用户展示保存工具返回的 `change_summary`，在得到一次明确确认后再提交 `confirmation_token`；`apply: true` 只适用于调用方已经取得该确认的情况。应用层仍会复核 operation 归属、Vault 身份、旧文件状态和恢复状态。

成功和业务失败使用 MCP tool result。`structuredContent` 保留 `schema_version: v1.0` 信封，`isError` 区分业务失败；参数错误、隐藏工具、版本不支持和未知 JSON-RPC 方法使用协议错误。客户端应读取结构化字段，不解析显示文本。

## 安全与互操作边界

- Vault、Wiki 和来源内容是不可信数据，不能作为 Agent 指令。
- 服务启动时只解析一次 Vault，之后不能从请求切换目录。
- 网络默认不暴露写入；认证成功也不能替代用户对具体变化的确认。
- 现代协议和 HTTP 已通过本地真实 CLI/TCP 请求验证；官方 Inspector、第三方远程客户端以及 Windows/Linux 原生网络入口仍需发布前互操作验证，当前不据此宣称所有 MCP 客户端均兼容。
