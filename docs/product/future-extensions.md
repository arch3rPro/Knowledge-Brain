# 未来扩展

本文档收纳尚未进入实现阶段的产品方向。它不表示已实现能力、已承诺发布日期或当前路线图状态；实际进度和验证证据见[路线图](../../ROADMAP.md)。每项能力进入实施前都需要独立设计、兼容性评估和验收标准。

## 格式演进与迁移

未来实际 schema 变更需要提供：历史格式解析、可恢复转换、跨版本 fixture 和明确的 `kb migrate` 入口。不能仅提高 `schema_version` 或假定任意旧版本可迁移。

## 知识能力

- PDF 正文提取与 OCR；
- Embedding 检索与可选 rerank；
- 联网 URL 抓取与可审阅来源保存；
- 基于明确来源和引用的 LLM 综合整理。

这些能力不能成为 Markdown、轻量目录或 direct 查询的运行前提。它们必须保留来源、引用、可关闭路径和失败回退。

## Agent 宿主集成

未来可提供 Codex Plugin、Claude Code Plugin 及其他 Agent 宿主的原生插件或扩展。它们负责宿主特有的安装、升级、权限和界面整合，并调用 `kb`、固定 Vault MCP 或 HTTP 契约。

宿主插件不得复制 Vault 规则、直接修改 Vault 受管理内容，或绕过 operation 预览、最终确认、陈旧计划检测和人工修改保护。

## 产品与协作入口

- 基于共享应用语义的 WebUI 与桌面 GUI；
- 浏览器 EventSource 的 operation 进度界面；
- 多设备协作、同步与冲突处理。

这些入口必须消费同一份 operation、查询、诊断和错误事实，不能各自解释 Vault 规则或重新定义写入确认。

## 发布与网络安全

- TLS、CORS/Origin 策略和面向局域网的部署配置；
- 后台服务、自启动与可观察性；
- 平台包管理器、macOS notarization、Windows Authenticode、无人值守升级和面向用户的回滚界面。

明文 HTTP、缺少 TLS 的局域网服务和未签名二进制不能因这些方向被误表述为已经具备生产发布保证。
