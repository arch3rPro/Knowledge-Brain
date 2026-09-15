# 知识计划参考

知识计划把外部 Agent、脚本、未来 GUI 或 WebUI 提出的内容，与 Knowledge-Brain 实际写入 Vault 的过程分开。入口先提交结构化请求；应用层核对当前文件、生成可审阅差异和派生文件；只有明确执行 operation ID 才会修改 Vault。

## 命令

```text
kb plan create <REQUEST_JSON> [--vault <PATH_OR_ID>] [--json]
kb operation show <OPERATION_ID> [--json]
kb apply <OPERATION_ID> [--json]
```

`plan create` 读取 UTF-8 JSON，只把计划保存在用户状态目录，不修改 Vault。`operation show` 返回计划及完整 diff。`apply` 重新核对计划观察到的状态后保存；同一已完成 operation ID 再次执行时返回原结果，不重复写日志。

## 请求

所有入口把输入解析为应用层共享的 `KnowledgePlanRequest`；文件路径只是 CLI 适配器参数，不属于应用层契约。

```json
{
  "schema_version": "v1.0",
  "changes": [
    {
      "kind": "upsert",
      "path": "articles/local-first.md",
      "before_sha256": null,
      "summary": "Add a reusable explanation of local-first storage.",
      "content": "---\ntype: Article\n...\n"
    }
  ]
}
```

字段规则：

- `schema_version` 当前必须是 `v1.0`。
- `changes` 至少一项，不超过配置的单次文件数限制，所有来源路径和目标路径不得重复。
- `kind` 支持 `upsert`、`delete` 和 `move`；省略时为兼容既有请求的 `upsert`。
- `path` 相对于 `Wiki/`，只接受 `research/**/*.md` 或 `articles/**/*.md`。保留名 `index.md`、`log.md` 在大小写不敏感的文件系统上同样禁止。
- 新建使用 `before_sha256: null`；替换使用读取原始字节得到的 64 位小写 SHA-256，并提供完整 `content`。
- 删除使用 `kind: "delete"` 和旧文件完整 SHA-256，不提供 `content`。移动使用 `kind: "move"`、`from_path`、目标 `path`、来源完整 SHA-256 和目标完整 `content`；目标必须不存在。删除和移动只处理受管理 concept，空内容不会被解释为删除。
- `summary` 必须是非空单行文本，作为人类可读日志条目。
- `content` 是完整 UTF-8 Markdown，不是 patch。它必须显式包含 `kb.managed: true`，通过 Producer Profile，并且不能已经达到 `stale_after`。
- `sources[].resource` 中的 `kb-source://` 引用必须指向 Vault 已保存的精确来源版本。
- 新内容使用 `kb.origin` 明确来源类型。`external_research` 至少需要一个已存在的精确 `kb-source://` 版本；普通 URL 不满足准入要求。`original` 表示不声称外部证据基础的用户原创内容，允许省略 `sources`。未声明 `origin` 保留给已有兼容内容，继续要求至少一个 source。
- 保存后的完整 `research` 或 `articles` 分区内不得出现重复 `title`。比较会统一 Unicode、连续空白和大小写；冲突返回 `invalid_config`，并在 `details` 中提供 `finding_code: duplicate_title`、分区和全部冲突路径。两个分区之间允许同名。

请求不能直接写 `Wiki/index.md`、`Wiki/log.md`、来源记录、原始对象、配置或运行目录。

Agent 对明确的 Wiki 变更必须使用本入口，不直接创建、修改、移动或删除 `Wiki/` 下的文件。人类仍可编辑 Markdown；之后使用 `kb lint` 检查普通页面误入受管理分区、索引缺项和其他结构问题，再决定是否提交受管理保存。正文历史恢复依赖用户已启用的 Git 或备份，不由 Knowledge-Brain 猜测旧版本。

## 计划和结果

计划包含 operation ID、Vault ID、绝对目标根、创建时间、程序与 schema 版本、请求内容、精确来源版本、配置与准入摘要、所有目标的旧哈希和完整新内容，以及统一文本 diff。计划和 `plan.sha256` 位于用户状态目录的 `operations/<operation_id>/`；目录在 Unix 上使用仅当前用户可访问的权限。

成功结果包含：

```json
{
  "kind": "save_knowledge",
  "operation_id": "...",
  "vault_id": "...",
  "target": "/absolute/vault/path",
  "changed": ["Wiki/articles/local-first.md", "Wiki/index.md", "Wiki/log.md"],
  "warnings": []
}
```

绝对路径只存在机器本地的计划和结果中，不写入可迁移的 Vault 内容。

## index 与 log

应用层根据计划执行后的全部受管理 research/article 文档生成 `Wiki/index.md` 的受管理区域，并根据本次请求生成 `Wiki/log.md` 的日期条目。日志明确区分 Creation、Update、Deletion 和 Move；移动记录旧路径和目标链接。两个文件都只替换以下标记之间的内容：

```markdown
<!-- kb:managed:start -->
<!-- kb:managed:end -->
```

标记必须各出现一次且顺序正确。标记之外的人类内容保持原字节；请求不能自行提供这两个派生文件。

## 保存、冲突与恢复

保存前会在 Vault 独占锁内重新核对计划摘要、有效期、程序和 schema 版本、Vault ID、配置、准入、精确来源版本和每个目标的旧哈希。任一项变化都会在写入前返回 `plan_stale`。

写入期间，用户状态目录保存每个目标的原字节和新哈希，Vault 的 `.kb/runtime/knowledge-pending.json` 标记暂时不可读取的混合状态。普通错误会按相反顺序恢复已经处理的文件。进程意外退出后，重试同一 `kb apply <OPERATION_ID>`：尚无完成凭据时先恢复再重新执行；已有完成凭据时完成清理并返回原结果。

恢复只覆盖仍等于该计划新内容的文件。若文件同时不同于计划的新旧版本，Knowledge-Brain 保留它并返回 `vault_needs_recovery`，不会覆盖独立编辑。来源保存与知识保存的 pending 状态互斥。

`result.json` 是知识已保存的完成依据。完成后轻量目录和 BM25 索引会被删除并按需重建；缓存清理失败作为 warning 返回，不撤销已经保存的 Markdown。

## 当前边界

Stage 3B 不调用 LLM，不从自然语言推断目标，不支持任意来源记录修改，也不承诺断电级持久性。MCP、HTTP、WebUI 和 GUI 都必须把结构化请求交给同一个应用层，不得重写上述规则。
