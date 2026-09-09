# 从空目录完成首次使用测试

本教程面向第一次使用 Knowledge-Brain 的用户。完成后，你会得到一个可读写的 Vault，并亲自验证初始化、准入配置、来源保存、跨目录查询、增量更新和可选 BM25F 索引。

全程只使用已安装的 `kb` 和普通文件编辑器，不使用源码目录中的开发二进制或项目测试脚本。命令中的 `<VAULT>` 表示你选择的 Vault 绝对路径，例如 macOS/Linux 的 `$HOME/Documents/Knowledge-Vault` 或 Windows 的 `$HOME\Documents\Knowledge-Vault`。

## 1. 确认 CLI 可用

```text
kb version
```

命令应显示应用版本和 schema 版本。若系统找不到 `kb`，先按 [README 安装说明](../../README.md#安装)完成安装和 `PATH` 配置，再继续。

## 2. 准备空目录

`kb init` 只接受不存在或完全为空的目录。如果要重新使用曾经初始化过的路径：

1. 运行 `kb vault list`，检查该路径是否仍有注册记录。
2. 如果存在，运行 `kb vault unregister <vault-id>`。这只解除本机注册，不删除 Vault 文件。
3. 使用文件管理器检查路径并删除确认不再需要的内容，或者改用一个新的空目录。

删除目录内容不可撤销。不要把命令中的 `<VAULT>` 原样复制，也不要对主目录、磁盘根目录或未核实的变量执行递归删除。

## 3. 初始化 Vault

```text
kb init "<VAULT>"
```

检查输出：

- `created_files` 列出 `admission.yml`、`KB.md`、`Wiki/` 和 `.kb/` 下的基础文件；
- `root` 是刚才指定的目录；
- `vault_id` 非空；
- `warnings` 为空。

再查看初始化状态：

```text
kb status --vault "<VAULT>"
kb doctor --vault "<VAULT>"
```

此时 `status` 应显示配置有效、schema 为 `v1.0`、准入目录数量为 0。初始化不会创建 Git 仓库，也不会替你创建个人主题目录。

## 4. 创建主题目录并配置准入

使用文件管理器或编辑器在 Vault 根目录创建两个测试目录：

```text
Research/
Tools/
```

这两个名字只用于本教程，不是 Knowledge-Brain 的固定分类。实际 Vault 可以使用你自己的一级主题目录。

打开 `<VAULT>/admission.yml`，将内容改为：

```yaml
schema_version: "v1.0"

directories:
  - id: "research"
    path: "Research"
    enabled: true
  - id: "tools"
    path: "Tools"
    enabled: true
```

保存后执行：

```text
kb config validate --vault "<VAULT>"
kb config admission list --vault "<VAULT>"
```

验证应成功，准入列表应显示 `research` 和 `tools` 均已启用。也可以用 `kb config admission add` 逐项修改；直接编辑 YAML 用于验证准入清单确实可由人读写。

## 5. 写入第一份来源笔记

用普通编辑器创建 `<VAULT>/Research/search-test.md`：

```markdown
# 本地搜索测试

Knowledge-Brain 使用普通 Markdown 保存可迁移知识。

唯一核验词：KB-USER-FLOW-2026
```

文件位于已启用的主题目录中，但此时还没有保存到 Wiki 来源记录。明确允许本次入库后，执行一次：

```text
kb source save --vault "<VAULT>" --yes
```

`--yes` 表示本次保存已经得到授权，因此不再要求额外确认。检查输出确认本次变化已保存；不要只根据退出码推断入库内容。

## 6. 查询并核对保存结果

先做相关性查询，再用唯一核验词做精确查询：

```text
kb query "本地搜索" --scope sources --vault "<VAULT>"
kb query "KB-USER-FLOW-2026" --scope sources --exact --vault "<VAULT>"
kb source verify --vault "<VAULT>"
```

检查结果：

- 两次查询都能找到 `Research/search-test.md` 对应的已保存来源；
- 精确查询报告 `match_mode: exact` 和 `backend: direct`；
- 来源校验没有损坏或缺失对象。

查询读取的是已保存版本。主题目录中尚未再次保存的修改不应提前出现在来源查询中。

## 7. 验证跨目录搜索

创建 `<VAULT>/Tools/cli-test.md`：

```markdown
# CLI 测试

命令行工具与研究笔记共享同一个 Vault。

跨目录核验词：PORTABLE-CROSS-DIR
```

保存并查询：

```text
kb source save --vault "<VAULT>" --yes
kb query "PORTABLE-CROSS-DIR" --scope sources --exact --vault "<VAULT>"
```

结果应指向 `Tools/cli-test.md`，证明查询范围来自准入清单，而不是某个写死的主题目录。

## 8. 验证增量修改

在 `Research/search-test.md` 末尾增加：

```markdown

增量核验词：INCREMENTAL-SOURCE-UPDATE
```

保存文件后，先查询尚未入库的修改：

```text
kb query "INCREMENTAL-SOURCE-UPDATE" --scope sources --exact --vault "<VAULT>"
```

此时不应命中。然后执行一次保存并再次查询：

```text
kb source save --vault "<VAULT>" --yes
kb query "INCREMENTAL-SOURCE-UPDATE" --scope sources --exact --vault "<VAULT>"
kb source verify --vault "<VAULT>"
```

第二次查询应命中，来源校验仍应通过。

## 9. 验证可选 BM25F 索引

默认 direct 搜索不依赖索引。切换到 BM25F 并建立索引：

```text
kb config set search.mode bm25 --vault "<VAULT>" --yes
kb cache rebuild --vault "<VAULT>"
kb query "本地 Markdown 知识" --scope sources --strict-backend --vault "<VAULT>"
```

查询结果应报告 `backend: bm25f`，且不应出现索引缺失或过期错误。

随后再次修改任意已准入来源，并运行 `kb source save --yes`。来源保存会使旧索引失效：

```text
kb query "本地 Markdown 知识" --scope sources --strict-backend --vault "<VAULT>"
kb query "本地 Markdown 知识" --scope sources --vault "<VAULT>"
```

第一条命令应明确报告索引需要重建；第二条命令应警告后回退到 direct，而不是让查询不可用。重建后再次严格查询：

```text
kb cache rebuild --vault "<VAULT>"
kb query "本地 Markdown 知识" --scope sources --strict-backend --vault "<VAULT>"
```

结果应重新使用 `bm25f`。BM25F 的完整匹配和失效规则见[搜索规则](../reference/search.md)。

## 10. 验证重新打开后的状态

关闭当前终端并打开一个新终端，然后执行：

```text
kb vault list
kb status --vault "<VAULT>"
kb query "KB-USER-FLOW-2026" --scope sources --exact --vault "<VAULT>"
```

Vault 注册、配置、来源记录和查询结果应保持有效。`.kb/cache/` 可以删除并重建；`Research/`、`Tools/`、`admission.yml` 和 `Wiki/` 才是本次测试需要保留和检查的内容。

## 完成标准

只有以下项目都由测试者实际观察到，才算首次使用测试完成：

- 空目录能够初始化，且没有自动创建 Git 仓库或个人分类；
- 人工编辑的 `admission.yml` 能通过校验；
- 明确入库只需一次 `kb source save --yes`；
- 未保存的来源修改不会提前出现在查询中；
- 保存后可跨主题目录查询，并可核对来源完整性；
- direct 查询不依赖缓存；
- BM25F 可建立、能识别过期索引、可回退并可重建；
- 新终端中仍能重新找到并查询 Vault。

任何一步的实际输出与上述结果不符时，保留完整命令、输出、操作系统和 `kb version`，不要用内部脚本代替失败的用户路径。
