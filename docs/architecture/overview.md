# 架构概览

Knowledge-Brain 把可迁移的 Vault 数据、机器本地状态和产品程序分开。Vault 复制到其他位置或系统后仍携带自己的身份、配置和人类内容；注册表、执行进度和宿主缓存不随 Vault 迁移。

```text
CLI / future GUI / future WebUI / future MCP / future HTTP
                         │
                         ▼
                  kb-app application layer
                         │
          ┌──────────────┼──────────────┐
          ▼              ▼              ▼
       kb-core       Vault files    local user state
    rules + types   Markdown/YAML   registry/operations
```

## Workspace 职责

- `kb-core`：schema 版本、稳定错误码、可移植路径、准入模型和操作计划类型。它不处理界面呈现。
- `kb-app`：完整用例、Vault 选择、配置合并、无损编辑、注册、锁、采用计划、状态和诊断。所有入口应调用这里的 `AppRequest → AppResponse`。
- `kb-protocol`：带 `schema_version` 的成功与错误 JSON 信封。
- `kb-cli`：解析命令参数并渲染人类或 JSON 输出；不直接读写 Vault 文件。

## 数据边界

Vault 内的持久内容包括根级 `admission.yml`、`KB.md`、固定三层 `Wiki/` 和 `.kb/config.yml`。`.kb/cache/` 与 `.kb/runtime/` 是可重建或运行时状态，不能成为知识的唯一副本。

用户级目录由操作系统规范解析，也可用 `KB_CONFIG_DIR`、`KB_STATE_DIR`、`KB_CACHE_DIR` 显式覆盖：

- 配置目录保存 `vaults.yml` 注册表和用户级 `config.yml`。
- 状态目录保存待执行计划、执行进度和完成凭据。
- 用户缓存目录为后续宿主级缓存保留。

Vault 注册表可以保存本机绝对路径；Vault 内生成的相对路径统一使用 `/`，并在写入前按 Windows、macOS 和 Linux 的共同规则校验。

## 写入模型

单文件配置修改先解析、编辑具体 YAML 语法树、重新解析完整候选配置，再原子替换。已有目录采用分成两个命令：`kb adopt` 只记录审核快照和将创建的文件，`kb apply` 在写入前重新核对所有原路径。

采用执行把进度保存在用户状态目录。中断恢复只处理该进度明确记录为本操作创建的路径，并再次核对生成文件哈希；未记录路径或已变化文件不会被删除。Vault 数据变更使用操作系统文件锁，锁的所有权由 OS 锁决定，JSON 锁信息只用于解释冲突。

## 兼容边界

当前 schema 是 `v1.0`。旧且可迁移的 schema 可以诊断和读取，但写入返回 `migration_required`；同主版本的更新 schema 以及更新主版本只开放诊断路径，写入返回 `schema_too_new`。程序版本使用 SemVer，不能代替 schema 兼容判断。

当前能力状态见 `kb capabilities --json`。Stage 1 没有搜索、提取器、MCP、HTTP、网络服务或内置 LLM。
