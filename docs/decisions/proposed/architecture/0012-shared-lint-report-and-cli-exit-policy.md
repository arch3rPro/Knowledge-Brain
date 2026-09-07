# 0012 — 共享 lint 报告与入口退出策略

- Status: proposed
- Class: architecture

## Problem

CLI、未来 MCP、HTTP、WebUI 和 GUI 都需要相同的知识结构检查。如果应用层返回“严格模式是否失败”或各入口自行检查文件，不同入口会产生不同规则、状态和错误语义。

## Proposal

`kb-app` 负责读取 Vault 并返回带稳定 finding code 的 `LintReport`。所有入口消费同一报告，不重新实现检查。

`--strict` 是 CLI 的进程策略：报告含任何 finding 时使用非零退出码，但不改变检查集合、严重度或 JSON 数据。应用层请求不包含 strict 字段。

## Alternatives considered

**strict 进入应用层。** 这会把进程退出语义泄漏到 GUI、HTTP 和 MCP，并使相同 Vault 因调用参数不同而得到不同报告。

**每个入口自行 lint。** 规则会重复并逐步漂移，也无法保证未来 WebUI/GUI 与 CLI 看到同一事实。

**发现 error 时总是让命令失败。** lint 的主要产物正是完整报告；在首个发现处转为命令错误会妨碍交互式查看和批量修复。自动化可显式选择 `--strict`。

## Acceptance criteria

- `LintReport` 从 `kb-app` 导出并可序列化。
- CLI 默认和 `--strict` 返回相同报告。
- 默认模式有 findings 时退出 0；strict 模式有 findings 时退出非零。
- lint 不写入 Vault 或用户状态。

## Risks

脚本如果忘记使用 `--strict`，只检查退出码会漏掉发现。命令参考和帮助必须明确这一点。

