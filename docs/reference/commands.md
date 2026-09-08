# 命令参考

所有带 `--json` 的命令在 stdout 输出一个 JSON 信封。成功数据位于 `data`，失败数据位于 `error`；业务失败使用非零退出码。JSON 模式不会把提示或诊断混入 stderr。每个信封都携带 `schema_version`。

`--full-hashes` 是全局选项。人类可读输出默认把独立的 64 位 SHA-256 显示为前 12 位；`--full-hashes` 显示完整 SHA-256。路径、operation ID、Vault ID 以及 `--json` 输出始终保留完整身份值。

## Vault 创建与采用

```text
kb init <TARGET> [--json]
kb adopt <TARGET> [--json]
kb apply <OPERATION_ID> [--json]
kb operation show <OPERATION_ID> [--json]
```

`init` 只接受不存在或空目录，创建最小 Vault 并尝试登记。它不创建 Git 仓库。

`adopt` 审查已有目录并把计划保存到用户状态目录，不修改目标。`apply` 按操作 ID 重新验证并执行；同一已完成 ID 再次执行时返回保存的结果，不重复产生影响。

计划创建响应保留各自既有的根字段，并在同一根级增加 `operation_summary`；`operation show` 响应保留 `state` 以及 `plan` 或 `result`，并在同一根级增加 `operation_summary`。`state` 选择持久化的 `plan` 或 `result` 载荷，`operation_summary.operation_state` 反映最新的已验证事件；因此失败后仍保留计划的操作会返回 `state: "planned"` 和 `plan`，同时摘要状态为 `failed`。客户端可以继续读取原字段。可提交的计划摘要以 `requires_confirmation: true` 和 `can_apply: true` 表示；完成或失败的摘要以 `requires_confirmation: false` 和 `can_apply: false` 表示不可提交。直接输入 `kb apply <OPERATION_ID>` 已是 CLI 用户的明确写入请求；由 Agent 或 UI 发起时，外层调用方须展示该摘要并在最终 apply 前取得一次明确确认。完整语义见[已批准的使用体验设计](../superpowers/specs/2026-09-08-usable-agent-skills-design.md#操作摘要与一次确认)。

## 备份与恢复

```text
kb backup create [--output <PATH.zip>] [--without-source-objects] [--vault <PATH_OR_ID>] [--json]
kb backup verify <PATH.zip> [--json]
kb backup restore <PATH.zip> --target <EMPTY_DIRECTORY> [--json]
```

`create` 不覆盖输出文件，也不在 Vault 内创建归档。默认包含全部来源对象；`--without-source-objects` 生成明确标记为不含完整来源证据的较小归档。`verify` 不解压到目标，`restore` 只接受不存在或真实空目录，并在私有临时目录中复核全部字节后放入目标。详细范围、清单和失败边界见[备份参考](backup.md)。

## 知识计划与保存

```text
kb knowledge save <REQUEST_JSON> [--yes | --confirm <TOKEN>] [--vault <PATH_OR_ID>] [--json]
kb plan create <REQUEST_JSON> [--vault <PATH_OR_ID>] [--json]
kb operation show <OPERATION_ID> [--json]
kb apply <OPERATION_ID> [--json]
```

通常的 Agent 或 UI 保存使用 `kb knowledge save`。默认调用只准备变更，返回机器使用的 `confirmation_token` 和面向用户的变更摘要；Agent 或 UI 只展示摘要并取得一次确认。确认后，它以 `--confirm <TOKEN>` 提交刚才准备的同一份内容。用户不需要看到或输入 token、operation ID，也不需要运行 `apply`。已在调用前取得授权的自动化可以使用 `--yes`，在一次调用内准备并保存。

`plan create`、`operation show` 和 `apply` 是高级接口，适用于延后执行、脚本编排或人工逐项审阅。`plan create` 把结构化 research/article 建议转换为可审阅计划，不修改 Vault；`apply` 在独占锁内重新核对并整批保存。请求字段、冲突和恢复语义见[知识计划参考](knowledge-plans.md)与[一次确认的保存入口设计](../superpowers/specs/2026-09-08-composite-save-entries-design.md)。

## 来源与搜索

```text
kb source save [--yes | --confirm <TOKEN>] [--vault <PATH_OR_ID>] [--json]
kb review [--vault <PATH_OR_ID>] [--json]
kb query <QUERY> [--scope wiki|sources|all] [--limit <1..100>] [--exact] [--strict-backend] [--vault <PATH_OR_ID>] [--json]
kb cache rebuild [--vault <PATH_OR_ID>] [--json]
kb source verify [--vault <PATH_OR_ID>] [--json]
kb lint [--strict] [--vault <PATH_OR_ID>] [--json]
```

通常的来源保存使用 `kb source save`。默认调用只检查 `admission.yml` 中启用的目录、准备来源变更，并返回 Agent/UI 保留的 `confirmation_token` 和变更摘要。Agent/UI 向用户展示一次摘要后，以 `--confirm <TOKEN>` 保存同一份来源版本；用户不需要理解 token、operation 或 `apply`。`--yes` 仅用于调用前已经取得授权的一次调用内准备并保存。没有来源变化时结果为 `unchanged`，不会产生 token 或待确认操作。

`review` 只查看启用的准入目录，返回新增、变化、删除及可能移动的来源。有变化时保存用户状态目录中的计划；没有变化时 `operation_id` 为 `null`。它是脚本、延后执行和人工审阅的高级接口；明确执行该 ID 的 `apply` 才保存来源与日志。计划核对及恢复规则见[来源格式](sources.md)与[一次确认的保存入口设计](../superpowers/specs/2026-09-08-composite-save-entries-design.md)。

`query` 默认 `scope=wiki`、`limit=10`，并按相关性发现结果；`--exact` 改为区分大小写的字面量核验。空白查询或越界 limit 返回 `invalid_query`。`all` 固定返回 Wiki、来源两组，limit 分别作用于每组；不生成 LLM 回答。每个响应以 `match_mode` 报告实际采用的模式。相关性查询的 BM25 索引不可用时默认整次回退 direct 并返回 warning；`--strict-backend` 改为返回 `index_stale`。匹配、排序、解释和索引新鲜度规则见[搜索参考](search.md)。

`cache rebuild` 从实际文件重建轻量目录；启用 BM25 时同时增量维护字段索引。它不创建知识内容。`source verify` 核对来源记录引用的所有历史对象，逐项返回 `pass`、`fail` 或 `not_checked`。成功取得报告不代表所有对象通过：自动化必须检查各项状态；来源记录无法解析时整个命令失败。

存在未恢复的来源保存或知识保存时，`review`、`query`、`cache rebuild`、`source verify`、`lint` 和其他写入返回 `vault_needs_recovery`。可以查看 `status`、读取配置、查看操作计划，并重试对应 `apply`。

`lint` 检查实际 Wiki Markdown、OKF frontmatter、受管理文档、链接和来源引用，不写入知识或缓存。默认模式返回完整报告并退出 0；`--strict` 在报告含任何发现时退出非零。字段、finding code 和限制见 [Wiki lint 参考](lint.md)。

## 配置与准入

```text
kb config show [--sources] [--vault <PATH_OR_ID>] [--json]
kb config get <KEY> [--vault <PATH_OR_ID>] [--json]
kb config set <KEY> <VALUE> [--vault <PATH_OR_ID>] [--local|--user] [--yes] [--json]
kb config unset <KEY> [--vault <PATH_OR_ID>] [--local|--user] [--yes] [--json]
kb config validate [--vault <PATH_OR_ID>] [--json]

kb config admission list [--vault <PATH_OR_ID>] [--json]
kb config admission add <ID> <TOP_LEVEL_DIRECTORY> [--vault <PATH_OR_ID>] [--yes] [--json]
kb config admission enable <ID> [--vault <PATH_OR_ID>] [--yes] [--json]
kb config admission disable <ID> [--vault <PATH_OR_ID>] [--yes] [--json]
kb config admission remove <ID> [--vault <PATH_OR_ID>] [--yes] [--json]
```

修改命令没有 `--yes` 时只预览差异。字段、默认值、层级和准入规则由[配置参考](configuration.md)统一说明。

## 注册与路径

```text
kb vault list [--json]
kb vault register <PATH> [--json]
kb vault rebind <VAULT_ID> <PATH> [--json]
kb vault unregister <VAULT_ID> [--json]
kb paths [--vault <PATH_OR_ID>] [--json]
```

移动 Vault 后，旧登记不会自动猜测新位置。使用 `rebind` 核对新位置内的 `vault_id` 后更新本机注册表。`unregister` 不删除 Vault 数据。

## 状态与诊断

```text
kb status [--vault <PATH_OR_ID>] [--json]
kb doctor [--vault <PATH_OR_ID>] [--json]
kb version [--json]
kb capabilities [--json]
```

`status` 返回选中 Vault 的事实状态，包括 schema 兼容性、配置、准入数量、恢复记录和缓存状态。`older_unsupported` 表示生产迁移目录没有到当前 schema 的完整路径；它只保留诊断能力。当前 schema 的配置损坏会让命令失败。

旧且不受支持的 schema 上，修改命令返回 `migration_unavailable`，并且不修改 Vault 配置。产品目前没有历史迁移路径，也不提供 `kb migrate` 命令；迁移路径的决策见 [ADR-0016](../decisions/accepted/architecture/0016-explicit-schema-migration-paths.md)。

`doctor` 返回彼此独立的 `pass`、`warn`、`fail` 或 `not_checked` 检查，不计算总分。配置损坏作为单项失败保留在报告中。`vault_structure` 检查选中 Vault 的必需文件和目录，并逐项报告缺失或类型无效的 Vault 路径；它不检查机器本地目录。`machine_runtime_directories` 检查本机的配置、状态和缓存目录：它们不是 Vault 内容，缺失时会按需创建，因此普通缺失仍为 `pass`；只有既有路径无法检查、不是目录或权限不可用时才报告发现。除锁检查可以创建并移除自己的空锁文件外，doctor 不编辑配置或 Wiki。

`version` 报告程序与 schema 版本。`capabilities` 明确报告功能是否实现；客户端不能从程序版本号推断能力。

## HTTP 服务

```text
kb serve [--bind <IP:PORT>] [--token-file <PATH>] [--allow-write] [--vault <PATH_OR_ID>]
```

默认回环、只读且无需 token；局域网监听和 HTTP apply 都要求 token 文件。服务固定使用启动时选中的 Vault，不能从请求切换路径。路由、鉴权和明文网络边界见 [HTTP 参考](http.md)。

## Portable Agent Skill

```text
kb skills detect [--vault <PATH_OR_ID>] [--json]
kb skills install [--host auto|codex|claude-code|gemini-cli|opencode] [--scope vault|user] [--mode copy|symlink] [--vault <PATH_OR_ID>] [--json]
kb skills status [--host auto|codex|claude-code|gemini-cli|opencode] [--scope vault|user] [--vault <PATH_OR_ID>] [--json]
kb skills uninstall [--host auto|codex|claude-code|gemini-cli|opencode] [--scope vault|user] [--vault <PATH_OR_ID>] [--json]
```

安装和卸载只创建可审阅 operation，必须再使用 `kb apply` 执行。它管理八项顶层 `kb-*` Skill；状态会区分 `absent`、`current`、`partial`、`modified`、`external` 与可迁移的 `legacy`。宿主路径、检测歧义、复制与链接模式，以及 `npx skills add` 外部安装边界见 [Portable Agent Skill 参考](agent-skill.md)。

## MCP stdio

```text
kb mcp [--vault <PATH_OR_ID>] [--allow-write]
```

服务固定使用启动时选中的 Vault，默认只暴露读取和计划工具。`--allow-write` 才注册 apply 工具；operation 归属和旧状态仍由应用层复核。工具列表、客户端配置和协议输出边界见 [MCP 参考](mcp.md)。
