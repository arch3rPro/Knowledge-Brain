# 0012 — 共享 lint 报告与入口退出策略

- Status: accepted / implemented
- Class: architecture

## Problem

CLI、未来 MCP、HTTP、WebUI 和 GUI 都需要相同的知识结构检查。如果应用层返回“严格模式是否失败”或各入口自行检查文件，不同入口会产生不同规则、状态和错误语义。

## Decision

`kb-app` 读取 Vault 并返回带稳定 finding code 的 `LintReport`。所有入口消费同一报告，不重新实现检查。

`--strict` 是 CLI 的进程策略：报告含任何 finding 时使用非零退出码，但不改变检查集合、严重度或 JSON 数据。应用层请求不包含 strict 字段。

## Alternatives considered

**strict 进入应用层。** 这会把进程退出语义泄漏到 GUI、HTTP 和 MCP，并使相同 Vault 因调用参数不同而得到不同报告。

**每个入口自行 lint。** 规则会重复并逐步漂移，也无法保证未来 WebUI/GUI 与 CLI 看到同一事实。

**发现 error 时总是让命令失败。** lint 的主要产物正是完整报告；在首个发现处转为命令错误会妨碍交互式查看和批量修复。自动化可以显式选择 `--strict`。

## Consequences

CLI、未来 MCP、HTTP、WebUI 和 GUI 共享字段、严重度和 finding code。CLI 自动化可以选择失败退出，而交互使用可以一次读取完整报告。

只检查默认退出码的脚本不会把 findings 当作失败；命令帮助和参考文档明确要求自动化使用 `--strict` 或读取报告。

