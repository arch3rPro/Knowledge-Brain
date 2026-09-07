# 搜索规则

## 范围与事实来源

| scope | 搜索内容 |
| --- | --- |
| `wiki`（默认） | `Wiki/research/`、`Wiki/articles/` 下 Markdown，以及 `Wiki/index.md` |
| `sources` | 来源记录正文、标题及记录引用的当前精确版本的原始对象 |
| `all` | 固定顺序返回 `wiki`、`sources` 两组，空组也保留 |

不搜索主题目录中尚未保存的文件，不把 `Wiki/log.md` 混入结果。旧对象参与完整性检查，但普通来源查询只搜索每条来源记录选中的版本；标记 `present: false` 的记录仍可检索。手工在 generated records 范围以外放置的 external-sources 文件不自动成为来源记录。

每次查询读取真实文件并即时提取；Wiki 人工修改不需要先刷新缓存。来源原始对象在提取前核对 SHA-256，损坏返回 `source_integrity_failed`，不会从未保存的主题文件补齐。unsupported/metadata_only 对象没有正文命中，但来源标题及记录中的人工说明仍可命中。

## 匹配与顺序

查询去除两端空白并转为小写。匹配完整短语及空白分隔的词项，不分词、不调用模型、不做语义推断；连续中文使用子串匹配。

Markdown 按 ATX 标题分段，围栏代码内的 `#` 不产生新章节。每段独立排序：

1. 完整短语出现次数，降序；
2. 命中的不同词项数，降序；
3. 标题/章节命中优先；
4. 可移植记录路径、原始行号，升序。

正文没有命中时，文档文件名/来源标题可产生元数据结果。返回每组最多 `limit` 条，默认 10，允许 1–100。摘要不超过 240 个 Unicode 字符，不跨到相邻章节。这是确定性的直接搜索，不是 BM25 分数。

结果中的 `path` 指向 Wiki 文档或来源记录；`content_path` 指向实际命中文本（文档、记录或原始对象），`line_start` 为该文件从 1 开始的章节行号。元数据命中的行号为 null。`source_uri` 提供精确版本引用；`heading`、`title`、`snippet` 和 `match_count` 供 CLI 或未来 UI 展示。

## 轻量目录与提取缓存

`kb cache rebuild` 生成 `.kb/cache/catalog.json`，包含 `schema_version: v1.0`、`indexer_version: catalog-v1` 及按 scope/path 排序的条目。条目只有路径、内容哈希、标题、标题列表和范围，不包含完整正文；来源条目的哈希针对来源记录 Markdown，来源对象哈希仍在记录内。

目录适合导航和后续索引构建，但不参与 direct 命中判定。当前 direct 路径不读取目录或提取缓存，因此其缺失、过期、格式损坏都不会改变查询结果。重建可能因实际文件不合法或缓存写入失败而报错；可在解决后重新运行。不能把目录缓存作为来源记录或原始对象的备份。

`search.mode: bm25` 在能力不可用时明确警告并使用 direct，不伪装成 BM25。完整可选 BM25F、Embedding 与 rerank 的状态统一见 [Roadmap](../../ROADMAP.md)。
