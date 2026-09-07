# Knowledge-Brain 设计方案

- 状态：已确认，按分阶段计划实施
- 日期：2026-09-07
- 产品名称：Knowledge-Brain
- 命令名称：`kb`

## 1. 产品定位

Knowledge-Brain 是一个本地优先、文件优先、Agent 中立的个人知识库框架。它把用户已有的主题目录作为待整理来源，把可长期维护的知识保存为普通 Markdown，并通过单一的 `kb` 程序向人、AI Agent、未来 WebUI 和桌面 GUI 提供一致的操作能力。

Knowledge-Brain 的核心价值是：

- 用户始终能用普通编辑器读取和修改自己的知识。
- Markdown、YAML 和 JSON 是数据真相，不依赖专有数据库才能恢复。
- 来源原文、研究过程和稳定知识分层保存。
- AI 只能提出修改建议；保存必须经过可检查的计划。
- 搜索从简单、可解释的 Markdown 检索开始，BM25 是完整但可选的增强能力。
- Windows、macOS 和 Linux 使用同一套数据结构和核心行为。
- Codex、Claude Code、Gemini CLI、OpenCode 或其他 Agent 都只是可替换入口。

### 1.1 目标

Knowledge-Brain 必须支持：

1. 在新目录中创建最小知识库结构，或安全采用已有目录。
2. 通过人可读的 `admission.yml` 指定哪些一级主题目录需要进入知识整理流程。
3. 发现来源变化，保存不可修改的来源版本，并形成可追溯的研究和文章。
4. 直接搜索实际 Markdown；用户启用 BM25 后提供更好的本地全文检索。
5. 让 CLI、MCP 和 HTTP 共用同一应用层、权限规则和错误语义。
6. 在保存中断、缓存损坏或计划过期时保护用户已有内容。
7. 允许 Vault 整体复制、校验备份并迁移到另一种操作系统。

### 1.2 非目标

初始正式版本不包含：

- 内置 LLM 或特定模型 API 客户端
- WebUI 或桌面 GUI
- Embedding、向量数据库或 rerank
- PDF 正文提取、OCR、语音转写和复杂 Office 版式还原
- 自有云服务或实时多端同步
- 自动 Git 初始化、提交、拉取或推送
- 通用 Skill 市场
- 后台 AI 自动整理
- 整个 Vault 的删除命令

WebUI、GUI、Embedding、rerank 和新的内容提取器是未来扩展，不改变 Markdown 数据真相和应用层接口。

## 2. 核心原则

### 2.1 文件是数据真相

用户知识以 Markdown、YAML 和 JSON 保存。搜索索引、提取结果、运行日志和进度记录都可以删除并重建。任何缓存都不得成为恢复知识所必需的唯一数据。

### 2.2 产品与用户数据分离

Knowledge-Brain 产品仓库包含 Rust 源码、Schema、模板、Skill 和文档。用户 Vault 只包含用户内容、可移植配置和本机运行数据。产品仓库不得预置个人主题目录，Vault 模板不得出现个人路径或示例目录，例如 `AI-Toolkit/`。

### 2.3 确定性核心与推理分离

Rust 核心负责路径、来源、校验、搜索、修改计划、保存和恢复。Agent 负责判断、比较和起草内容。Agent 的建议必须通过核心校验，不能直接获得不受约束的 Wiki 文件写入能力。

### 2.4 所有入口共享业务逻辑

CLI、MCP、HTTP、未来 WebUI 和 GUI 只负责输入输出适配。它们不得分别实现来源处理、搜索、权限、写入或恢复逻辑。

### 2.5 明确降级

可选能力不可用时必须报告真实状态：

- BM25 缺失或损坏时回退到直接 Markdown 搜索。
- 提取器不能可靠取得正文时报告“仅保存原文件”。
- 缓存更新失败时保留已经保存的知识，并标记缓存过期。
- 只完成部分检查时不得声称整个知识库正常。

## 3. 系统架构

```text
人 / Agent / WebUI / GUI
          │
          ├── CLI
          ├── MCP stdio
          └── HTTP + SSE
                  │
                  ▼
          Knowledge-Brain 应用层
                  │
       ┌──────────┼──────────┐
       ▼          ▼          ▼
   Vault 文件   来源提取   搜索实现
       │          │       direct / BM25
       └──────────┴──────────┘
                  │
                  ▼
          Markdown / YAML / JSON
```

CLI 直接调用应用层，不要求启动服务。`kb mcp` 和 `kb serve` 在同一个可执行文件中启动对应适配器。HTTP 服务是未来 WebUI、GUI 和局域网客户端的共享接口。

## 4. 用户 Vault

### 4.1 目录结构

```text
Vault/
├── admission.yml
├── KB.md
├── <用户定义的主题目录>/
├── Wiki/
│   ├── index.md
│   ├── log.md
│   ├── external-sources/
│   │   └── .objects/
│   │       └── sha256/
│   ├── research/
│   └── articles/
└── .kb/
    ├── config.yml
    ├── config.local.yml
    ├── schemas/
    ├── cache/
    └── runtime/
```

顶层主题目录完全由用户命名。Knowledge-Brain 不创建通用主题目录，也不把个人目录名称写入模板。

### 4.2 三层 Wiki

`Wiki/external-sources/` 保存人可读的来源记录及其不可修改的原始版本。

`Wiki/research/` 保存围绕问题形成的阶段性研究、比较和未定结论。Research 默认是 `draft` 或其他非稳定状态，不因文件存在而视为事实。

`Wiki/articles/` 保存当前可复用的知识。Article 可以使用适合内容的 OKF 类型，不强制所有文件使用同一种 `type`。

来源可以直接形成 article，也可以先形成 research。系统不强制 research 必须先于 article。

### 4.3 `Wiki/index.md`

`Wiki/index.md` 是人可读的导航入口，列出 Wiki 中的重要页面、简短说明和分类。Knowledge-Brain 只修改带明确边界标记的受管理区域，保留用户在其他区域写下的内容。

中小规模知识库可以只依赖此文件和直接 Markdown 搜索。`index.md` 不是机器索引的导出副本，也不要求列出每个内部缓存项。

### 4.4 `Wiki/log.md`

`Wiki/log.md` 只记录已经成功保存的知识变化，例如新增来源记录、更新研究、替代文章或人工确认。查询、doctor、lint、缓存重建和失败的保存不进入此文件。

详细机器事件写入 `.kb/runtime/`，不得把调试信息混入人类知识日志。

### 4.5 不设置固定 `hot.md`

临时上下文、近期命中和运行状态保存在 `.kb/cache/context.json` 等可重建文件中。用户需要记录当前关注主题时，应创建普通 research 文档，而不是依赖框架固定的 `hot.md`。

## 5. 配置模型

### 5.1 配置文件职责

| 位置 | 职责 | 是否可迁移 |
| --- | --- | --- |
| `admission.yml` | 准入目录清单 | 是 |
| `.kb/config.yml` | Vault 的共享配置和稳定 `vault_id` | 是 |
| `.kb/config.local.yml` | 本机覆盖、监听和服务设置 | 否 |
| 用户级配置 | 多 Vault 注册和用户默认值 | 否 |
| 环境变量 | 临时运行覆盖 | 否 |
| CLI 参数 | 当前命令覆盖 | 否 |

有效配置的优先级为：

```text
CLI 参数 > 环境变量 > Vault 本机配置 > Vault 共享配置 > 用户配置 > 内置默认值
```

`admission.yml` 是准入目录的唯一数据源，不在其他配置层重复维护目录清单。

### 5.2 `admission.yml`

```yaml
schema_version: "v1.0"

directories:
  - id: work-notes
    path: Work-Notes
    enabled: true
    include:
      - "**/*.md"
      - "**/*.txt"
      - "**/*.pdf"
    exclude:
      - "**/.git/**"
      - "**/tmp/**"

  - id: reading
    path: Reading
    enabled: false
```

准入规则：

- `path` 必须是 Vault 根目录下的一级相对目录。
- 不允许绝对路径、`..`、`Wiki/`、`.kb/` 或符号链接目标。
- `id` 是稳定标识；目录改名不要求改变 `id`。
- `enabled: true` 的目录才由 `kb review` 检查。
- `include` 和 `exclude` 只控制该准入目录内的文件筛选。
- 不跟随符号链接或 Windows junction。
- 隐藏文件默认不处理；需要时由用户显式开启。
- 单文件大小、单次文件数和单次读取总量使用可配置上限。
- `admission.yml` 不记录待办队列或处理进度。

### 5.3 配置命令

```text
kb config show [--sources]
kb config get <key>
kb config set <key> <value> [--local|--user]
kb config unset <key> [--local|--user]
kb config validate

kb config admission list
kb config admission add <id> <top-level-directory>
kb config admission enable <id>
kb config admission disable <id>
kb config admission remove <id>
```

`kb config` 可以直接管理 `.kb/config.yml`、`.kb/config.local.yml`、用户配置和 `admission.yml`。修改前必须验证目标层和最终有效配置。配置编辑必须保留未知字段、字段顺序和目标节点之外的注释；无法安全保留时拒绝自动修改并输出人工修改建议。命令提供差异预览，不能借一次设置重写无关内容。

## 6. 格式与版本

Knowledge-Brain 使用三种独立版本：

```text
程序版本：SemVer，例如 1.2.0
Knowledge-Brain schema_version：例如 "v1.0"
OKF okf_version："0.2"
```

配置使用 YAML，Wiki 和来源记录使用 Markdown，操作计划、事件、协议和能力说明使用 JSON，连续机器日志使用 JSONL。

HTTP URL 不包含 `/v1`。每个机器响应在内容中携带 `schema_version`。

### 6.1 兼容规则

| Vault 格式 | 程序行为 |
| --- | --- |
| 当前程序完整支持 | 允许查询和写入 |
| 较旧但可迁移 | 允许诊断、备份和读取；写入前要求迁移 |
| 同一主版本但更新的小版本 | 只允许 status、doctor 和 backup，提示升级程序 |
| 更新的主版本 | 不解释未知知识结构，只允许诊断和原样备份 |
| 格式损坏 | 停止写入并报告具体文件和字段 |

替换 `kb` 程序不会自动迁移 Vault。迁移通过 `kb migrate plan` 生成计划，并由 `kb apply <operation_id>` 执行。迁移执行前必须创建并验证备份，不提供跳过迁移备份的参数。

## 7. 来源模型

### 7.1 来源身份

准入文件的逻辑地址为：

```text
kb-source://<admission-id>/<relative-path>
```

精确来源版本增加内容哈希：

```text
kb-source://<admission-id>/<relative-path>?sha256=<hash>
```

当前路径可以变化，带 SHA-256 的版本身份不可变化。旧来源证据不得因文件改名、删除或重新抓取而自动删除。

### 7.2 来源保存

来源采用“人可读记录 + 内容寻址原文件”结构：

```text
Wiki/external-sources/source-example.md
Wiki/external-sources/.objects/sha256/ab/abcdef...
```

来源 Markdown 保存标题、逻辑地址、精确版本、媒体类型、抓取时间、提取状态和引用信息。`.objects/` 保存原始字节。相同字节只需保存一份对象，但可以由多个来源记录引用。

### 7.3 来源发现

`kb review`：

1. 只遍历 `admission.yml` 中启用的目录。
2. 发现新增、修改、删除和可能移动的文件。
3. 使用大小和修改时间进行快速筛选，以 SHA-256 作为最终判断。
4. 提取可供审查的正文和结构。
5. 返回变化、来源摘要及已有 Wiki 关联。
6. 不创建来源记录，不修改 Wiki，不追加 `Wiki/log.md`。

`kb review` 可以更新 `.kb/cache/` 中的发现信息。Agent 生成计划后，`kb apply` 在保存前重新计算来源哈希；来源已变化时整个计划失效。

### 7.4 文件变化

来源文件被删除时，历史原始对象和引用保留，来源记录标记原位置不存在。系统不自动删除相关知识。

相同哈希出现在新路径时只提示“可能移动”。两个不同文件可能具有相同内容，因此系统不能只凭哈希自动认定改名。

### 7.5 URL 来源

URL 不进入 `admission.yml`。用户显式执行：

```text
kb source capture https://example.com/article
```

该操作只在用户调用时联网，记录最终 URL、重定向链、时间、HTTP 状态和内容类型，不运行网页 JavaScript，也不使用浏览器 Cookie。请求必须限制下载大小、重定向次数和超时时间；默认禁止访问本机、回环及局域网地址。抓取结果先形成来源保存计划，再通过 `kb apply` 写入。

## 8. 内容提取

来源提取使用三个明确结果：

| 状态 | 含义 |
| --- | --- |
| `text_ready` | 已取得可搜索、可引用的正文 |
| `metadata_only` | 已保存原文件和元数据，但没有可靠正文 |
| `unsupported` | 当前版本不能安全处理 |

初始正式版本的内置能力：

- Markdown、纯文本：完整正文和标题结构。
- YAML、JSON、CSV：作为结构化文本读取，不推断业务含义。
- HTML、EPUB：提取正文、标题和链接。
- DOCX：提取段落、标题和表格文本，不承诺还原版式。
- PDF、图片、音频、视频、PPTX、XLSX：只保存原文件和元数据。

PDF 正文提取和 OCR 是未来扩展。扩展必须通过同一个 `Extractor` 接口接入，不能成为读取 Markdown、HTML、EPUB 或 DOCX 的前置依赖。

提取结果保存在 `.kb/cache/extracted/<source-sha256>/<extractor-id>-<version>.json`，属于可重建缓存。提取器升级可以产生新的提取结果，但不能改变来源原始对象。

### 8.1 `Extractor` 扩展点

多种文件格式需要多个实现，因此核心定义 `Extractor` 接口。输入包括来源字节、媒体类型和大小限制；输出包括正文块、标题层级、页码或位置、警告及提取器版本。

内置提取器运行在 Rust 进程中。未来外部提取器使用受限 JSON 子进程协议：不通过 shell 拼接命令，清除密钥类环境变量，限制运行时间和输出量，并使用仅当前用户可访问的随机临时目录。外部提取器不能直接修改 Wiki。

## 9. Wiki 文档与 OKF v0.2

Knowledge-Brain 生成和管理的 Wiki 文档遵守 OKF v0.2，并增加严格的 Knowledge-Brain Producer Profile。

主要规则：

- `generated` 是包含 `by`、`at` 等字段的对象，不是布尔值。
- `verified` 是验证事件或事件列表，不是布尔值。
- `generated` 描述内容由谁产生；`verified` 描述谁按何种方式确认，两者相互独立。
- `stable` 表示内容适合当前复用，不等于已经验证。
- Knowledge-Brain 扩展统一放在 `kb:` 下，避免污染 OKF 顶层字段。
- 文档只存储 `kb.supersedes`，被替代关系从现有数据反向推导。
- 未知 OKF 字段必须保留。
- 链接使用标准 Markdown 链接，不依赖特定编辑器的 Wikilink。

人可以直接编辑 `Wiki/`。Knowledge-Brain 在下一次 lint、review 或 apply 时验证人工修改，不因为文件不是由 Agent 生成就拒绝它。原始来源对象由系统管理，不允许就地编辑。

## 10. 搜索设计

### 10.1 搜索层级

Knowledge-Brain 提供三层搜索能力：

1. `Wiki/index.md`：面向人的导航索引。
2. `.kb/cache/catalog.json`：轻量文件和标题目录。
3. 可选 BM25F：完整本地全文索引。

用户明确选择搜索模式，不根据文档数量自动切换：

```yaml
search:
  mode: direct   # direct 或 bm25
```

### 10.2 直接 Markdown 搜索

`direct` 模式查询实际 Markdown，而不是只读缓存。目录缓存缺失、过期或损坏时，查询仍然可用。该模式适合少量文档，也是所有高级索引失效时的可靠回退路径。

### 10.3 BM25F

BM25F 是可选能力，但实现必须完整：

- 按标题、标题层级、标签、别名和正文分字段计分。
- 以 Markdown 标题区段作为主要检索块。
- 中文及无空格文本使用 1–3 gram；其他文本使用适合语言的词元规则。
- 支持增量新增、更新和删除。
- 索引记录源文件哈希及索引器版本。
- 索引过期或损坏时默认回退到直接搜索。
- 严格模式下可以选择报错而不回退。
- 所有结果可以解释命中字段、区段和分数来源。

Embedding 和 rerank 不属于初始版本，也不作为 BM25 正常工作的必要条件。

### 10.4 Wiki 与来源分开

```text
kb query "检索词"                  # 默认只查 Wiki
kb query "检索词" --scope sources  # 查来源记录和提取正文
kb query "检索词" --scope all      # 分组显示两类结果
```

`--scope all` 不把来源和已形成知识混成同一排名。搜索只负责确定性检索，不自动调用 LLM 综合答案。

## 11. 审查、计划与保存

### 11.1 标准流程

```text
kb review
    ↓
Agent 按需读取来源片段
    ↓
Agent 提交结构化修改建议
    ↓
核心验证并生成 operation_id
    ↓
用户查看精确差异
    ↓
kb apply <operation_id>
```

Agent 不能直接修改 Wiki。CLI、MCP 和 HTTP 都必须使用相同的计划格式和应用流程。

计划记录：

- 涉及文件及其原始 SHA-256
- 来源精确版本
- 当时的 `admission.yml` 和相关配置摘要
- 完整目标内容或可确定生成目标内容的数据
- 计划创建时间和兼容版本
- 唯一 `operation_id`

执行前重新核对全部文件和配置。任何输入发生变化时，系统不保存任何计划内容，并要求重新生成计划。

### 11.2 保存保证

面向用户的保证是：

> 所有知识文件全部保存成功才算完成；中途失败时恢复原来的文件。

保存步骤包括准备新文件、记录原文件状态、逐个替换目标、核对最终内容、记录完成结果，最后更新缓存。运行中断后，下次启动检查未结束操作：

- 全部目标已经符合计划：补记为成功。
- 只有部分目标被替换：恢复原文件。
- 文件系统故障导致无法恢复：阻止查询和写入，并报告 `vault_needs_recovery`。

知识文件保存成功而缓存更新失败时，知识不回退；缓存标记过期，查询改用直接 Markdown 搜索。

### 11.3 并发与重复请求

查询使用共享读取锁，修改使用独占写入锁。CLI、MCP 和 HTTP 使用 Vault 内同一把跨平台锁。锁由操作系统持有，进程退出后释放；旁边的说明信息仅用于显示当前命令、`operation_id` 和开始时间。

计划生成期间不长期占用写入锁。`kb apply` 取得锁后重新验证计划。

`operation_id` 同时防止网络重试导致重复保存：

- 已完成：返回原完成记录。
- 正在执行：返回当前进度。
- 上次中断：进入恢复流程。
- 相同 ID 对应不同内容：拒绝执行。

## 12. 校验与诊断

`kb lint` 检查知识结构，必须是确定性、离线和只读操作。它验证：

- Markdown 和 frontmatter 是否可解析
- Knowledge-Brain 管理文档是否符合严格 Producer Profile
- 来源版本和引用是否存在
- Markdown 链接是否可解析
- `supersedes` 是否指向合法对象
- 路径、大小写、Unicode 和跨平台文件名是否冲突

普通 OKF 文档只需满足 OKF 的宽松底线；由 Knowledge-Brain 管理的文档使用严格规则。语义矛盾只能由 Agent 提示，不能由 lint 假装判定事实真伪。

`kb lint` 不提供自动修复。需要修改文件时生成计划并由 `kb apply` 执行。

`kb doctor` 检查运行环境、配置、权限、锁、缓存、恢复状态、端口和宿主集成。`kb source verify` 核对来源原始字节。三者职责不得混合，也不生成含糊的健康分数。

## 13. 命令面

单一 `kb` 可执行文件提供：

```text
kb init
kb adopt
kb status
kb doctor
kb config
kb review
kb source
kb query
kb plan
kb apply
kb lint
kb recover
kb cache
kb backup
kb migrate
kb vault
kb skills
kb mcp
kb serve
kb paths
kb version
kb capabilities
kb update
```

### 13.1 初始化与采用

`kb init` 只接受不存在或空目录，创建最小 Vault，不创建示例主题、Git 仓库、BM25 索引或局域网服务。

`kb adopt` 用于已有目录，先生成采用计划，不移动已有文件，不自动加入准入目录，不自动改写 Markdown，也不自动转换 OpenKnowledge、Obsidian 或其他产品格式。

### 13.2 Vault 注册

用户级注册表将稳定 `vault_id` 映射到当前机器路径。移动 Vault 后可以重新绑定路径。注册表不写入 Vault，也不随备份迁移。

## 14. CLI、MCP 和 HTTP

### 14.1 共享应用接口

`kb-core` 定义领域规则，`kb-app` 提供应用操作。CLI、MCP 和 HTTP 只转换参数及结果。任何入口不得自行访问 Vault 绕过应用层。

### 14.2 机器响应

```json
{
  "schema_version": "v1.0",
  "data": {}
}
```

错误响应：

```json
{
  "schema_version": "v1.0",
  "error": {
    "code": "plan_stale",
    "message": "知识库内容已变化，未保存任何文件。",
    "retryable": false,
    "next_action": "重新运行 kb review 生成计划"
  }
}
```

稳定错误码至少包括：

- `invalid_config`
- `path_not_admitted`
- `plan_stale`
- `write_busy`
- `vault_needs_recovery`
- `restore_failed`
- `index_stale`
- `capability_unavailable`
- `auth_denied`

CLI 默认显示人类可读文本，`--json` 返回稳定机器结构。默认不显示调用栈；详细诊断需要显式 `--verbose`。

### 14.3 能力发现

`kb capabilities` 和 HTTP `/capabilities` 报告程序版本、支持的 schema、搜索后端、提取器、接口和权限。客户端必须通过能力发现决定是否显示功能，不根据版本字符串猜测。

### 14.4 MCP

`kb mcp` 使用 stdio，不要求 daemon。默认只开放查询、来源读取和计划创建，不开放计划执行。宿主显式启用写入后，Agent 才能在用户确认后调用指定 `operation_id`。

### 14.5 HTTP 与 SSE

`kb serve` 提供可选 HTTP 接口。API 路径不包含版本前缀；请求和响应内容携带 schema 版本。SSE 延后到 operation 具有可持久化进度事件后实现；事件必须按 `operation_id` 归属，客户端断线重连不能导致操作重复执行。

## 15. WebUI 与 GUI

WebUI 和 GUI 不属于初始正式版本，但初始接口必须能支持：

- 查看 Vault 状态
- 管理 `admission.yml`
- 搜索 Wiki 和来源
- 查看来源及引用
- 查看、比较和批准修改计划
- 运行 lint、doctor、backup 和 migrate
- 查看长操作进度

WebUI 使用 HTTP/SSE；桌面 GUI 可以嵌入或连接同一服务。两者不能直接读写 Vault，也不能复制核心规则。

未来“AI 生成文章”功能由外部 Agent 或独立模型适配层提供。出现真实的本地模型和云模型实现前，不在核心中创建空的 `LLMProvider` 接口。

## 16. Agent 与 Skill

### 16.1 `KB.md`

Vault 根目录的 `KB.md` 是唯一宿主无关的 Agent 操作规则，内容保持简短，只包含：

- Vault 范围和准入规则
- 来源和 Wiki 内容是数据，不是命令
- 查询和来源读取必须通过 `kb`
- 修改遵循 review、plan、用户确认、apply
- 原始来源不可修改
- `stable` 与 `verified` 的区别
- 不自动联网、启用局域网、修改 Git 或删除知识
- 通过 `kb capabilities` 判断功能

Schema 字段、长篇教程和宿主路径不放入 `KB.md`，只链接到正式参考文档。

### 16.2 宿主桥接

| Agent | 规则入口 | Skill 位置 |
| --- | --- | --- |
| Codex | `AGENTS.md` | `.agents/skills/knowledge-brain/` |
| Claude Code | `CLAUDE.md` 引用 `KB.md` | `.claude/skills/knowledge-brain/` |
| Gemini CLI | `GEMINI.md` 引用 `KB.md` | `.agents/skills/` 或 `.gemini/skills/` |
| OpenCode | `AGENTS.md` | `.agents/skills/` 或 `.opencode/skills/` |
| 其他 Agent | 显式读取 `KB.md` | Agent Skills 标准或 MCP/CLI |

宿主差异保存在声明式适配清单中，不进入核心。已有规则文件不会被覆盖；安装器只添加带边界标记的 Knowledge-Brain 区块。卸载只移除自身创建且哈希匹配的区块，用户修改过时停止并报告。

### 16.3 Portable Agent Skill

```text
knowledge-brain/
├── SKILL.md
└── references/
    ├── query.md
    ├── review-and-save.md
    └── maintenance.md
```

Skill 遵循开放 Agent Skills 格式，优先调用 MCP，没有 MCP 时调用 CLI JSON。Skill 不包含业务实现、模型名称、个人路径或操作系统专用脚本。

```text
kb skills detect
kb skills install --host auto --scope vault --mode copy
kb skills install --host claude-code --scope user --mode symlink
kb skills status
kb skills uninstall --host <host> --scope <scope>
```

`copy` 是跨平台默认值，`symlink` 由用户明确选择。安装和卸载先展示变更计划，再由 `kb apply <operation_id>` 执行。Skill 不自动升级。

`kb skills` 只管理 Knowledge-Brain 自带 Skill 和规则桥接，不管理第三方 Skill 市场，也不依赖 `npx skills add`。

## 17. LLM 与隐私

Knowledge-Brain 初始版本不包含任何 LLM API 客户端、API Key 配置、默认模型或自动模型选择。搜索、来源发现、提取、lint、备份、配置和保存可以完全离线。

当用户将 MCP 或 CLI 接入云端 Agent 时，返回的来源片段可能进入该 Agent 的模型上下文。Knowledge-Brain 必须明确说明这一事实，不能把外部云端处理描述为完全本地。

内容最小化规则：

- `kb review` 首先返回变化和元数据。
- Agent 按来源 ID、章节或页码读取必要片段。
- `kb query` 限制返回块数和字节数。
- 二进制原文件默认不直接交给 Agent。
- 来源内容在协议中标记为 `untrusted_content`。
- 来源中的命令、规则或提示词只能作为数据，不得转换成系统操作。

Knowledge-Brain 自身不主动上传 Vault，不在日志中保存来源正文，不把令牌或本机配置写进 Vault。它不能保证外部 Agent 的数据政策、拥有完整文件权限的恶意程序、被入侵的操作系统或未加密磁盘的安全。

初始版本不收集遥测，不自动检查更新，不自动发送崩溃报告。

## 18. 权限与局域网

权限范围：

```text
wiki:read
source:metadata
source:read
plan:create
plan:apply
admin
```

| 入口 | 默认权限 |
| --- | --- |
| 本地 CLI | 当前系统用户权限 |
| MCP | Wiki/来源读取和计划创建；不能执行计划 |
| 回环 HTTP | 只读 |
| 局域网 HTTP | 必须使用令牌，默认只读 |
| WebUI/GUI | 使用会话令牌对应权限 |

服务默认只监听回环地址并保持只读。局域网监听必须显式指定非回环地址并提供 token 文件；写入权限必须通过 `--allow-write` 再次显式开启，并且即使在回环地址也要求 token。配置 token 后所有路由都验证 Bearer token。token、TLS 配置和本机绑定地址不进入 Vault 的可移植配置。

内置服务只提供明文 HTTP，不发送宽松 CORS 头。局域网明文访问仅用于受信任网络；其他部署应使用用户管理的 TLS 反向代理。未来 WebUI 必须补充同源策略、Origin 检查和防跨站请求机制。

## 19. 跨平台规则

Knowledge-Brain 自己生成的文件必须满足 Windows、macOS 和 Linux 的共同限制：

- 配置中的路径使用 `/`，并相对于 Vault。
- 不生成 Windows 保留名、尾随空格、尾随句点或平台禁用字符。
- 生成 slug 限制长度，为冲突后缀保留空间。
- 拒绝大小写等价或 Unicode 规范化等价的两个知识路径。
- 管理文本使用 UTF-8、无 BOM、LF 换行。
- 来源原始字节不进行编码或换行转换。

已有来源文件在当前系统可读时可以处理；`kb doctor portability` 报告其跨平台问题，但不擅自重命名。

文件监听器只用于减少重复扫描，不是正确性前提。监听事件丢失后，显式命令必须通过实际文件状态发现变化。

用户级配置和状态目录使用操作系统标准位置，并允许环境变量覆盖。`kb paths` 显示所有解析后的实际路径。任何默认值不得包含开发者用户名或 macOS 专用目录。

## 20. 备份与多设备使用

### 20.1 可迁移内容

备份包含：

- `admission.yml`
- `KB.md`
- `Wiki/`
- `admission.yml` 中列出的所有主题目录，包括暂时禁用的目录
- `.kb/config.yml`
- `.kb/schemas/`

备份排除：

- `.kb/config.local.yml`
- `.kb/cache/`
- `.kb/runtime/`
- 用户级 Vault 注册表
- 用户级 Agent/Skill 安装记录
- Git 内部目录

### 20.2 标准备份

```text
kb backup create [--output <path.zip>]
kb backup verify <path.zip>
kb backup restore <path.zip> --target <empty-directory>
```

备份是普通 ZIP，包含 `manifest.json`。清单记录清单 `schema_version`、`vault_schema_version`、`vault_id`、创建时间、程序版本、来源证据完整性、目录，以及每个文件的相对路径、大小和 SHA-256。

默认包含来源原始对象。`--without-source-objects` 可以生成较小快照，但清单必须标记为“不包含完整来源证据”，不能显示为完整备份。

恢复只允许写入不存在或空目录，不覆盖现有 Vault，也不自动合并。恢复必须拒绝绝对路径、`../`、符号链接、junction、大小写冲突、Unicode 冲突和保留文件名。所有内容先解压到私有临时目录并校验，成功后再放入目标目录。

### 20.3 Git 与目录同步

Git 和 Dropbox、OneDrive、Syncthing 等工具均为外部可选能力。Knowledge-Brain 不自动同步或解决冲突。

建议 Git 忽略 `.kb/cache/`、`.kb/runtime/` 和 `.kb/config.local.yml`。来源对象是否使用 Git LFS 由用户决定。

多台机器同时修改同一同步目录时，文件哈希变化使旧计划失效。系统发现冲突副本时停止写入，不自动选择较新文件。需要多个客户端同时操作时，推荐由一台机器运行局域网服务，让所有客户端共享同一写入锁。

## 21. 恢复、缓存与运行数据

```text
kb recover status
kb recover <operation_id>
kb cache status
kb cache rebuild
kb cache clear
```

缓存可以按容量限制清理。完成且核对无误的恢复临时文件可以删除；未完成操作的恢复数据在恢复结束前不得删除。

操作计划超过配置保留时间后不能执行。机器事件日志按大小和保留天数轮换，不记录来源正文。来源原始对象和 `Wiki/log.md` 永不自动删除。初始版本不提供删除来源证据的 `kb gc`；`kb doctor storage` 只报告占用和孤立对象。

## 22. 定时检查

Knowledge-Brain 不安装后台任务，也不直接管理 cron、systemd、launchd 或 Windows Task Scheduler。外部调度器统一调用：

```text
kb review --json
```

定时运行只做确定性的只读检查，不自动调用 Agent、生成文章或执行计划。发现变化不是命令错误，JSON 返回 `changes_count`。CI 需要把变化视为失败时显式使用 `--fail-on-changes`。

调度器适配和通知可以在后续版本中通过独立适配层实现，不进入核心。

## 23. 产品仓库

```text
Knowledge-Brain/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── kb-core/
│   ├── kb-app/
│   ├── kb-protocol/
│   ├── kb-cli/
│   ├── kb-mcp/
│   └── kb-server/
├── schemas/
├── assets/
│   ├── vault-template/
│   └── host-adapters/
├── skills/
│   └── knowledge-brain/
├── tests/
│   ├── fixtures/
│   ├── integration/
│   └── e2e/
├── docs/
│   ├── architecture/
│   ├── guides/
│   ├── reference/
│   └── decisions/
├── reference/
├── README.md
├── LICENSE
└── SECURITY.md
```

### 23.1 Crate 职责

`kb-core` 保存领域类型、路径和 Schema 规则、OKF 校验、错误码以及确有多个实现的接口，不依赖 CLI、HTTP、MCP 或操作系统入口。

`kb-app` 实现全部应用流程以及本地 Vault、来源、直接搜索、BM25、提取、保存、恢复、备份和迁移。

`kb-protocol` 定义 JSON 请求、响应、分页、进度事件和协议 Schema，不包含业务判断。

`kb-cli` 生成唯一 `kb` 可执行文件，处理参数、人类输出、`--json`，并分派 MCP 和 HTTP 子命令。

`kb-mcp` 只处理 MCP stdio、权限映射和协议转换。

`kb-server` 只处理 HTTP、SSE、身份验证和网络安全。

搜索、提取、配置、来源、计划、恢复等先作为 `kb-app` 中的清晰模块存在。只有出现独立发布、明显编译隔离或第二个产品复用时才提升为独立 crate。

### 23.2 文档归属

- `README.md`：定位、快速开始、支持范围和参考项目。
- `docs/architecture/`：系统结构和主要数据流。
- `docs/reference/`：命令、配置、Schema 和协议。
- `docs/guides/`：面向任务的操作步骤。
- `docs/decisions/`：重大决定、替代方案和代价。
- Vault `KB.md`：Agent 每次操作需要的简短规则。
- 生成的协议和 Schema 参考：由生成器更新，不手工编辑。

`reference/` 只用于研究，不编译、不进入发布包、不成为运行依赖。

## 24. 安装、更新与发布

Knowledge-Brain 发布为一个自包含 `kb` 程序。Skill 模板、默认 Schema 和宿主适配清单编译进程序。

正式发布提供 Windows、macOS 和 Linux 的 x86_64 与 ARM64 压缩包，并附 SHA-256、签名和构建来源证明。官方压缩包不要求开发环境；`cargo install` 只是面向已有 Rust 环境的可选安装方式。包管理器属于分发适配，不进入核心。

Knowledge-Brain 不自动联网检查更新，不自动下载或替换自身。用户显式执行 `kb update check` 时才访问发布源；初始版本不实现自更新。

平台只有使用真实发布产物完成端到端测试后才能标记为“支持”。只完成编译或 `--help` 启动的平台标记为“预览”。

## 25. 分阶段实施

### 阶段 1：Vault 基础

范围：Rust workspace、单一 `kb` 入口、Schema、配置分层、`admission.yml`、init、adopt、config、status、doctor、跨平台路径和最小 Vault 模板。

验收：Windows、macOS 和 Linux 可以创建、采用、移动并重新打开同一个最小 Vault。

### 阶段 2：来源与读取

范围：准入目录变化发现、内容寻址来源、内置提取器、review、source、直接 Markdown 查询、轻量目录以及 Wiki/来源分开搜索。

验收：修改、删除和移动来源后，真实 CLI 准确报告变化；删除缓存后仍可查询。

### 阶段 3：知识形成与安全保存

范围：三层 Wiki、OKF v0.2 Producer Profile、plan、apply、写入锁、恢复、lint、index 和 log。

验收：从来源审查到文章保存完整走通；在每个文件替换步骤模拟中断，重新打开后得到完整旧状态或完整新状态。

### 阶段 4：Agent 与应用接口

范围：MCP、HTTP/SSE、权限令牌、回环和局域网、Portable Agent Skill，以及 Codex、Claude Code、Gemini CLI 和 OpenCode 适配。

验收：至少两种不同 Agent 的真实入口完成查询、生成计划和用户确认保存；HTTP 与 CLI 的有效状态、错误和刷新行为一致。

### 阶段 5：完整搜索、备份与发布

范围：完整 BM25F、备份/校验/恢复、迁移、发布包、签名和各平台真实产物测试。

验收：BM25 开关不改变知识真相，索引损坏能够回退，备份能在另一系统恢复，只有完成完整流程的平台才标记支持。

每个阶段通过实际用户流程验收后再进入下一阶段。代码存在、单元测试通过或页面能渲染都不能单独证明阶段完成。

## 26. 测试策略

### 26.1 测试层级

- 单元测试：路径、解析、分词、排序、Schema 和权限规则。
- 集成测试：来源、搜索、计划、保存、恢复、配置和缓存共同工作。
- 协议快照：CLI JSON、MCP 和 HTTP 的用户可见结构。
- 真实入口测试：运行构建后的 `kb`，不通过源码模式替代。
- 端到端测试：局域网、URL 抓取、跨系统备份恢复和发布产物。

### 26.2 关键失败场景

必须验证：

- 两个 CLI 或 CLI/HTTP 同时保存时只有一个写入。
- 计划生成后人工修改目标，旧计划拒绝且人工内容不丢失。
- 保存每一步被强制终止后能够恢复。
- 磁盘不足、只读文件和权限失败不留下半套知识。
- 知识保存成功但 BM25 更新失败时，直接搜索能找到新内容。
- 重复提交同一 `operation_id` 不重复追加日志或生成文章。
- 缓存删除、损坏和过期都不影响 Markdown 数据真相。
- ZIP 路径穿越、符号链接和大小写冲突被拒绝。
- CLI、MCP 和 HTTP 返回相同业务结果和稳定错误码。
- Vault 重开、程序重启和机器迁移后状态仍正确。
- 安装的 Skill 能被目标 Agent 真实发现并调用。

测试必须重新读取文件、重新运行命令或从真实客户端观察结果，不能只断言组件自己返回“成功”。

## 27. 参考方案分析

### 27.1 OpenKnowledge

保留：Markdown/OKF、MCP 工具、Agent 通过受控入口访问知识、CLI 和知识库结合。

不采用：完整 fork、Desktop/WebUI 耦合、Node/Bun 运行时、单一内容根和产品特定目录。Knowledge-Brain 是独立 Rust 项目，不依赖 OpenKnowledge 才能运行。

### 27.2 claude-obsidian

保留：来源原文不可修改、内容寻址、明确计划、失败恢复、分层知识、BM25 本地检索和可选能力诚实降级。

不采用：Claude 专属命名、Python 执行链、固定 15 个 Skills、`hot.md`、复杂 evidence ledger 和 Agent 独占 Wiki 写入。

### 27.3 LLM-Wiki

保留：来源与 Wiki 分离、`index.md` 优先、中小规模不强制向量检索、知识逐步复合增值。

调整：Wiki 不由 LLM 独占。人可以编辑，Agent 修改必须经过计划和确认。来源、research、articles 使用明确三层，而 overview/synthesis 是普通文章，不是固定系统文件。

### 27.4 Google Knowledge Catalog

保留：CLI 与 MCP 使用同一程序、结构化元数据、验证与实际执行分开、工具协议服务于多个 Agent。

不采用：Google Cloud 数据目录领域模型、TypeScript/Python 双实现和云端数据资产依赖。

### 27.5 OKF v0.2

采用 OKF 的开放 Markdown 语义、provenance、`generated`、`verified`、状态和来源表达。Knowledge-Brain 通过 `kb:` 扩展 Producer Profile，不修改 OKF 的既有字段含义。

## 28. 重大架构决定

以下决定在实现前以 proposed ADR 记录，落地后根据真实实现更新状态：

### 28.1 独立 Rust 核心

决定：使用独立 Rust workspace 和单一自包含程序。

替代方案：完整 fork OpenKnowledge；在 Python 脚本上扩展；保留 Node/Bun CLI。它们分别带来上游耦合、跨平台运行环境依赖或难以共享的业务逻辑，因此不采用。

### 28.2 Markdown 是唯一知识真相

决定：Markdown/YAML/JSON 保存可迁移数据，索引和数据库只作缓存。

替代方案：SQLite 或向量数据库作为主存储。该方案提高查询便利，但会使普通文件不再足以恢复知识，因此不采用。

### 28.3 `admission.yml` 是准入清单

决定：它只声明需要检查的一级主题目录，不记录待办状态。

替代方案：`pending.yml`、`triage` 队列或自动扫描整个 Vault。它们混淆配置和运行状态，或扩大未经授权的读取范围，因此不采用。

### 28.4 三层 Wiki

决定：固定 `external-sources`、`research`、`articles`，另有 `index.md` 和 `log.md`。

替代方案：按实体类型建立大量固定目录；只设一个平面 Wiki；增加固定 `hot.md`。大量目录限制主题表达，平面结构混淆知识成熟度，`hot.md` 把运行缓存写成知识，因此不采用。

### 28.5 确定性核心不内置 LLM

决定：Agent 通过 MCP、CLI JSON 或 HTTP 提交建议。

替代方案：核心直接调用特定模型；同时维护多个模型 SDK。它们会引入供应商绑定、密钥管理和不可控联网，因此不采用。

### 28.6 直接搜索为基础，BM25F 可选

决定：少量文档使用真实 Markdown 与轻量目录；BM25F 由用户显式启用且完整实现。

替代方案：只用文件名/grep；默认启用向量检索；根据文档数量自动切换。它们分别缺少排序质量、增加模型依赖或使行为不可预测，因此不采用。

### 28.7 计划与明确保存

决定：Agent 生成可检查计划，用户使用 `operation_id` 保存；保存失败恢复原文件。

替代方案：Agent 直接写 Wiki；依赖 Git 回滚；每个文件独立保存。它们无法统一覆盖无 Git Vault、跨入口并发和多文件一致性，因此不采用。

### 28.8 一个应用层，多种入口

决定：CLI、MCP、HTTP、未来 WebUI/GUI 共享 `kb-app`。

替代方案：CLI 调用常驻服务；每个入口实现自己的逻辑。前者增加运行依赖，后者造成行为漂移，因此不采用。

### 28.9 标准 ZIP 备份，不内置同步平台

决定：提供可校验 ZIP 和空目录恢复，Git/云盘由用户选择。

替代方案：只建议复制目录；自建云同步。前者不能验证一致性，后者显著扩大产品范围，因此不采用。

## 29. 整体验收标准

Knowledge-Brain 的初始正式版本只有同时满足以下条件才算完成：

1. 支持平台使用发布包完成真实初始化、采用、审查、计划、保存、查询、恢复和备份流程。
2. 删除全部缓存后，Markdown 知识、来源引用和直接查询仍然可用。
3. 人工编辑不会被旧计划或缓存覆盖。
4. 任何失败都不会被笼统显示为成功；知识、来源、索引和日志结果分别报告。
5. MCP 与 HTTP 不具有比同权限 CLI 更大的文件访问范围。
6. 局域网默认关闭；开启后默认只读且必须认证。
7. 使用两种不同 Agent 完成真实 Skill 发现和计划流程。
8. Vault 可以在另一操作系统恢复，并通过 Schema、来源哈希和链接检查。
9. 不安装 Node、Python、Java 或常驻服务也能运行全部核心功能。
10. 产品仓库和默认 Vault 中不存在个人路径、个人主题目录或参考项目运行依赖。

## 30. 后续步骤

本方案获得文档级复核后，下一步是编写分阶段实施计划。实施计划需要把每个阶段拆成可验证任务，标明真实入口测试和用户验收点。在实施计划再次获得明确授权前，不初始化 Rust workspace，不创建产品代码，不修改 Git 状态。
