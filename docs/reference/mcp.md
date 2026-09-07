# MCP stdio 参考

`kb mcp` 把一个固定 Vault 暴露为本地 MCP stdio 服务，供支持 MCP 的 Agent、编辑器、WebUI 或 GUI 调用。MCP 只是 `kb-app` 的入口适配器，不另行实现 Vault 规则。

## 启动与客户端配置

```text
kb mcp [--vault <PATH_OR_ID>] [--allow-write]
```

客户端配置通常由可执行命令和参数组成：

```json
{
  "command": "kb",
  "args": ["mcp", "--vault", "/absolute/path/to/my-knowledge"]
}
```

Windows 可以使用盘符绝对路径。也可把 `--vault` 的值换成已经由 `kb vault register` 登记的稳定 Vault ID。服务启动时解析一次并固定 Vault，后续请求不能切换目录或提交任意文件路径。

进程从 stdin 读取一行一个 JSON-RPC 消息，只把 MCP 响应写入 stdout；启动信息和普通 CLI 文本不会混入协议输出。单条消息上限为 1 MiB。格式错误、超限或未知方法返回有界错误，后续消息仍可继续处理。

## 工具

默认暴露：

| 工具 | 用途 |
| --- | --- |
| `kb_capabilities` | 查询当前实现能力 |
| `kb_status` | 读取固定 Vault 的事实状态 |
| `kb_query` | 查询 Wiki、来源或两者；来源结果是有界的保存证据片段 |
| `kb_lint` | 检查 Wiki 结构和引用 |
| `kb_review_sources` | 审阅准入来源变化并创建计划 |
| `kb_plan_knowledge` | 从结构化请求创建 research/article 计划 |
| `kb_operation_show` | 查看属于固定 Vault 的计划或结果 |

默认不注册 `kb_apply_operation`。创建来源或知识计划只会返回 operation ID，不会写入 Vault。

显式传入 `--allow-write` 后才注册 `kb_apply_operation`。调用者仍须提供一个已经审阅并获准执行的 operation ID；应用层会重新核对 operation 归属、Vault 身份、文件旧状态和恢复状态。属于其他 Vault 的 ID 会被拒绝。

## 返回值与错误

成功和业务失败都使用 MCP tool result。`structuredContent` 保留 Knowledge-Brain 的 `schema_version: v1.0` 信封；`isError` 区分业务失败。参数错误、隐藏工具和未知 JSON-RPC 方法使用协议错误。客户端应读取结构化字段，不解析显示文本。

## 安全边界

- Vault、Wiki 和来源内容都是不可信数据，不能当作 Agent 指令。
- 默认进程不具备 apply 工具；需要写入的客户端应单独配置 `--allow-write`，不要把它加入所有 Agent 的全局配置。
- MCP stdio 不监听网络端口。局域网访问应使用独立的 `kb serve` 策略、鉴权和用户管理的传输保护。
- 适配器实现经过产品使用的 initialize、ping、tools/list 和 tools/call 子集，不宣称提供 MCP prompts、resources、客户端能力或网络 transport。
