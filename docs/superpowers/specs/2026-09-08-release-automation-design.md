# 按需验证与正式 CLI 发布设计

- Status: approved design
- Date: 2026-09-08
- Decision: [ADR-0019](../../decisions/proposed/architecture/0019-on-demand-verification-and-tagged-releases.md)

## 目标

Knowledge-Brain 只在维护者主动请求验证或推送正式版本 tag 时使用 GitHub Actions。正式版本以 GitHub Release 发布三个可下载、可校验、可追溯的 CLI 二进制包。

普通 push 和 pull request 不触发工作流。

## 目标与非目标

首个发布目标为：

| 平台 | Rust target | 包格式 | 可执行文件 |
| --- | --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `.tar.gz` | `kb` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` | `kb` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.zip` | `kb.exe` |

`macos-latest` 用 ARM64 GitHub-hosted runner 构建并运行 Apple Silicon 包。每个目标的 release 二进制必须完成真实 CLI journey，不能用其他架构的测试结果替代。

首版不包含：

- macOS Developer ID 签名或 notarization；
- Windows Authenticode 签名；
- Homebrew、Scoop、WinGet、cargo registry 或其他第三方分发通道；
- Linux ARM64、macOS Intel 或 Windows ARM64 包；
- 自动升级器。

## 版本与触发

版本的唯一来源是根目录 `Cargo.toml` 的 workspace package version。

正式发布使用带注释 tag `vX.Y.Z`。发布工作流必须拒绝以下情况：

- 轻量 tag；
- tag 名称不是严格的 `vX.Y.Z`；
- tag 版本与 Cargo 版本不同；
- tag 指向的提交不在 `main` 的祖先链上。

一个符合要求的 tag 自动开始完整发布。发布工作流不提供手动 publish 输入，避免输入版本与 tag 或二进制不一致。

手动验证使用 `workflow_dispatch`，可选择 `linux`、`macos`、`windows` 或 `all`。手动任务仍先运行格式与 Clippy，再启动选中的原生 job。GitHub 的 “Re-run failed jobs” 是首选失败重试方式：它只重跑失败的原生 job；若原运行已经过期，维护者可手动选择同一平台重新验证。

## 工作流结构

工作流分为三层，避免构建步骤漂移：

1. 可复用原生构建工作流拥有质量检查、目标矩阵、测试、release 构建、包内 smoke test、打包、内部 artifact 上传和构建证明。
2. 手动验证工作流调用可复用工作流，并把选中的平台传入目标矩阵。
3. tag 发布工作流调用同一可复用工作流并传入三个目标；其 publication job 在全部成功后下载包并创建 GitHub Release。

原生 job 使用 `fail-fast: false`。一个目标失败不会取消其他目标；其余平台仍提供独立结果。

构建 job 只拥有读取仓库、上传内部 artifact 与生成 provenance 所需权限。最终 publication job 单独拥有 `contents: write`。发布 job 只处理本次运行生成的内部 artifact，不从其他 workflow 或本机文件收集二进制。

## 包、校验与构建证明

每个归档名为：

~~~text
knowledge-brain-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
knowledge-brain-vX.Y.Z-aarch64-apple-darwin.tar.gz
knowledge-brain-vX.Y.Z-x86_64-pc-windows-msvc.zip
~~~

每个归档只包含：

- 对应平台的 `kb` 或 `kb.exe`；
- `LICENSE`；
- 面向该归档的简短安装与校验文本。

归档在创建后必须在同一原生 job 中解包并执行 `kb version --json`。输出版本必须等于 tag 版本，且平台可执行文件名正确。通过后才上传内部 artifact。

最终 publication job 对三个归档生成一个标准 `SHA256SUMS` 文件。每个归档还生成 GitHub build provenance，供用户使用 `gh attestation verify <archive> -R arch3rPro/Knowledge-Brain` 验证构建来源。校验和与 provenance 不替代平台代码签名；文档必须明确 macOS Gatekeeper 和 Windows SmartScreen 仍可能显示提示。

## GitHub Release 与失败恢复

所有 native job 成功后，publication job：

1. 创建该 tag 的 draft Release；
2. 上传三个归档和 `SHA256SUMS`；
3. 生成 Release Notes；
4. 将 draft 发布为公开 Release。

用户只会看到完整的公开 Release。若 publication job 失败，重跑该 job，并复用本次运行的已验证内部 artifact；不得重新触发三个 native build job。发布脚本必须拒绝覆盖已有公开 Release；维护者需要撤回或替换版本时，创建新补丁版本，而不是静默替换资产。

## 维护者操作与用户文档

实现后新增发布指南，作为正式流程的唯一操作说明。它覆盖：

1. 修改 `Cargo.toml` 版本和变更说明并合入 `main`；
2. 手动执行全平台验证或只验证有风险的平台；
3. 创建带注释 `vX.Y.Z` tag 并推送；
4. 在 Actions 中查看三个原生 job 与 publication job；
5. 只重试失败平台或只重试 publication job；
6. 下载归档、校验 `SHA256SUMS`、验证 GitHub provenance；
7. 针对错误版本发布新补丁版本。

README 的安装部分在实现后以 GitHub Release 二进制下载为主，源码 `cargo install --path` 保留为开发者路径。README 只链接发布指南和校验参考，不重复工作流细节。

## 验收证据

实现至少提供：

- workflow 语法与触发条件检查；
- 手动单平台任务只解析并启动该平台的矩阵测试；
- tag、Cargo 版本、注释 tag 与 `main` 祖先关系的正反例；
- 三个目标各自的打包、解包和 `kb version --json` 验证；
- 归档名、内容与 `SHA256SUMS` 的可重复断言；
- 无签名凭据时不尝试 notarization 或 Authenticode；
- 发布 job 仅在三个 artifact 到齐时开始，并在失败重试时复用它们；
- GitHub 上一次真实预发布 tag 的三平台构建、provenance 与公开 Release 资产检查。

本文件拥有发布自动化的设计和验收条件。实现后的命令由发布指南拥有，工作流细节由 `.github/workflows/` 拥有，用户下载步骤由 README 拥有。
