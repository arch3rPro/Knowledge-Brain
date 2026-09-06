# Stage 1 命令参考

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
