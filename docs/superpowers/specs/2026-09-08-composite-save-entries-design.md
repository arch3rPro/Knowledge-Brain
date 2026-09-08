# 组合保存入口设计

- Status: draft for review
- Date: 2026-09-08
- Decision: [ADR-0018](../../decisions/proposed/architecture/0018-composite-save-coordination.md)

## 目标

为两类常见写入任务提供短而明确的入口，同时保留可组合的底层命令：

- 保存已准入主题目录中变更的来源材料；
- 保存经过结构化请求描述的 Wiki 知识变更。

入口必须在 CLI、MCP、HTTP 与未来 WebUI/GUI 之间复用相同的应用层语义。它不取代既有 `review`、`plan create`、`operation show` 或 `apply`，也不新增持久化 operation 类型。

## 用户接口

### CLI

```text
kb source save [--yes] [--vault <path-or-id>] [--json]
kb knowledge save <REQUEST.json> [--yes] [--vault <path-or-id>] [--json]
```

`kb source save` 只巡检 `admission.yml` 中启用的目录，创建既有的 `capture_sources` 计划。`kb knowledge save` 使用与 `kb plan create` 相同的 JSON 请求格式，创建既有的 `save_knowledge` 计划。

不带 `--yes` 时，两条命令只创建预览和计划；它们不等待终端输入。带 `--yes` 时，CLI 将这次调用视为用户针对该操作的一次明确写入授权：先生成计划，再仅提交这次生成的 operation。没有变化的来源保存返回 `unchanged`，不创建或提交 operation。

`kb review`、`kb plan create`、`kb operation show` 和 `kb apply <operation-id>` 保持原有语义。需要在一次调用前后插入人工检查、外部审批或自定义流程的用户继续使用这些原语。

### MCP

新增两个工具：

```text
kb_source_save({ "apply": false })
kb_knowledge_save({ "request": { ... }, "apply": false })
```

`apply` 缺省为 `false`。当 MCP 未使用 `kb mcp --allow-write` 启动时，工具仍可生成预览，但 `apply: true` 返回稳定的授权错误，且不创建或提交计划。启用 `--allow-write` 后，`apply: true` 仍是调用方已经取得最终用户授权后的明确写入请求；MCP 不把工具调用本身解释为用户同意。

### HTTP

新增固定 Vault 路由：

```text
POST /source/save
{ "apply": false }

POST /knowledge/save
{ "request": { ... }, "apply": false }
```

`apply` 缺省为 `false`。HTTP 服务没有 `--allow-write` 时，`apply: true` 返回 `403`，且不生成计划。启用写入还必须遵守现有固定 Vault、HTTP Bearer token 和非 loopback 监听限制。`/review`、`/plans` 与 `/operations/{id}/apply` 不移除。

## 共享应用层合同

`kb-app` 新增对应的 source save 和 knowledge save request，并以同一协调函数执行以下流程：

```text
选择 Vault
    ↓
调用既有 review 或 plan-create 路径
    ↓
无变化 → unchanged 响应
有计划且未请求执行 → planned 响应
有计划且明确请求执行 → 对该 operation 调用既有 selected-Vault apply 路径 → applied 响应
```

协调函数不得复制来源捕获、知识计划、提交、恢复、索引更新或 operation 写入逻辑。它在创建计划后，从该计划中取得完整 operation ID，并用该 ID 调用现有 selected-Vault apply 路径。因此锁、提交前状态检查、陈旧计划检测和恢复语义仍由已有实现负责。

新的组合响应仅用于新入口，稳定结构如下：

```json
{
  "phase": "planned | applied | unchanged",
  "preview": { "...": "既有 review 或 plan-create 的计划字段" },
  "operation_summary": { "...": "当前 operation 摘要" },
  "result": null
}
```

`planned` 的 `operation_summary` 表示计划状态；`applied` 的 `operation_summary` 表示完成状态，并且 `result` 是既有 apply 结果；`unchanged` 的 `operation_summary` 与 `result` 都为 `null`。`preview` 始终说明本次检查或生成的内容，不能被 apply 结果替换。机器可读 Hash、operation ID、Vault ID 和路径继续使用完整规范值。

如果提交阶段失败但计划已经保存，错误保持原有稳定错误码，并在 `error.details.operation_id` 中提供完整 ID。调用方可用 `kb operation show` 或既有 HTTP/MCP operation 接口检查其状态；不得把失败伪装成已提交或静默重新规划。

## 确认、权限与并发

组合入口不增加交互式确认。默认预览为 Agent、UI 或自动化提供展示 `operation_summary` 并获得一次用户同意的材料。CLI 的 `--yes`、MCP 的 `apply: true` 与 HTTP 的 `apply: true` 是不同适配器传递到同一应用层的明确执行标记。

计划和提交沿用既有两个阶段的锁边界。计划完成与提交开始之间，文件可能发生变化；提交必须继续以 operation 内已有的版本检查拒绝陈旧内容。失败后保留可检查的 operation，而不是扩大锁范围或跳过检查。

## 非目标

- 不新增交互式向导、自动确认或后台自动提交；
- 不移除原有 CLI/MCP/HTTP operation 接口；
- 不改变 operation 文件格式、operation kind、Hash 长度或 Vault 文件布局；
- 不实现 WebUI、GUI、PDF/OCR、Embedding 或 rerank；
- 不把 Agent Skill 当作唯一入口。Skill 只在实现后更新为优先使用组合入口。

## 验收与文档归属

实现采用针对性的真实入口测试，不运行无关 workspace 全量测试。至少覆盖：

- CLI 两条命令的默认预览、`--yes` 提交、JSON envelope 与人类输出；
- admitted 目录无变化时 `kb source save` 的 `unchanged` 结果；
- `kb knowledge save` 对请求文件的校验与失败信息；
- MCP 和 HTTP 的默认预览、写入未授权拒绝、启用写入后的提交，以及固定 Vault 隔离；
- 计划在提交前陈旧时的错误、operation ID 可发现性和不发生额外写入；
- 既有 `review`、`plan create` 与 `apply` 回归行为。

本文件拥有组合入口的产品语义与适配器合同。[命令参考](../../reference/commands.md)、MCP/HTTP reference 和各个 `kb-*` Skill 在实现时记录使用方式并链接本文件；[使用体验改进](../../product/usability-backlog.md)只保留问题和状态。
