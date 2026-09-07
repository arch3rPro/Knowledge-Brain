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
- `changes` 至少一项，不超过配置的单次文件数限制，path 不得重复。
- `path` 相对于 `Wiki/`，只接受 `research/**/*.md` 或 `articles/**/*.md`。保留名 `index.md`、`log.md` 在大小写不敏感的文件系统上同样禁止。
- 新建文件使用 `before_sha256: null`；替换文件使用读取原始字节得到的 64 位小写 SHA-256。Stage 3B 不支持删除或移动。
- `summary` 必须是非空单行文本，作为人类可读日志条目。
- `content` 是完整 UTF-8 Markdown，不是 patch。它必须显式包含 `kb.managed: true`，通过 Producer Profile，并且不能已经达到 `stale_after`。
- `sources[].resource` 中的 `kb-source://` 引用必须指向 Vault 已保存的精确来源版本。

请求不能直接写 `Wiki/index.md`、`Wiki/log.md`、来源记录、原始对象、配置或运行目录。

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

应用层根据计划执行后的全部受管理 research/article 文档生成 `Wiki/index.md` 的受管理区域，并根据本次请求生成 `Wiki/log.md` 的日期条目。两个文件都只替换以下标记之间的内容：

```markdown
<!-- kb:managed:start -->
<!-- kb:managed:end -->
```

标记必须各出现一次且顺序正确。标记之外的人类内容保持原字节；请求不能自行提供这两个派生文件。

## 保存、冲突与恢复

保存前会在 Vault 独占锁内重新核对计划摘要、有效期、程序和 schema 版本、Vault ID、配置、准入、精确来源版本和每个目标的旧哈希。任一项变化都会在写入前返回 `plan_stale`。

写入期间，用户状态目录保存每个目标的原字节和新哈希，Vault 的 `.kb/runtime/knowledge-pending.json` 标记暂时不可读取的混合状态。普通错误会按相反顺序恢复已经处理的文件。进程意外退出后，重试同一 `kb apply <OPERATION_ID>`：尚无完成凭据时先恢复再重新执行；已有完成凭据时完成清理并返回原结果。

恢复只覆盖仍等于该计划新内容的文件。若文件同时不同于计划的新旧版本，Knowledge-Brain 保留它并返回 `vault_needs_recovery`，不会覆盖独立编辑。来源保存与知识保存的 pending 状态互斥。

`result.json` 是知识已保存的完成依据。完成后目录缓存会被删除并按需重建；缓存清理失败作为 warning 返回，不撤销已经保存的 Markdown。

## 当前边界

Stage 3B 不调用 LLM，不从自然语言推断目标，不支持删除、移动或任意来源记录修改，也不承诺断电级持久性。MCP、HTTP、WebUI 和 GUI 尚未实现，但它们应把已解析请求交给同一个应用层，而不是重写上述规则。
