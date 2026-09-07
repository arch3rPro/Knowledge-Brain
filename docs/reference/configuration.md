# 配置参考

## 配置层与优先级

同一字段从低到高按以下顺序合并：

1. 内置默认值
2. 用户配置 `<config_dir>/config.yml`
3. Vault 配置 `<vault>/.kb/config.yml`
4. 本机 Vault 配置 `<vault>/.kb/config.local.yml`
5. 环境变量
6. 请求级覆盖；应用接口已保留该层，当前 CLI 没有通用覆盖参数

高优先级值无效时命令失败，不会退回低优先级值。`kb config show --sources` 显示每个生效字段的值和来源。

`schema_version` 在每个存在的 YAML 配置文件中都是必填项。`vault_id` 只能出现在 `.kb/config.yml`，不能由用户层、本机层或 `kb config set` 覆盖。

## 可配置字段

| 键 | 类型 | 默认值 | 环境变量 |
| --- | --- | --- | --- |
| `search.mode` | `direct` 或 `bm25` | `direct` | `KB_SEARCH_MODE` |
| `limits.max_file_bytes` | 正整数 | `52428800` | `KB_LIMITS_MAX_FILE_BYTES` |
| `limits.max_files_per_review` | 正整数 | `10000` | `KB_LIMITS_MAX_FILES_PER_REVIEW` |
| `limits.max_total_read_bytes` | 正整数 | `524288000` | `KB_LIMITS_MAX_TOTAL_READ_BYTES` |
| `files.include_hidden` | `true` 或 `false` | `false` | `KB_FILES_INCLUDE_HIDDEN` |
| `operations.plan_retention_hours` | 正整数 | `168` | `KB_OPERATIONS_PLAN_RETENTION_HOURS` |

`direct` 查询真实文件。`bm25` 偏好可以保存，但在二进制未提供 BM25 时返回明确警告并使用 direct；实际能力以 `kb capabilities --json` 为准。

来源保存计划在 `operations.plan_retention_hours` 后不能首次执行；重新运行 `kb review`。已经中断的保存先恢复旧内容，再判断计划是否仍可执行；完成回执不受过期规则影响。此字段不自动删除计划、恢复记录或来源历史。

读取限制适用于来源遍历、记录读取和查询的各个读取阶段。文件数量按遍历遇到的条目（包括目录）保守计数，进入排除子树前即停止。总读取字节限制是每个读取阶段的上限，不是整个进程多次复核的累计 IO 上限；重复核对和恢复可能再次读取相同文件。

`kb config set/unset` 默认编辑 `.kb/config.yml`。`--local` 改为编辑 `.kb/config.local.yml`，`--user` 改为编辑用户配置；两个参数不能同时使用。没有 `--yes` 时只返回差异预览。编辑器保留无关字段、字段顺序和注释；如果无法安全保留，则拒绝写入。

## `admission.yml`

`admission.yml` 是准入清单，不是待办队列。只有这里启用的一级目录才会进入 `kb review` 的来源读取范围。

```yaml
schema_version: "v1.0"

directories:
  - id: work-notes
    path: Work-Notes
    enabled: true
    include:
      - "**/*.md"
    exclude:
      - "**/.git/**"
```

规则：

- `id` 在清单内唯一，并作为启用、停用和移除操作的稳定标识。
- `path` 必须是已存在的可移植一级目录，不能是绝对路径或嵌套路径。
- `Wiki`、`.kb`、链接、junction/reparse point、Windows 保留名和跨平台大小写/Unicode 冲突会被拒绝。
- 即使条目为 `enabled: false`，其结构和路径仍会校验。
- 未写 `include` 时默认包含 Markdown、文本、HTML、EPUB、DOCX 和 PDF；未写 `exclude` 时默认排除 `.git` 与 `.kb`。PDF 当前只保存原始对象和元数据。
- `kb config admission remove` 只删除准入记录，不删除对应目录。
- 停用、移除或排除来源不会删除已保存的副本，也不会伪造“来源已删除”的变更。
- glob 相对主题根匹配，区分大小写；`**` 匹配任意层级。排除规则优先，隐藏文件默认跳过，`.git` 与 `.kb` 始终跳过。

准入目录不能从其他配置层隐式增加。高级 `include/exclude` 当前通过人工编辑 YAML 配置，再用 `kb config validate` 校验。

## Vault 选择

需要 Vault 的命令按以下顺序选择：显式 `--vault <path-or-id>`、`KB_VAULT`、当前目录的最近 Vault 祖先、唯一的已注册 Vault。存在多个注册项且没有更明确选择时，命令失败，不按最近使用时间猜测。

用户目录覆盖变量必须是非空绝对路径。Vault 注册失败不会撤销已经成功的 `init` 或 `apply`；结果的 `warnings` 会给出 `kb vault register <path>` 修复命令。
