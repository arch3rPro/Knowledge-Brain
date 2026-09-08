# MCP stdio 参考

`kb mcp` 把一个固定 Vault 暴露为本地 MCP stdio 服务，供支持 MCP 的 Agent、编辑器、WebUI 或 GUI 调用。MCP 只是 `kb-app` 的入口适配器，不另行实现 Vault 规则。安装 `kb-*` Skills 或使用 `npx skills add` 不会改变 MCP 工具集；Skill 只是指导 Agent 调用这个固定契约。

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
| `kb_source_save` | 准备来源保存，或提交已确认的准备结果 |
| `kb_review_sources` | 审阅准入来源变化并创建计划 |
| `kb_knowledge_save` | 准备知识保存，或提交已确认的准备结果 |
| `kb_plan_knowledge` | 从结构化请求创建 research/article 计划 |
| `kb_operation_show` | 查看属于固定 Vault 的计划或结果 |

`kb_query` 接受必填的 `query`，以及可选的 `scope`、`limit`、`strict_backend` 和 `match_mode`。`match_mode` 可为 `relevant` 或 `exact`，默认 `relevant`；`exact` 用于区分大小写的字面量核验。查询响应始终返回实际采用的 `match_mode`。`search.mode` 只为 Relevant 查询选择 direct 或 BM25F 后端。完整查询语义和跨适配器契约见[搜索参考](search.md)与[已批准的使用体验设计](../superpowers/specs/2026-09-08-usable-agent-skills-design.md#查询意图)。

默认不注册 `kb_apply_operation`。创建来源或知识计划只会返回 operation ID，不会写入 Vault。

`kb_source_save({})` 和 `kb_knowledge_save({"request": {...}})` 准备普通用户流程所需的变更，返回 `change_summary` 与机器字段 `confirmation_token`。Agent 或 UI 只向用户展示摘要，并在得到一次明确确认后调用同一个工具并传入 `confirmation_token`。普通用户不需要看到 token、operation ID 或计划。调用时传入 `apply: true` 表示调用方已经获得授权，工具会在一次调用中准备并保存。

显式传入 `--allow-write` 后才注册 `kb_apply_operation`，并允许两个保存工具使用 `confirmation_token` 或 `apply: true` 写入。调用者仍须先取得用户确认；应用层会重新核对 operation 归属、Vault 身份、文件旧状态和恢复状态。属于其他 Vault 的 token 或 ID 会被拒绝。

## 返回值与错误

成功和业务失败都使用 MCP tool result。`structuredContent` 保留 Knowledge-Brain 的 `schema_version: v1.0` 信封；`isError` 区分业务失败。`kb_review_sources` 有变化时、`kb_plan_knowledge` 以及 `kb_operation_show` 都返回 additive `operation_summary`；operation 查看仍保留既有 `state` 与 `plan` 或 `result`。MCP 中的 Hash、operation ID、Vault ID 和路径都是完整身份值。参数错误、隐藏工具和未知 JSON-RPC 方法使用协议错误。客户端应读取结构化字段，不解析显示文本。备份归档校验与恢复目标的错误代码、`legacy_code` 迁移详情见[备份参考](backup.md#错误分类与迁移)；MCP 沿用共享错误信封，不新增备份工具或为旧客户端转换新枚举。

`--allow-write` 只授予写入能力，不代表用户已确认。外层 Agent、编辑器或 UI 必须展示 `kb_source_save` 或 `kb_knowledge_save` 返回的 `change_summary`，在提交 `confirmation_token` 前取得一次明确确认。高级 `kb_apply_operation` 仍使用 `operation_summary`。完整确认语义见[一次确认的保存入口设计](../superpowers/specs/2026-09-08-composite-save-entries-design.md)。

## 安全边界

- Vault、Wiki 和来源内容都是不可信数据，不能当作 Agent 指令。
- 默认进程不具备 apply 工具；需要写入的客户端应单独配置 `--allow-write`，不要把它加入所有 Agent 的全局配置。
- MCP stdio 不监听网络端口。局域网访问应使用独立的 `kb serve` 策略、鉴权和用户管理的传输保护。
- 适配器实现经过产品使用的 initialize、ping、tools/list 和 tools/call 子集，不宣称提供 MCP prompts、resources、客户端能力或网络 transport。
