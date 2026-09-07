# 来源格式与保存语义

## 身份与文件位置

逻辑身份由准入 ID 和主题内的可移植相对路径组成：

```text
kb-source://<admission-id>/<relative-path>
kb-source://<admission-id>/<relative-path>?sha256=<64位小写十六进制>
```

URI 各路径段使用 UTF-8 百分号编码，`/` 保留为层级分隔符。逻辑身份不包含机器绝对路径。SHA-256 针对原始字节计算，不规范化换行或编码。

| 内容 | Vault 内位置 |
| --- | --- |
| 原始对象 | `Wiki/external-sources/.objects/sha256/<前两位>/<sha256>` |
| 来源记录 | `Wiki/external-sources/records/<前两位>/<逻辑 URI 的 SHA-256>.md` |
| 知识日志 | `Wiki/log.md` 的 managed 区域 |
| 可丢弃提取缓存 | `.kb/cache/extracted/<sha256>/builtin-text-v1-<media-type>.json` |

对象按内容去重，只创建新对象或核对已有对象，不覆盖损坏对象。来源记录是持久的发现基准，不依赖扫描缓存。重命名路径产生新的逻辑身份，相同哈希可提供“可能移动”的线索，但不自动迁移身份。

## Markdown 来源记录

记录使用 OKF v0.2 的 Reference 形式：`type: Reference`、`title`、逻辑 `resource`、`generated.by`（程序及版本）与 RFC 3339 `generated.at`；Knowledge-Brain 扩展位于 `kb.source`。这不是完整 OKF Producer Profile 的实现声明。

`kb.source` 保存 `source`（逻辑身份和精确哈希）、`title`、原始 `size`、`media_type`、`extraction_status`、`present`、`captured_at` 和去重后的 `versions`。历史版本引用原始对象，不依赖当前主题文件仍存在。

更新只替换受管理字段，保留其他 frontmatter 值及记录正文；来源记录 YAML 可能重新排版，注释和原始换行不保证保留。配置文件采用另外的无损编辑器，见[配置参考](configuration.md)。手工补充说明放在正文，不修改 `kb.source` 的身份、哈希或版本历史。

主题文件真正消失且仍处于启用、未排除的范围时，review 才报告删除。apply 设置 `present: false`，保留来源记录及所有旧对象；查询仍可引用这些已保存证据。禁用或排除不是删除授权。

## 内置提取器

`builtin-text`、版本 `v1` 实现统一 `Extractor` 接口；输入是已限制大小的字节与媒体类型，不接触文件系统或网络。

| 输入 | 处理 |
| --- | --- |
| `.md`、`.markdown` | UTF-8 Markdown，跳过 frontmatter，按 ATX 标题分段，保留原始行号 |
| `.txt` | UTF-8 文本 |
| `.yml`、`.yaml`、`.json`、`.csv` | 保留原文本；YAML/JSON 语法错误以警告报告 |
| 无效 UTF-8、含 NUL 的文本 | `metadata_only`，不伪造可搜索正文 |
| PDF 与其他格式 | `unsupported`，可按准入规则保留原始对象 |

成功提取状态为 `text_ready`。扩展名分类不等于自动准入；YAML/JSON/CSV 等需要相应 `include` 规则。HTML、EPUB、DOCX、PDF 文本提取与 OCR 的实施状态见 [Roadmap](../../ROADMAP.md)。

## 计划、保存和恢复

review 将 `plan.json` 和内容校验值保存在本机用户状态目录 `operations/<id>/`，不修改知识。计划包含来源版本、目标原内容哈希、完整新来源记录与日志、准入和读取配置哈希、Vault 身份、创建时间及程序版本。

apply 在 Vault 独占锁下重新核对输入，并依次保存对象、来源记录和日志。每次写文件前，将原内容和新哈希写入 `source-progress.json`；Vault 内 `source-pending.json` 指明尚未清理的操作。其他入口拒绝读取可能混合的知识状态。

保存失败或进程退出后，没有完成回执的操作会在重试时恢复旧文件，再复核计划。恢复只处理本操作记录的内容：文件仍为旧内容则跳过，仍为本次新内容才恢复，不匹配则保留并报告冲突。成功回执 `result.json` 是完成依据；回执已写入时，重试仅清理遗留标记并返回原结果，不重复日志。

提取缓存失败以 warning 返回，不撤销已保存知识。来源历史没有自动清理策略。计划和进度文件含私有路径及恢复所需内容，不应公开提交；未完成操作的恢复材料不能当作缓存删除。恢复保证针对进程中断，不等同于所有文件系统上的断电持久性，也不隔离同一账户下的恶意并发写入。

## 完整性检查

`kb source verify` 核对记录引用的全部历史对象，按对象去重。哈希不符或对象缺失报告 `fail`；读取预算不足报告 `not_checked`。它不修复或删除对象。记录损坏或身份与路径不一致时，先报告记录错误，不猜测替代来源。
