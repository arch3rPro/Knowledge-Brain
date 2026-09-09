# Wiki lint 参考

`kb lint` 对选中 Vault 的 `Wiki/` 执行离线、确定性结构检查。它读取实际 Markdown 和来源记录，不依赖搜索缓存，也不修改 Wiki、缓存、知识日志或操作进度。

```text
kb lint [--strict] [--vault <PATH_OR_ID>] [--json]
```

默认模式只要完整报告成功生成就退出 0，即使报告包含 error 或 warning。`--strict` 在 `findings` 非空时退出非零，报告内容与默认模式相同。自动化如果以发现作为失败条件，必须使用 `--strict` 或读取 `findings`。

## 报告

JSON 模式将以下数据放在标准成功信封的 `data` 中：

```json
{
  "schema_version": "v1.0",
  "checked_files": 3,
  "findings": [
    {
      "severity": "warning",
      "code": "orphan_concept",
      "path": "Wiki/articles/example.md",
      "message": "No Wiki concept or index links to this concept.",
      "remediation": "Link it from a relevant concept or index, or leave it intentionally orphaned."
    }
  ]
}
```

`line` 在可以定位时出现，使用从 1 开始的文件行号。发现按 `path`、`line`、`severity`、`code` 排序，因此相同输入会产生稳定顺序。`error` 表示格式或引用契约不成立；`warning` 表示内容仍可读取，但可能过期、断链或缺少导航关系。

无法安全遍历路径、超过读取上限、Vault 配置不可读取或存在待恢复来源保存时，命令返回标准业务错误，而不是不完整的 lint 报告。

## 文档范围

扫描范围包括：

- `Wiki/index.md` 和 `Wiki/log.md`；
- `Wiki/research/**/*.md`；
- `Wiki/articles/**/*.md`；
- `Wiki/external-sources/records/**/*.md`。

`Wiki/external-sources/.objects/` 保存原始字节，不作为 Markdown 扫描。遍历不跟随符号链接或 Windows reparse point，并使用 Vault 的文件数、单文件字节数和总读取量上限。

普通 concept 遵守 OKF v0.2 的开放底线：UTF-8 Markdown、文件开头的 YAML frontmatter mapping，以及非空字符串 `type`。未知类型和字段合法。`index.md`、`log.md` 使用 OKF 保留文件规则。

只有 concept 明确声明以下字段时才使用严格的 Knowledge-Brain Producer Profile：

```yaml
kb:
  managed: true
```

受管理 concept 还需要非空 `title`、显式合法 `status`、带 `by` 和 `at` 的 `generated`，以及至少一个带唯一 `id` 和 `resource` 的 source。`verified` 仍是可选字段；`stable` 不代表已经验证。可选 `kb.supersedes` 只保存从当前 concept 指向旧 concept 的单向关系。

Knowledge-Brain 采用 OKF v0.2，并且不改变未知字段的含义。

## Finding codes

| Code | Severity | 含义 |
| --- | --- | --- |
| `markdown_not_utf8` | error | Markdown 不是 UTF-8。 |
| `frontmatter_required` | error | concept 缺少开头 frontmatter。 |
| `frontmatter_unclosed` | error | frontmatter 没有闭合。 |
| `frontmatter_invalid` | error | frontmatter 不是有效 YAML mapping。 |
| `type_required` | error | concept 缺少非空字符串 `type`。 |
| `reserved_frontmatter_forbidden` | error | 保留文件包含不允许的 frontmatter。 |
| `okf_version_unsupported` | error | `Wiki/index.md` 声明的 OKF 版本不是 `0.2`。 |
| `log_date_heading_invalid` | error | log 的二级标题不是 `YYYY-MM-DD`。 |
| `status_invalid` | error | status 不是 `draft`、`stable` 或 `deprecated`。 |
| `generated_invalid` | error | generated 不是 mapping。 |
| `generated_actor_required` | error | `generated.by` 缺失或为空。 |
| `generated_actor_invalid` | error | `generated.by` 不符合 OKF actor 约定。 |
| `generated_timestamp_invalid` | error | 已提供的 `generated.at` 不是带 offset 的 ISO 8601 时间。 |
| `verified_invalid` | error | verified 不是 mapping 或 mapping 列表。 |
| `verified_actor_required` | error | verification event 缺少非空 `by`。 |
| `verified_actor_invalid` | error | verification event 的 `by` 不符合 OKF actor 约定。 |
| `verified_timestamp_invalid` | error | verification event 缺少合法 `at`。 |
| `sources_invalid` | error | sources 不是列表。 |
| `source_invalid` | error | source 项不是 mapping。 |
| `source_resource_required` | error | source 缺少非空 resource。 |
| `source_id_duplicate` | error | 同一 concept 内 source ID 重复。 |
| `source_last_modified_invalid` | error | source 的 last_modified 不是带 offset 的 ISO 8601 时间。 |
| `usage_window_invalid` | error | usage_window 缺少合法的 from 或 to。 |
| `stale_after_invalid` | error | stale_after 不是带 offset 的 ISO 8601 时间。 |
| `stale_document` | warning | 当前时间已达到 stale_after。 |
| `managed_title_required` | error | 受管理 concept 缺少 title。 |
| `managed_status_required` | error | 受管理 concept 没有显式 status。 |
| `managed_generated_required` | error | 受管理 concept 缺少 generated mapping。 |
| `managed_generated_timestamp_required` | error | 受管理 concept 缺少合法 generated.at。 |
| `managed_sources_required` | error | 受管理 concept 没有 source。 |
| `managed_source_id_required` | error | 受管理 source 缺少非空 ID。 |
| `supersedes_entry_invalid` | error | kb.supersedes 不是字符串列表。 |
| `supersedes_self` | error | concept 指定自己为被替代目标。 |
| `supersedes_invalid_target` | error | 被替代目标不是现存 concept。 |
| `link_outside_wiki` | error | 本地 Markdown 链接逃出 Wiki 或进入 `.objects`。 |
| `link_target_invalid` | error | 链接包含无效编码或不可移植路径。 |
| `broken_link` | warning | concept 的本地 Markdown 目标不存在。 |
| `index_drift` | warning | index 中列出的本地目标不存在。 |
| `orphan_concept` | warning | research/article 没有入站 Wiki 引用。 |
| `portable_path_collision` | error | 路径在大小写或 Unicode 可移植规则下冲突。 |
| `source_record_invalid` | error | 来源记录的 `kb.source` 无法解析。 |
| `source_identity_invalid` | error | 来源身份、历史或记录路径不一致。 |
| `source_version_missing` | error | 精确 kb-source URI 格式错误或历史版本不存在。 |
| `source_version_outdated` | warning | 精确版本存在，但不是来源记录当前版本。 |

## 链接限制

lint 检查标准 Markdown 文件目标，并忽略外部 scheme。以 `/` 开头的链接相对于 `Wiki/`，其他链接相对于当前文件。百分号编码会按 UTF-8 解码。fragment 会从文件查找中移除；当前版本不校验 heading fragment，因为 Markdown 消费者的 slug 规则并不统一。

孤立页和 index drift 是导航提示，不代表文档不符合 OKF。index 不需要列出每个 concept；后续知识保存能力负责受管理区域的同步。
