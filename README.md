# Knowledge-Brain

Knowledge-Brain 是面向人和 AI 工具的本地知识库基础设施。它以普通目录、Markdown、YAML 和 JSON 保存知识，让不同编辑器、Agent 和应用共享同一套可读取、可迁移的 Vault。

本项目使用 [MIT License](LICENSE)。贡献方式与本地验证要求见 [CONTRIBUTING.md](CONTRIBUTING.md)。

[Features](#features) · [快速开始](#快速开始) · [命令参考](docs/reference/commands.md) · [Roadmap](ROADMAP.md) · [设计文档](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Features

- **File-first Vault** — 人类可直接读写长期内容；缓存和索引不是知识的唯一副本。
- **Explicit admission** — `admission.yml` 明确指定允许进入知识处理范围的一级主题目录。
- **Structured Wiki** — 来源、阶段性研究和可复用文章分别保存在固定的三层 `Wiki/` 结构中。
- **Reviewable changes** — 采用已有目录等多文件操作先生成计划，再由用户明确执行。
- **Traceable sources** — 保留原始文件副本和精确版本，来源变更不覆盖旧证据。
- **Layered search** — 默认直接查询真实文件；可选 BM25F 提供字段加权、中文检索和可解释评分。
- **Verified backups** — 生成带逐文件 SHA-256 清单的标准 ZIP，并只向空目录恢复。
- **Portable format** — Vault 路径和文件名按 Windows、macOS 与 Linux 的共同规则校验。
- **Tool-independent core** — CLI 与未来的 MCP、HTTP、WebUI 和 GUI 共用同一应用层。
- **Agent-ready interfaces** — 内置跨宿主 Agent Skill，并提供固定 Vault、默认只读的 MCP stdio 入口。
- **Offline by default** — 基础操作不依赖 LLM、Node.js、Python、数据库、云账号或常驻服务。

## Vault 如何组织

```text
My-Knowledge/
├── KB.md                         # Vault 规则
├── admission.yml                # 获准读取的一级主题目录
├── Work/                        # 用户自定义主题目录
├── Reading/                     # 用户自定义主题目录
├── Wiki/
│   ├── external-sources/        # 来源记录和原始对象
│   │   ├── records/
│   │   └── .objects/sha256/
│   ├── research/                # 阶段性研究
│   ├── articles/                # 可复用知识文章
│   ├── index.md                 # 人类可读的知识入口
│   └── log.md                   # 知识变更记录
└── .kb/
    ├── config.yml               # Vault 身份和共享配置
    ├── schemas/                 # 配置格式定义
    ├── cache/                   # 可重建缓存
    └── runtime/                 # 运行状态
```

顶层主题目录由使用者命名。Knowledge-Brain 不会创建 `Work/`、`Reading/` 等个人分类，也不会自动将已有目录加入准入清单。

`admission.yml` 决定哪些主题目录可以被处理；`Wiki/external-sources/`、`Wiki/research/` 和 `Wiki/articles/` 分别保存来源、研究与文章。详细语义见[架构概览](docs/architecture/overview.md)。

## 安装

当前从源码安装，需要 Rust 1.85 或更高版本。在仓库根目录执行：

```bash
cargo install --path crates/kb-cli --locked
kb version
```

`cargo install` 会把 `kb` 安装到 Cargo 的可执行文件目录；Linux、macOS 和 Windows 使用同一条命令。如果该目录尚未加入 `PATH`，也可以不安装，直接执行：

```bash
cargo run --release -p kb-cli -- version
```

开发构建生成的文件位于 `target/release/kb`（Windows 为 `target\release\kb.exe`）。

## 快速开始

### 创建 Vault

```bash
kb init ./my-knowledge
mkdir ./my-knowledge/Notes
```

`kb init` 创建最小 Vault，不会创建 Git 仓库或个人主题目录。

### 配置准入目录

```bash
kb config admission add notes Notes --vault ./my-knowledge
kb config admission add notes Notes --vault ./my-knowledge --yes
```

第一次调用显示差异预览；带 `--yes` 的调用才会保存。准入操作要求目录已经存在，并且不会删除目录内容。

### 保存来源与查询

将 Markdown 或文本放入 `Notes/`。下面的 `review` 只检查 `admission.yml` 中已启用的目录，并生成一份待确认的保存计划：

```bash
kb review --vault ./my-knowledge
kb operation show <operation-id>
kb apply <operation-id>
kb query "关键词" --scope sources --vault ./my-knowledge
```

用 `review` 返回的 ID 替换 `<operation-id>`。没有变化时不产生新计划。来源文件保持原样，保存的副本位于 `Wiki/external-sources/`。查询默认只搜索 Wiki；`--scope all` 同时返回 Wiki 与来源两组结果。见[保存与查询来源](docs/guides/capture-and-query-sources.md)。

### 检查结果

```bash
kb status --vault ./my-knowledge
kb config admission list --vault ./my-knowledge
kb doctor --vault ./my-knowledge
```

### 采用已有目录

```bash
kb adopt ./existing-notes --json
kb operation show <operation-id> --json
kb apply <operation-id> --json
```

`adopt` 只保存审核计划，不修改目标目录；`apply` 会重新核对计划中的已有文件，再添加 Vault 框架。参见[采用已有目录](docs/guides/adopt-an-existing-directory.md)。

## 常用命令

| 命令 | 用途 |
| --- | --- |
| `kb init` | 创建最小 Vault |
| `kb adopt` | 审核已有目录并生成采用计划 |
| `kb apply` | 执行已审核的操作计划 |
| `kb review` | 查看准入来源变化并生成保存计划 |
| `kb query` | 查询 Wiki 或已保存来源 |
| `kb cache rebuild` | 重建轻量目录及已启用的搜索索引 |
| `kb source verify` | 核对已保存来源的完整性 |
| `kb backup` | 创建、校验和恢复标准 ZIP 备份 |
| `kb config` | 查看、校验和修改配置或准入清单 |
| `kb vault` | 管理本机 Vault 注册和路径绑定 |
| `kb status` | 查看 Vault 状态和 schema 兼容性 |
| `kb doctor` | 运行独立诊断 |
| `kb capabilities` | 查询当前二进制公开的能力 |
| `kb skills` | 检测、安装、检查或安全卸载可移植 Agent Skill |
| `kb mcp` | 为一个固定 Vault 启动 MCP stdio 服务 |
| `kb serve` | 按需启动固定 Vault 的可选 HTTP/SSE 接口 |

完整语法见[命令参考](docs/reference/commands.md)，配置层级和 `admission.yml` 格式见[配置参考](docs/reference/configuration.md)。

## Agent Skills

项目发布八个可单独安装的 `kb-*` Skills。需要 Knowledge-Brain 管理 Vault/User 范围安装、迁移和安全卸载时，使用 `kb skills`；只需要把 Skill 文件加入某个 Agent 工作区时，使用 `npx skills add . --skill '*' --agent codex --yes` 或 `npx skills add . --skill kb-query --agent codex --yes`。两种方式各自管理自己的文件，详见[Agent Skill 参考](docs/reference/agent-skill.md)。

## 架构

```text
CLI / MCP / HTTP / future adapters
          │
          ▼
       kb-app
          │
    ┌─────┴──────────┐
    ▼                ▼
 kb-core       Vault / local state
```

- `kb-core` 定义 schema、错误码、可移植路径和领域类型。
- `kb-app` 实现完整用例和文件操作。
- `kb-protocol` 定义稳定 JSON 信封。
- `kb-cli` 只负责参数解析和输出呈现。

入口适配器不直接实现 Vault 业务规则。完整边界见[架构概览](docs/architecture/overview.md)。

## 开发

Linux 和 macOS：

```bash
bash scripts/check-stage-2.sh
```

Windows PowerShell：

```powershell
./scripts/check-stage-2.ps1
```

检查覆盖格式、Clippy、测试、release 构建和真实 CLI 工作流。开发阶段和后续范围由 [Roadmap](ROADMAP.md) 统一记录。

## 文档

- [创建新 Vault](docs/guides/create-a-vault.md)
- [采用已有目录](docs/guides/adopt-an-existing-directory.md)
- [保存与查询来源](docs/guides/capture-and-query-sources.md)
- [来源格式](docs/reference/sources.md)
- [搜索规则](docs/reference/search.md)
- [备份、校验与恢复](docs/reference/backup.md)
- [知识计划与安全保存](docs/reference/knowledge-plans.md)
- [Portable Agent Skill](docs/reference/agent-skill.md)
- [MCP stdio](docs/reference/mcp.md)
- [命令参考](docs/reference/commands.md)
- [配置参考](docs/reference/configuration.md)
- [架构概览](docs/architecture/overview.md)
- [Roadmap](ROADMAP.md)
- [安全边界](SECURITY.md)
- [完整设计](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)
- [架构决策记录](docs/decisions/)
