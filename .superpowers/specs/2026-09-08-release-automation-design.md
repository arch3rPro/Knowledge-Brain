# 按需验证与正式 CLI 发布设计

- Status: proposed revision
- Date: 2026-09-08
- Decisions: [ADR-0019](../../docs/decisions/proposed/architecture/0019-on-demand-verification-and-tagged-releases.md), [ADR-0020](../../docs/decisions/proposed/architecture/0020-explicit-verified-cli-updates.md), [ADR-0021](../../docs/decisions/proposed/process/0021-build-before-tag-release.md)

## 目标

Knowledge-Brain 只在维护者主动请求验证或正式发布时使用 GitHub Actions。正式版本通过一次手动发布任务完成测试、构建、tag、签名和 GitHub Release，发布三个可下载、可校验、可追溯且可由官方二进制安全更新的 CLI 包。

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
- 后台自动检查或自动升级器。

## 版本与触发

版本的唯一来源是根目录 `Cargo.toml` 的 workspace package version。

正式发布由 `workflow_dispatch` 启动，版本从根目录 `Cargo.toml` 读取。工作流必须先拒绝以下情况：

- 运行提交不是所选 `main` 提交；
- Cargo 版本不是严格的 `X.Y.Z` 稳定版本；
- 缺少 `docs/releases/vX.Y.Z.md`；
- 已存在公开 Release，或对应 tag 不属于本次运行提交；publication 重试可接受本次提交上已有的同名 annotated tag。

工作流不接收独立版本输入，避免输入值与 Cargo 或二进制不一致。质量和三个原生 job 全部成功后，publication job 才在运行提交上创建带注释 `vX.Y.Z` tag；tag push 本身不触发工作流。

手动验证使用 `workflow_dispatch`，可选择 `linux`、`macos`、`windows` 或 `all`。手动任务仍先运行格式与 Clippy，再启动选中的原生 job。GitHub 的 “Re-run failed jobs” 是首选失败重试方式：它只重跑失败的原生 job；若原运行已经过期，维护者可手动选择同一平台重新验证。

## 工作流结构

工作流分为三层，避免构建步骤漂移：

1. 可复用原生构建工作流拥有质量检查、目标矩阵、测试、release 构建、包内 smoke test、打包、内部 artifact 上传和构建证明。
2. 手动验证工作流调用可复用工作流，并把选中的平台传入目标矩阵。
3. 手动发布工作流调用同一可复用工作流并传入三个目标；其 publication job 在全部成功后下载包、创建 tag 并创建 GitHub Release。

原生 job 使用 `fail-fast: false`。一个目标失败不会取消其他目标；其余平台仍提供独立结果。

构建 job 只拥有读取仓库、上传内部 artifact 与生成 provenance 所需权限。最终 publication job 单独拥有 `contents: write`。发布 job 只处理本次运行生成的内部 artifact，不从其他 workflow 或本机文件收集二进制。

## 包、校验与构建证明

每个归档名为：

~~~text
knowledge-brain-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
knowledge-brain-vX.Y.Z-aarch64-apple-darwin.tar.gz
knowledge-brain-vX.Y.Z-x86_64-pc-windows-msvc.zip
~~~

每个平台同时发布一份无需解压的可执行文件：

~~~text
knowledge-brain-vX.Y.Z-x86_64-unknown-linux-gnu
knowledge-brain-vX.Y.Z-aarch64-apple-darwin
knowledge-brain-vX.Y.Z-x86_64-pc-windows-msvc.exe
~~~

每个归档只包含：

- 对应平台的 `kb` 或 `kb.exe`；
- `LICENSE`；
- 面向该归档的简短安装与校验文本。

归档在创建后必须在同一原生 job 中解包并执行 `kb version --json`。输出版本必须等于 tag 版本，且平台可执行文件名正确。通过后才上传内部 artifact。

最终 publication job 对三个归档和三个可执行文件生成一个标准 `SHA256SUMS` 文件，并使用 `KB_UPDATE_SIGNING_KEY` 签名为 `SHA256SUMS.minisig`。与该私钥匹配的公钥是仓库中的审查对象，并编译进官方 Release 二进制。私钥不进入仓库、构建 artifact 或 GitHub Release。缺少该 Secret 时 publication job 失败。

每个归档和可执行文件还生成 GitHub build provenance，供用户使用 `gh attestation verify <file> -R arch3rPro/Knowledge-Brain` 验证构建来源。校验和、更新签名与 provenance 不替代平台代码签名；文档必须明确 macOS Gatekeeper 和 Windows SmartScreen 仍可能显示提示。

## GitHub Release 与失败恢复

所有 native job 成功后，publication job：

1. 生成三个归档和三个可执行文件的 `SHA256SUMS` 与 `SHA256SUMS.minisig`，并确认八个资产完整；
2. 在本次运行提交上创建并推送带注释 tag；若 publication 重试时该 tag 已存在，则严格核对其类型和提交；
3. 使用 `docs/releases/vX.Y.Z.md` 创建该 tag 的 draft Release；
4. 上传三个归档、三个可执行文件、`SHA256SUMS` 与 `SHA256SUMS.minisig`；
5. 将 draft 发布为公开 Release。

用户只会看到完整的公开 Release。若 publication job 失败，重跑该 job，并复用本次运行的已验证内部 artifact；不得重新触发三个 native build job。发布脚本必须拒绝覆盖已有公开 Release；维护者需要撤回或替换版本时，创建新补丁版本，而不是静默替换资产。

## 维护者操作与用户文档

实现后新增发布指南，作为正式流程的唯一操作说明。它覆盖：

1. 修改 `Cargo.toml` 版本并编写 `docs/releases/vX.Y.Z.md`，然后合入 `main`；
2. 在 Actions 中手动启动一次正式发布；
3. 查看质量、三个原生 job 与 publication job；
4. 只重试失败平台或只重试 publication job；
5. 下载归档、校验 `SHA256SUMS`、验证 GitHub provenance；
6. 针对错误版本发布新补丁版本。

README 的安装部分在实现后以 GitHub Release 二进制下载为主，源码 `cargo install --path` 保留为开发者路径。README 只链接发布指南和校验参考，不重复工作流细节。

## 显式 CLI 更新

`kb update check` 与 `kb update` 是唯一会查询更新服务的命令；程序不会后台检查或自动下载。两者只查询 GitHub 的最新正式 Release，忽略 draft 和 prerelease。

官方 Release 二进制在构建时内嵌自己的目标标识与 Minisign 公钥。只有带这两个标识的二进制可运行更新命令；通过 `cargo install`、源码构建或开发目录启动的 `kb` 返回安装边界错误，并提示使用其原安装方法更新。

`kb update check` 只报告当前版本、可用版本、匹配归档和下载地址，不写入磁盘。`kb update` 按以下顺序执行：

1. 获取最新 Release 的目标归档、`SHA256SUMS` 与 `SHA256SUMS.minisig`；
2. 用内嵌公钥验证 `SHA256SUMS.minisig`；
3. 从已验证的 checksum 文件读取目标归档的 SHA-256，并核对下载归档；
4. 解包到私有临时目录，拒绝路径穿越、链接和多余可执行文件；
5. 运行已验证的新二进制 `kb version --json`，核对其版本与目标标识；
6. 启动一个复制出的短生命周期 helper，主进程退出后由 helper 替换安装位置的二进制；
7. helper 仅在新文件就位后删除可恢复备份和临时文件。

更新只接受比当前版本更新的稳定版本。没有 Release、没有目标归档、版本不新、网络失败、签名失败、hash 不匹配、归档不安全、版本或目标不匹配、无写权限或替换失败时，旧二进制必须保持可用。Windows helper 对临时文件锁使用有限重试；超出限制时保留备份与诊断信息。

更新签名密钥需要轮换时，新的公钥必须先由旧私钥签名并随一个可验证 Release 发布；旧密钥已不可用时，用户按发布指南手工下载并校验新版本，不能绕过签名验证。

## 验收证据

实现至少提供：

- workflow 语法与触发条件检查；
- 手动单平台任务只解析并启动该平台的矩阵测试；
- 手动发布触发、Cargo 稳定版本、Release Notes、`main` 提交和 tag 冲突的正反例；
- 三个目标各自的打包、解包和 `kb version --json` 验证；
- 归档名、内容、`SHA256SUMS` 和 `SHA256SUMS.minisig` 的可重复断言；
- 无签名凭据时不尝试 notarization 或 Authenticode；
- 发布 job 仅在三个 artifact 到齐时开始，并在失败重试时复用它们；
- `kb update check` 的稳定版本选择、目标选择、无写入与非官方安装拒绝；
- `kb update` 的签名、checksum、归档、版本、目标、替换、备份恢复与 Windows 锁重试正反例；
- publication job 在构建成功后创建 annotated tag 的顺序与同提交重试检查；
- GitHub 上一次真实发布的三平台构建、provenance 与公开 Release 资产检查。

本文件拥有发布自动化的设计和验收条件。实现后的命令由发布指南拥有，工作流细节由 `.github/workflows/` 拥有，用户下载步骤由 README 拥有。
