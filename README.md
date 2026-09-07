# Knowledge-Brain

面向人和 AI 工具的本地知识库基础设施。

Knowledge-Brain 使用普通目录、Markdown、YAML 和 JSON 管理知识。你可以直接阅读和编辑所有长期内容，也可以让不同的 CLI、Agent、WebUI 或 GUI 共用同一套知识边界和操作规则。Vault 不依赖特定编辑器、模型厂商、Agent 框架或云服务。

[快速开始](#快速开始) · [Vault 结构](#vault-如何组织) · [当前能力](#当前能力) · [命令参考](docs/reference/commands.md) · [设计文档](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)

> 项目处于早期开发阶段。当前版本提供可靠的 Vault 基础能力；内容发现、搜索和知识加工仍在后续开发计划中。

## 为什么选择 Knowledge-Brain

- **文件属于你。** Markdown、YAML 和 JSON 是可迁移的长期数据，不需要专有数据库才能恢复知识。
- **目录由你决定。** `Work/`、`Reading/` 或其他主题目录都由使用者创建；项目不会植入个人分类法。
- **读取范围明确。** `admission.yml` 是准入清单，只有显式启用的一级目录才会进入后续知识处理范围。
- **修改可以审查。** 对已有目录的采用先生成计划，再显式执行；执行前会重新核对被审核的文件。
- **入口可以替换。** CLI 和未来的 MCP、HTTP、WebUI、GUI 共用 `kb-app` 应用层，不在各入口重复实现文件规则。
- **检索按规模渐进。** 设计以直接读取 Markdown 和轻量目录索引为基础，完整 BM25 作为可选能力；Embedding 和 rerank 是后续扩展，而不是使用前提。

## Vault 如何组织

一个 Vault 是可以整体复制、备份或放入版本控制的普通目录：

```text
My-Knowledge/
├── KB.md                         # Vault 规则和操作边界
├── admission.yml                # 获准读取的一级主题目录
├── Work/                        # 用户自定义主题目录
├── Reading/                     # 用户自定义主题目录
├── Wiki/
│   ├── external-sources/        # 来源记录和不可修改的原始对象
│   │   └── .objects/sha256/
│   ├── research/                # 阶段性研究和未定结论
│   ├── articles/                # 可复用知识文章
│   ├── index.md                 # 人和工具都能读取的知识入口
│   └── log.md                   # 知识变更记录
└── .kb/
    ├── config.yml               # 随 Vault 迁移的配置和身份
    ├── schemas/                 # 配置格式定义
    ├── cache/                   # 可重建缓存
    └── runtime/                 # 运行时状态
```

主题目录和 `Wiki/` 的职责不同：主题目录保存你原有的材料，`admission.yml` 决定哪些目录可以被处理；`Wiki/external-sources/`、`Wiki/research/` 和 `Wiki/articles/` 分别保存来源、阶段性研究与可复用文章。`kb init` 只创建框架目录，不创建 `Work/`、`Reading/` 等个人主题目录。

## 安装

当前尚未发布预编译安装包，需要 Rust 1.85 或更高版本从源码构建：

```bash
cargo build --release -p kb-cli
```

构建结果位于：

- Linux/macOS：`target/release/kb`
- Windows：`target\release\kb.exe`

可以把该文件复制到 `PATH` 中，或在项目目录内直接运行。Knowledge-Brain 不需要 Node.js、Python、数据库或常驻服务。

## 快速开始

### 创建新 Vault

```bash
kb init ./my-knowledge
mkdir ./my-knowledge/Notes
kb config admission add notes Notes --vault ./my-knowledge
kb config admission add notes Notes --vault ./my-knowledge --yes
kb status --vault ./my-knowledge
```

第一次 `admission add` 只显示变更预览，带 `--yes` 的第二次调用才会保存。准入操作不会创建或删除主题目录。

### 采用已有目录

已有笔记目录使用两步流程：

```bash
kb adopt ./existing-notes --json
kb operation show <operation-id> --json
kb apply <operation-id> --json
kb doctor --vault ./existing-notes
```

`adopt` 只保存审核计划，不修改目标目录。`apply` 会重新核对已有文件，再添加 Knowledge-Brain 框架；已有一级目录不会被自动写入准入清单。完整说明见[采用已有目录](docs/guides/adopt-an-existing-directory.md)。

### 查看 Vault

```bash
kb status --vault ./my-knowledge
kb config show --sources --vault ./my-knowledge
kb config admission list --vault ./my-knowledge
kb capabilities --json
```

`capabilities` 是判断当前二进制实际能力的权威入口，客户端不应根据版本号猜测功能。

## 常用命令

| 命令 | 用途 |
| --- | --- |
| `kb init` | 在空目录中创建最小 Vault |
| `kb adopt` | 审核已有目录并生成采用计划 |
| `kb apply` | 执行已经审核的操作计划 |
| `kb config` | 查看、校验或修改分层配置和准入清单 |
| `kb vault` | 管理本机 Vault 注册和移动后的重新绑定 |
| `kb status` | 查看 Vault 的事实状态和 schema 兼容性 |
| `kb doctor` | 运行彼此独立的诊断检查 |
| `kb capabilities` | 查询当前二进制已经实现的能力 |

所有命令和参数见[命令参考](docs/reference/commands.md)，配置优先级及 `admission.yml` 格式见[配置参考](docs/reference/configuration.md)。支持 `--json` 的命令会返回带 `schema_version` 的稳定信封，适合未来 UI 和自动化调用。

## 当前能力

| 能力 | 状态 |
| --- | --- |
| Vault 初始化和已有目录采用 | 已实现 |
| `admission.yml` 准入管理 | 已实现 |
| 分层配置和无损 YAML 修改 | 已实现 |
| Vault 注册、移动和重新绑定 | 已实现 |
| 操作计划、中断恢复和并发锁 | 已实现 |
| 状态、诊断和稳定 JSON 协议 | 已实现 |
| 直接 Markdown 搜索和轻量索引 | 计划中 |
| 可选完整 BM25 | 计划中 |
| 来源发现、正文提取和知识加工 | 计划中 |
| Embedding 和 rerank | 后续扩展 |
| MCP、HTTP、WebUI 和 GUI | 后续入口 |

当前 schema 版本是 `v1.0`。Stage 1 已在本机 macOS 完整运行，仓库同时配置了 Ubuntu、macOS 和 Windows 原生 CI；取得三种原生任务的成功记录前，不将项目描述为跨平台验证完成或可发布版本。

## 设计原则

Knowledge-Brain 的实现遵守以下边界：

1. Vault 中的人类可读文件是长期数据，缓存和索引必须可以重建。
2. `admission.yml` 表示读取授权，不是待处理队列。
3. 需要修改多个文件的操作采用“审核计划 → 明确执行”。
4. Git、联网、局域网服务和外部模型调用都不能被自动开启。
5. Vault 内路径按 Windows、macOS 和 Linux 的共同规则校验。
6. 产品逻辑位于统一应用层，界面和协议只是适配入口。

更完整的目录语义、检索设计、错误模型及未来扩展见[设计文档](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)和[架构概览](docs/architecture/overview.md)。

## 开发

Rust workspace 包含四个 crate：

| Crate | 职责 |
| --- | --- |
| `kb-core` | schema、错误码、可移植路径和领域类型 |
| `kb-app` | 完整应用用例和文件操作 |
| `kb-protocol` | 稳定 JSON 成功/失败信封 |
| `kb-cli` | 命令解析和输出呈现 |

在 Linux 或 macOS 上运行 Stage 1 检查：

```bash
bash scripts/check-stage-1.sh
```

在 Windows PowerShell 中运行：

```powershell
./scripts/check-stage-1.ps1
```

检查包括格式、Clippy、全部测试、release 构建，以及初始化、准入、移动、重新绑定、诊断和目录采用等真实 CLI 路径。

## 文档

- [创建新 Vault](docs/guides/create-a-vault.md)
- [采用已有目录](docs/guides/adopt-an-existing-directory.md)
- [命令参考](docs/reference/commands.md)
- [配置参考](docs/reference/configuration.md)
- [架构概览](docs/architecture/overview.md)
- [安全边界](SECURITY.md)
- [完整设计](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)
- [架构决策记录](docs/decisions/)
