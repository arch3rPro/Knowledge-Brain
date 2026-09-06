# Knowledge-Brain

Knowledge-Brain 是一个本地优先、文件优先的可移植知识库工具。Vault 中可长期保留的内容使用 Markdown、YAML 和 JSON；CLI、未来 WebUI/GUI 以及其他 Agent 入口共用同一 Rust 应用层。

当前仓库实现的是 Stage 1 Vault 基础：初始化或安全采用目录、`admission.yml` 准入配置、分层配置、Vault 注册与迁移、状态诊断、操作计划和中断恢复。搜索、来源发现、正文提取、BM25、MCP、HTTP、备份与同步尚未实现；以 `kb capabilities --json` 的结果为准。

## 快速开始

需要 Rust 1.85 或更高版本。当前尚未提供发布安装包。

```bash
cargo build --release -p kb-cli
./target/release/kb init ./my-vault
mkdir ./my-vault/Notes
./target/release/kb config admission add notes Notes --vault ./my-vault --yes
./target/release/kb status --vault ./my-vault
```

`kb init` 不会创建 Git 仓库。`admission add` 只授权已经存在的一级目录，不会代替用户创建主题目录。

## 当前验证状态

Stage 1 的本机 macOS release 流程已通过。仓库包含 Ubuntu、macOS、Windows 原生 CI 门禁，但在三种原生任务都产生成功证据前，不把 Stage 1 称为跨平台验证完成或可发布版本。

## 文档

- [创建新 Vault](docs/guides/create-a-vault.md)
- [采用已有目录](docs/guides/adopt-an-existing-directory.md)
- [命令参考](docs/reference/commands.md)
- [配置参考](docs/reference/configuration.md)
- [架构概览](docs/architecture/overview.md)
- [安全边界](SECURITY.md)
- [完整设计](docs/superpowers/specs/2026-09-07-knowledge-brain-design.md)
