# 命令参考

所有带 `--json` 的命令在 stdout 输出一个 JSON 信封。成功数据位于 `data`，失败数据位于 `error`；业务失败使用非零退出码。JSON 模式不会把提示或诊断混入 stderr。每个信封都携带 `schema_version`。

## Vault 创建与采用

```text
kb init <TARGET> [--json]
kb adopt <TARGET> [--json]
kb apply <OPERATION_ID> [--json]
kb operation show <OPERATION_ID> [--json]
```

`init` 只接受不存在或空目录，创建最小 Vault 并尝试登记。它不创建 Git 仓库。

`adopt` 审查已有目录并把计划保存到用户状态目录，不修改目标。`apply` 按操作 ID 重新验证并执行；同一已完成 ID 再次执行时返回保存的结果，不重复产生影响。

## 来源与搜索

```text
kb review [--vault <PATH_OR_ID>] [--json]
kb query <QUERY> [--scope wiki|sources|all] [--limit <1..100>] [--vault <PATH_OR_ID>] [--json]
kb cache rebuild [--vault <PATH_OR_ID>] [--json]
kb source verify [--vault <PATH_OR_ID>] [--json]
```

`review` 只查看启用的准入目录，返回新增、变化、删除及可能移动的来源。有变化时保存用户状态目录中的计划；没有变化时 `operation_id` 为 `null`。它不保存来源、不改主题文件或 Wiki；明确执行该 ID 的 `apply` 才保存来源与日志。计划核对及恢复规则见[来源格式](sources.md)。

`query` 默认 `scope=wiki`、`limit=10`。空白查询或越界 limit 返回 `invalid_query`。`all` 固定返回 Wiki、来源两组，limit 分别作用于每组；不生成 LLM 回答。匹配、排序和定位规则见[搜索参考](search.md)。

`cache rebuild` 从实际文件重建目录，不创建知识内容。`source verify` 核对来源记录引用的所有历史对象，逐项返回 `pass`、`fail` 或 `not_checked`。成功取得报告不代表所有对象通过：自动化必须检查各项状态；来源记录无法解析时整个命令失败。

存在未恢复的来源保存时，`review`、`query`、`cache rebuild`、`source verify` 和配置写入返回 `vault_needs_recovery`。可以查看 `status`、读取配置、查看操作计划，并重试对应 `apply`。

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

`status` 返回选中 Vault 的事实状态，包括 schema 兼容性、配置、准入数量、恢复记录和缓存状态。当前 schema 的配置损坏会让命令失败。

`doctor` 返回彼此独立的 `pass`、`warn`、`fail` 或 `not_checked` 检查，不计算总分。配置损坏作为单项失败保留在报告中。除锁检查可以创建并移除自己的空锁文件外，doctor 不编辑配置或 Wiki。

`version` 报告程序与 schema 版本。`capabilities` 明确报告功能是否实现；客户端不能从程序版本号推断能力。
