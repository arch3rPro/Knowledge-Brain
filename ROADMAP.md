# Knowledge-Brain Roadmap

本文档记录实现阶段和验证状态。产品语义由[设计文档](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)定义，具体开发步骤由各阶段的 implementation plan 定义。

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

**Status:** in progress

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

未验证：Windows/Linux 原生运行、Rust 1.85 原生构建、断电恢复。CI 配置使用 Rust 1.85 和三平台原生任务，但尚不能当作通过证据。Stage 2 整体尚未完成。

### Stage 2B — 文档提取器

**Status:** implemented（定向本地测试）。内置 HTML、EPUB 和 DOCX 提取器使用纯 Rust 依赖，实现正文、标题、链接和可引用位置。PDF 正文提取与 OCR 保持 future。全量回归、release 二进制及 Windows/Linux 原生验证未在本阶段运行。

## Stage 3 — 知识形成与安全保存

**Status:** planned

- OKF v0.2 Producer Profile
- 来源、research 和 article 之间的引用关系
- 知识变更计划、确认、保存和恢复
- `Wiki/index.md` 与 `Wiki/log.md` 的受管理区域
- lint、来源新鲜度和冲突提示

## Stage 4 — Agent 与应用入口

**Status:** planned

- 可移植 Agent Skill
- MCP 适配器
- 本机 HTTP 与 SSE
- 可选局域网访问策略
- WebUI 和 GUI 共用的应用接口

## Stage 5 — 搜索、备份与发布

**Status:** planned

- 完整、可选的 BM25F
- 索引损坏后的直接 Markdown 回退
- 标准 ZIP 备份、校验和恢复
- schema 迁移
- Windows、macOS 和 Linux 发布产物
- 安装包、签名和升级流程

Embedding 和 rerank 保持 future，除非独立设计证明它们能带来足够收益且不成为知识库运行前提。
