# 知识计划与安全保存设计

- 状态：已确认，进入实现
- 日期：2026-09-07
- 范围：Stage 3B

## 目标

Stage 3B 提供从结构化知识建议到持久 Markdown 的最小完整闭环：创建可审查计划、查看精确目标和差异、按 operation ID 保存、处理中断后恢复，以及重复执行已完成 ID 时返回原结果。CLI、未来 MCP、HTTP、WebUI 和 GUI 复用相同应用请求和持久计划格式。

## 命令

```text
kb plan create <REQUEST_JSON> [--vault <PATH_OR_ID>] [--json]
kb operation show <OPERATION_ID> [--json]
kb apply <OPERATION_ID> [--json]
```

CLI 从 UTF-8 JSON 文件读取请求。文件是入口输入，不进入 Vault；应用层接收已解析的 `KnowledgePlanRequest`，不依赖 CLI 文件路径。

## 请求格式

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

规则：

- `changes` 包含 1 至配置文件数上限个项目，path 不重复。
- path 相对于 `Wiki/`，只允许 `research/**/*.md` 和 `articles/**/*.md`；使用 `/` 和共同跨平台路径规则。
- Stage 3B 只支持新建和替换，不支持删除或移动。
- 新建目标的 `before_sha256` 必须为 `null` 且文件不存在；替换目标必须提供当前原始字节的 64 位小写 SHA-256。计划创建时立即核对。
- `summary` 是非空单行文本，用于人类变更日志；不得包含换行。
- `content` 是完整 UTF-8 Markdown，大小不超过 Vault 单文件上限。
- content 必须是 OKF concept，显式包含 `kb.managed: true`，并通过 Knowledge-Brain Producer Profile。error finding 拒绝计划；`stale_document` warning 也拒绝，因为新计划不能创建已过期内容。
- `sources[].resource` 中的每个 `kb-source://` URI 必须是来源记录里存在的精确版本。计划保存这些精确版本并在 apply 前复核。

## 计划格式与审查

应用层生成 `KnowledgePlan`，包含：

- schema、operation ID、Vault ID、绝对目标根、创建时间和程序版本；
- 原始请求中的目标 path、before hash、summary 和完整 content；
- 从 content 推导并去重的精确来源版本；
- `admission.yml` 哈希和影响读取行为的有效配置摘要哈希；
- 派生的 `Wiki/index.md` 与 `Wiki/log.md` 完整目标及原始哈希；
- 所有写入目标的统一文本 diff。

计划和独立 `plan.sha256` 保存在用户状态目录 `operations/<operation_id>/`。`operation show` 返回完整计划；人类输出优先显示 diff，JSON 返回完整 typed plan。计划文件被修改后 apply 返回 `plan_stale`。

## 受管理 index

`Wiki/index.md` 只替换以下边界之间的内容：

```markdown
<!-- kb:managed:start -->
<!-- kb:managed:end -->
```

边界必须各出现一次且顺序正确，否则拒绝计划。受管理区域从计划执行后的全部 `kb.managed: true` research/article 重建，分为 `## Research` 和 `## Articles`，各项按可移植 path 排序：

```markdown
- [Title](articles/path.md) — description
```

`description` 缺失时不写破折号说明。区域外字节保持不变。

## 受管理 log

`Wiki/log.md` 使用相同边界规则。计划把本次变更按创建、更新分类，写入计划创建日期的 `## YYYY-MM-DD` 下；同日已有受管理条目时在同日标题后追加，日期块保持新到旧。每项使用目标标题、链接和请求 summary。计划生成后重复 apply 不重复日志。

## 保存与恢复

apply 取得 Vault 独占锁并依次执行：

1. 核对计划摘要、schema、程序版本、有效期限、Vault ID、配置和准入哈希。
2. 核对全部目标、index、log 的当前哈希和全部精确来源版本。
3. 把每个目标的原字节与新哈希持久化到用户状态目录的 `knowledge-progress.json`。
4. 写入 Vault 的 `.kb/runtime/knowledge-pending.json`，使读取和其他写入拒绝混合状态。
5. 逐个替换目标并核对最终哈希。
6. 持久化 `result.json` 作为完成依据，清理 pending/progress，再尝试删除可重建 `catalog.json`。

写入失败时按相反顺序恢复：仍是计划新内容的文件恢复原字节或删除；已经是原内容的文件跳过；与新旧都不同的文件保留并返回 `vault_needs_recovery`。进程退出后，重试同一 apply：没有完成回执则先恢复再重新复核计划；已有完成回执则只清理遗留状态并返回原结果。

来源保存 pending 与知识保存 pending 互斥。存在任一 pending 时，query、review、lint、cache rebuild、source verify 和其他写入返回 `vault_needs_recovery`；status 与 operation show 仍可用于诊断。

## 非目标

- 不允许删除、移动或任意改写 source record。
- 不接受对 `Wiki/index.md`、`Wiki/log.md` 或 `.objects/` 的直接请求写入。
- 不调用 LLM，不从 prose 推断目标文件。
- 不实现 WebUI、GUI、MCP 或 HTTP 适配器。
- 不承诺断电级持久性；保证覆盖受控进程失败与重启恢复。

