# Knowledge-Brain Roadmap

本文档记录实现阶段和验证状态，是进度的唯一归属。产品语义由[设计文档](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)定义，具体开发步骤由各阶段的 implementation plan 定义；旧计划中保留的未勾选步骤不代表当前实现状态。

状态含义：

- **implemented**：代码和本地工作流已经实现。
- **in progress**：当前实施阶段。
- **planned**：设计已确定，尚未进入实现。
- **future**：保留方向，进入实施前仍需独立设计。

## Stage 1 — Vault 基础

**Status:** implemented；macOS 工作流已验证，Windows 和 Linux 等待原生 CI 证据。

- 最小 Vault 初始化与已有目录采用
- `admission.yml` 准入管理
- 分层配置和无损 YAML 修改
- Vault 注册、移动和重新绑定
- 操作计划、中断恢复和并发锁
- 状态、诊断和稳定 JSON 协议

实施计划：[Vault Foundation](docs/superpowers/plans/2026-09-07-vault-foundation.md)

## Stage 2 — 来源与读取

**Status:** implemented（定向本地测试）；跨平台验收仍待原生 CI。

当前实施计划：[Source Discovery and Direct Search](docs/superpowers/plans/2026-09-07-source-discovery-and-direct-search.md)

- 只遍历 `admission.yml` 中启用的目录
- 识别新增、变化、删除及可能移动的来源
- 使用 SHA-256 标识精确来源版本
- 保存内容寻址的原始对象和人类可读来源记录
- 提取 Markdown、文本及基础结构化文本
- 构建可重建的轻量目录
- 查询真实 Markdown，并在目录缺失或损坏时继续工作

当前计划先实现 Markdown、文本、YAML、JSON 和 CSV；HTML、EPUB 与 DOCX 使用独立的 Stage 2 提取器计划。PDF 正文提取属于未来扩展。BM25、Embedding、rerank、LLM 综合回答和联网 URL 抓取不属于本阶段。

### Stage 2A — 本地来源与直接搜索

**Status:** implemented（macOS 本地工作流）；跨平台验收仍待原生 CI。

已经提供 `review → operation show → apply`、来源版本历史、`query`、`cache rebuild` 和 `source verify`。来源保存更新原始对象、来源记录和日志，不修改主题文件。

本地验证使用 Rust/Cargo 1.95.0：格式、Clippy、workspace 测试、release 构建，以及 release 二进制的真实 CLI 工作流。覆盖新增/修改/移动/删除、规则排除、陈旧计划、版本完整性、缓存缺失/损坏/写入失败、中文查询、移动后重开及独立用户状态。另有子进程在对象、记录、日志和回执写入后直接退出的恢复测试，以及人工改动保护测试。

未验证：Windows/Linux 原生运行、Rust 1.85 原生构建、断电恢复。CI 配置使用 Rust 1.85 和三平台原生任务，但尚不能当作通过证据。

### Stage 2B — 文档提取器

**Status:** implemented（定向本地测试）。内置 HTML、EPUB 和 DOCX 提取器使用纯 Rust 依赖，实现正文、标题、链接和可引用位置。PDF 正文提取与 OCR 保持 future。全量回归、release 二进制及 Windows/Linux 原生验证未在本阶段运行。

## Stage 3 — 知识形成与安全保存

**Status:** implemented（定向本地测试）；跨平台验收仍待原生 CI。

- OKF v0.2 Producer Profile
- 来源、research 和 article 之间的引用关系
- 知识变更计划、确认、保存和恢复
- `Wiki/index.md` 与 `Wiki/log.md` 的受管理区域
- lint、来源新鲜度和冲突提示

### Stage 3A — OKF 与只读 lint

**Status:** implemented（定向本地测试）。`kb lint` 已实现 OKF v0.2 底线、显式受管理 Producer Profile、保留文件、Markdown 链接、孤立页、`supersedes`、精确来源版本、新鲜度和可移植路径检查。报告由应用层共享，CLI 的 `--strict` 只控制退出码。

本阶段运行了 core、app 和真实 CLI 的定向测试，以及格式和相关 crate 的 Clippy；未运行 workspace 全量测试、release 二进制流程或 Windows/Linux 原生验证。知识写入计划、受管理 index/log 更新和中断恢复由 Stage 3B 提供。

命令与 finding code 见 [Wiki lint 参考](docs/reference/lint.md)，实施计划见 [Wiki Lint](docs/superpowers/plans/2026-09-07-wiki-lint.md)。

### Stage 3B — 知识计划与安全保存

**Status:** implemented（定向本地测试）。`kb plan create` 接收结构化 research/article 请求并生成不改 Vault 的可审阅计划；现有 `operation show` 和 `apply` 完成查看、旧状态复核、受管理 index/log 派生、整批保存、重复执行和中断恢复。来源保存与知识保存的恢复状态互斥，所有入口复用 `kb-app` 的 typed request/report。

本阶段验证了 core 请求边界、应用层计划与保存、进程在 pending/每个文件/完成凭据处退出后的恢复、独立编辑保护、陈旧/过期/被修改计划、来源版本消失、缓存失效 warning，以及真实 CLI 的 init → plan → show → apply → query → strict lint → 换目录重开流程。只运行相关 crate 的定向测试、格式和 Clippy；未运行 workspace 全量测试、release 二进制流程、断电测试或 Windows/Linux 原生验证。

请求格式和恢复语义见[知识计划参考](docs/reference/knowledge-plans.md)，实施计划见[Knowledge Save](docs/superpowers/plans/2026-09-07-knowledge-save.md)。

## Stage 4 — Agent 与应用入口

**Status:** in progress

- 可移植 Agent Skill
- MCP 适配器
- 本机/局域网可选 HTTP（Stage 4A implemented）；SSE planned
- 可选局域网访问策略
- WebUI 和 GUI 共用的应用接口

### Stage 4A — 可选 HTTP 适配器

**Status:** implemented（定向本地测试）。`kb serve` 默认回环只读；局域网监听和 operation apply 要求 token 文件及显式权限。服务固定到启动时解析的 Vault ID，路由通过 `kb-app` 复用状态、诊断、查询、lint、来源 review、知识计划、operation 查看与 apply，不接受任意 Vault 或文件路径。CLI 子进程和真实 TCP 测试覆盖启动信封、鉴权、请求限制、只读拒绝、跨 Vault 拒绝及显式保存。

当前没有 TLS、daemon、自启动或 SSE。局域网明文模式只适用于受信任网络或用户管理的 TLS 反向代理。未运行 workspace 全量测试、release 二进制流程、浏览器 WebUI、拒绝服务加固或 Windows/Linux 原生验证。路由和安全边界见 [HTTP 参考](docs/reference/http.md)，实施计划见 [HTTP API](docs/superpowers/plans/2026-09-07-http-api.md)。

## Stage 5 — 搜索、备份与发布

**Status:** in progress

- 完整、可选的 BM25F（Stage 5A implemented）
- 索引缺失、损坏、版本不兼容或陈旧后的 direct 回退（Stage 5A implemented）
- 标准 ZIP 备份、校验和恢复（Stage 5B implemented）
- schema 迁移
- Windows、macOS 和 Linux 发布产物
- 安装包、签名和升级流程

Embedding 和 rerank 保持 future，除非独立设计证明它们能带来足够收益且不成为知识库运行前提。

### Stage 5A — 可选 BM25F

**Status:** implemented（定向本地测试）。`search.mode: bm25` 提供标题、别名、章节、标签和正文加权，ASCII 词项与中文 1–3 gram、章节级结果、确定性整数评分及字段贡献解释。`.kb/cache/bm25.json` 支持未变文档复用、增改删更新和精确新鲜度核对；默认失败策略是整次 direct 回退，`--strict-backend` 返回 `index_stale`。

本阶段运行了 core 搜索契约、应用层索引/排序/失效、来源及知识保存后的双缓存失效、真实 CLI BM25 流程、JSON/文档契约和既有 Stage 2 查询流程的定向测试，以及格式和相关 crate 的 Clippy。未运行 workspace 全量测试、release 二进制流程或 Windows/Linux 原生验证。

排序与索引语义见[搜索规则](docs/reference/search.md)，实施计划见[BM25F Search](docs/superpowers/plans/2026-09-07-bm25f-search.md)。

### Stage 5B — 可校验 ZIP 备份

**Status:** implemented（定向本地测试）。`kb backup create|verify|restore` 使用标准 ZIP 和逐文件 SHA-256 清单，完整保存 Wiki、共享配置、schema 及启用或停用的准入主题目录；本机配置、缓存、恢复状态和 Git 内部目录不迁移。可选精简归档明确标记缺少完整来源证据。恢复只面向不存在或真实空目录，并经过不可信归档校验、私有同级暂存和写入时二次哈希核对。

本阶段验证了清单结构、空目录、完整/精简范围、链接和路径穿越拒绝、可移植路径冲突、哈希篡改、已有输出、Vault 内输出、非空目标、待恢复状态阻断，以及真实 CLI 创建 → 移动归档 → 独立状态目录校验 → 恢复 → 显式路径重开查询。只运行相关 crate 的定向测试、格式和 Clippy；未运行 workspace 全量测试、release 二进制流程、断电测试或 Windows/Linux 原生跨系统恢复。

使用和安全边界见[备份参考](docs/reference/backup.md)，实施计划见[Verified ZIP Backup](docs/superpowers/plans/2026-09-07-verified-zip-backup.md)。
