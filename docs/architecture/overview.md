# 架构概览

Knowledge-Brain 把可迁移的 Vault 数据、机器本地状态和产品程序分开。Vault 复制到其他位置或系统后仍携带自己的身份、配置和人类内容；注册表、执行进度和宿主缓存不随 Vault 迁移。

```text
CLI / HTTP / future GUI / future WebUI / future MCP
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
- `kb-app`：完整用例、Vault 选择、配置合并、无损编辑、注册、锁、采用计划、来源保存、搜索、备份、状态和诊断。所有入口应调用这里的 `AppRequest → AppResponse`。
- `kb-protocol`：带 `schema_version` 的成功与错误 JSON 信封。
- `kb-cli`：解析命令参数并渲染人类或 JSON 输出；不直接读写 Vault 文件。
- `kb-server`：HTTP 路由、鉴权、请求大小和状态码映射；只调用固定 Vault 的应用请求。

## 数据边界

Vault 内的持久内容包括根级 `admission.yml`、`KB.md`、固定三层 `Wiki/` 和 `.kb/config.yml`。`.kb/cache/` 可以重建；`.kb/runtime/` 不存知识，但未完成操作的恢复标记必须保留到恢复完成。

用户级目录由操作系统规范解析，也可用 `KB_CONFIG_DIR`、`KB_STATE_DIR`、`KB_CACHE_DIR` 显式覆盖：

- 配置目录保存 `vaults.yml` 注册表和用户级 `config.yml`。
- 状态目录保存待执行计划、执行进度和完成凭据。
- 用户缓存目录为后续宿主级缓存保留。

Vault 注册表可以保存本机绝对路径；Vault 内生成的相对路径统一使用 `/`，并在写入前按 Windows、macOS 和 Linux 的共同规则校验。

## 写入模型

单文件配置修改先解析、编辑具体 YAML 语法树、重新解析完整候选配置，再原子替换。已有目录采用分成两个命令：`kb adopt` 只记录审核快照和将创建的文件，`kb apply` 在写入前重新核对所有原路径。

采用、来源保存和知识保存都把计划、进度与完成凭据保存在用户状态目录。知识请求由入口解析为 `KnowledgePlanRequest`，`kb-app` 统一生成 index/log、核对旧哈希并执行整批保存，因此未来 GUI、WebUI、MCP 和 HTTP 不需要重写文件规则。中断恢复只处理进度明确记录的路径；独立变化的文件会保留并要求人工判断。Vault 数据变更使用操作系统文件锁，锁的所有权由 OS 锁决定，JSON 锁信息只用于解释冲突。完整契约见[知识计划参考](../reference/knowledge-plans.md)。

## 兼容边界

当前 schema 是 `v1.0`。旧且可迁移的 schema 可以诊断和读取，但写入返回 `migration_required`；同主版本的更新 schema 以及更新主版本只开放诊断路径，写入返回 `schema_too_new`。程序版本使用 SemVer，不能代替 schema 兼容判断。

当前能力状态见 `kb capabilities --json`，实施状态见 [Roadmap](../../ROADMAP.md)。

## 来源与读取

来源身份、精确版本、提取器、查询类型和 OKF 单文档规则定义在 `kb-core`；文件发现、来源记录、保存计划及恢复、原始对象核对、搜索和 Wiki 跨文档 lint 由 `kb-app` 实现。`Extractor` 接收受限字节，不访问文件系统。新提取器复用该契约，入口不自行执行提取或文件写入。

来源保存也使用独立 review/apply 流程。记录、对象和日志共同恢复，读取入口在未完成保存时拒绝读取混合状态。来源快照与恢复规则由[来源参考](../reference/sources.md)定义；目录缓存及查询行为由[搜索参考](../reference/search.md)定义。

CLI 和未来应用入口消费同一个 `LintReport`。文档规则和 CLI 的 strict 退出策略由 [Wiki lint 参考](../reference/lint.md)定义。

## HTTP 边界

`kb serve` 在同一可执行文件中按需启动，不把 CLI 变成 daemon 客户端。服务启动时把一个 Vault 解析为稳定 ID，之后每个路由只构造该 Vault 的 typed `AppRequest`。HTTP 的 token、监听地址和写入开关属于进程启动策略，不进入可迁移 Vault 配置。完整路由与网络限制见 [HTTP 参考](../reference/http.md)。

## 备份边界

备份只迁移 Vault 共享内容，不携带本机配置、缓存、恢复状态或注册表。`kb-app` 统一收集准入目录、生成和验证逐文件清单，并把已完整复核的 ZIP 暂存恢复到空目标；入口适配器不自行解压。完整契约见[备份参考](../reference/backup.md)。
