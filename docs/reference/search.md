# 搜索规则

## 范围与事实来源

| scope | 搜索内容 |
| --- | --- |
| `wiki`（默认） | `Wiki/research/`、`Wiki/articles/` 下 Markdown，以及 `Wiki/index.md` |
| `sources` | 来源记录正文、标题及记录引用的当前精确版本的原始对象 |
| `all` | 固定顺序返回 `wiki`、`sources` 两组，空组也保留 |

不搜索主题目录中尚未保存的文件，不把 `Wiki/log.md` 混入结果。旧对象参与完整性检查，但普通来源查询只搜索每条来源记录选中的版本；标记 `present: false` 的记录仍可检索。手工在 generated records 范围以外放置的 external-sources 文件不自动成为来源记录。

direct 模式每次查询读取真实文件并即时提取；Wiki 人工修改不需要先刷新缓存。来源原始对象在提取前核对 SHA-256，损坏返回 `source_integrity_failed`，不会从未保存的主题文件补齐。unsupported/metadata_only 对象没有正文命中，但来源标题及记录中的人工说明仍可命中。

## 匹配意图

查询默认使用 `match_mode=relevant` 发现相关结果；CLI 的 `--exact`、MCP 和 HTTP 的 `match_mode=exact` 改为区分大小写的 Unicode 字面量核验。CLI、MCP 和 HTTP 的查询响应都以 `match_mode` 报告实际采用的模式。

`exact` 去除查询两端空白后按完整字面文本匹配，不分词、不转小写、不调用模型，也不使用 BM25F 排序或解释。它不读取、构建或更新 BM25F 缓存，结果使用 `backend=direct`，`score_micros` 和 `explanation` 为空。

配置项 `search.mode` 只为 `relevant` 选择 direct 或 BM25F 后端，不改变 `exact` 的行为。

## Relevant：Direct 匹配与顺序

查询去除两端空白并转为小写。匹配完整短语及空白分隔的词项，不分词、不调用模型、不做语义推断；连续中文使用子串匹配。

Markdown 按 ATX 标题分段，围栏代码内的 `#` 不产生新章节。每段独立排序：

1. 完整短语出现次数，降序；
2. 命中的不同词项数，降序；
3. 标题/章节命中优先；
4. 可移植记录路径、原始行号，升序。

正文没有命中时，文档文件名/来源标题可产生元数据结果。返回每组最多 `limit` 条，默认 10，允许 1–100。摘要不超过 240 个 Unicode 字符，不跨到相邻章节。这是确定性的直接搜索，不是 BM25 分数。

结果中的 `path` 指向 Wiki 文档或来源记录；`content_path` 指向实际命中文本（文档、记录或原始对象）。`line_start` 是 Markdown 或文本从 1 开始的章节行号；HTML、EPUB 和 DOCX 使用 `location` 返回格式原生位置。元数据命中的两种位置都为空。`source_uri` 提供精确版本引用；`heading`、`title`、`snippet` 和 `match_count` 供 CLI 或未来 UI 展示。

direct 结果的 `backend` 为 `direct`，`match_count` 表示直接命中次数；`score_micros` 和 `explanation` 为空。

## Relevant：可选 BM25F

在配置中显式设置 `search.mode: bm25` 后，`kb cache rebuild` 会从实际 Wiki 和已保存来源生成 `.kb/cache/bm25.json`。索引格式使用 `schema_version: v1.0` 和 `indexer_version: bm25f-v1`。它是可删除、可重建的本地派生数据，不是知识或来源的备份。

BM25F 按标题、别名、章节标题、标签和正文分别统计词项，默认权重依次为 3.0、2.5、2.5、2.0 和 1.0；参数为 `k1=1.2`、`b=0.75`。英文和数字按连续词项处理；其他字母数字序列产生 1–3 字符 gram，支持不依赖外部分词器的中文检索。Markdown 仍以章节为结果块，来源对象沿用格式原生位置。

结果的 `backend` 为 `bm25f`，`score_micros` 是用于稳定排序的定点整数分数；`explanation` 给出规范化查询词项和各字段贡献。BM25F 不返回 direct 的出现次数，因此 `match_count` 为 0。分数只适合比较同一次查询中的结果，不是跨查询的相关性百分比。

重建索引时，路径和内容指纹未变化的文档保留原 generation；新增或变化文档重新提取，已删除文档从索引移除。查询前会核对所请求范围内的实际路径和内容指纹。索引缺失、损坏、版本不兼容或陈旧时，默认整次请求回退到 direct 并返回 warning；`scope=all` 不混用新旧后端。传入 `--strict-backend` 则返回 `index_stale`，不回退。该参数在 direct 模式中没有影响。

来源或知识保存成功后会删除目录和 BM25 缓存；BM25 模式需要再次运行 `kb cache rebuild` 才能恢复排序查询。在此之前仍可使用默认回退读取真实文件。

## 轻量目录与提取缓存

`kb cache rebuild` 始终生成 `.kb/cache/catalog.json`，包含 `schema_version: v1.0`、`indexer_version: catalog-v1` 及按 scope/path 排序的条目。条目只有路径、内容哈希、标题、标题列表和范围，不包含完整正文；来源条目的哈希针对来源记录 Markdown，来源对象哈希仍在记录内。只有启用 BM25 时才同时维护完整文本索引。

目录适合导航和后续索引构建，但不参与 direct 命中判定。当前 direct 路径不读取目录或提取缓存，因此其缺失、过期、格式损坏都不会改变查询结果。重建可能因实际文件不合法或缓存写入失败而报错；可在解决后重新运行。不能把目录缓存作为来源记录或原始对象的备份。

Embedding 与 rerank 不参与当前查询，状态统一见 [Roadmap](../../ROADMAP.md)。
