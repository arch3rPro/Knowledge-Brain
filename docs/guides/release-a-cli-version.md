# 发布 CLI 版本

本指南面向有仓库发布权限的维护者。普通 push、pull request 和 tag push 均不运行发布任务。正式版本由一次手动工作流完成测试、构建、创建 tag 和发布。

## 首次密钥设置

1. 在受控的本机环境生成 Minisign 密钥对。私钥应使用强口令保护；不要在仓库目录或 shell 历史中保存私钥内容。
2. 将两行公钥文件保存为 `assets/release/kb-update.minisign.pub` 并提交。仓库中只能包含 public key。
3. 通过 GitHub 仓库的 Actions secrets 设置私钥内容 `KB_UPDATE_SIGNING_KEY` 和私钥口令 `KB_UPDATE_SIGNING_PASSWORD`。也可以分别从不回显内容的标准输入运行 `gh secret set`。
4. 比较仓库公钥的 Minisign key ID 与本机私钥对应的公钥，确认后删除所有临时副本。

发布任务把私钥 secret 写入权限为 `0600` 的临时文件，通过标准输入提供口令，生成 `SHA256SUMS.minisig` 后删除该文件。日志不得输出 secret、私钥路径内容或解密口令。

## 可选的平台验证

**Verify one native platform** 用于发布前诊断，并不是正式发布的必经步骤。可以选择 `linux`、`macos`、`windows` 或 `all`；它不会创建 tag、归档或 Release。

## 创建正式版本

1. 把根 `Cargo.toml` 的 workspace version 改为目标 `X.Y.Z`，更新 `Cargo.lock`，编写 `docs/releases/vX.Y.Z.md`，并将这些改动提交、推送到 `main`。
2. 在 GitHub Actions 中手动运行 **Release CLI version**，分支选择 `main`。工作流不接收单独的版本参数。
3. 工作流先确认运行提交就是当前 `origin/main`，再从 `Cargo.toml` 读取版本并检查版本说明。
4. 质量检查和 Linux、macOS、Windows 三个平台在同一次运行中完成测试、构建、CLI journey、打包和 build provenance。
5. 三个平台全部成功后，发布任务才创建 annotated `vX.Y.Z` tag、签名校验和、上传资产并公开 Release。
6. 确认公开 Release 正好包含三个平台可执行文件、三个完整归档、`SHA256SUMS` 和 `SHA256SUMS.minisig`，并检查 provenance 证明和版本说明。

可执行文件名称为：

- `knowledge-brain-vX.Y.Z-x86_64-unknown-linux-gnu`
- `knowledge-brain-vX.Y.Z-aarch64-apple-darwin`
- `knowledge-brain-vX.Y.Z-x86_64-pc-windows-msvc.exe`

归档名称为：

- `knowledge-brain-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `knowledge-brain-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `knowledge-brain-vX.Y.Z-x86_64-pc-windows-msvc.zip`

发布脚本从 `docs/releases/vX.Y.Z.md` 读取版本说明。只有三个可执行文件、三个归档、`SHA256SUMS` 和 `SHA256SUMS.minisig` 全部存在时才创建 tag；完整资产上传后才取消 draft。它拒绝修改已经公开的 Release。macOS notarization 和 Windows Authenticode 不在当前发布流程中。

## 失败处理

构建或打包失败时使用 **Re-run failed jobs**。矩阵中已经成功的平台不会重新构建。此时尚未创建版本 tag。

发布任务在创建 tag 后上传失败时，同样使用 **Re-run failed jobs**。它会复用本次运行的内部 artifact，并且只接受指向同一运行提交的 annotated tag；脚本只允许覆盖仍处于 draft 状态的资产。

如果公开 Release 的代码或资产有误，不得移动、删除后复用或强推已有 tag。修正代码，把版本增加到新的 patch 版本，再启动一次正式发布。

## 密钥轮换

计划内轮换先发布一个由旧私钥签名、但程序内嵌新公钥的过渡版本；确认过渡版本可更新后，再替换两个签名 secrets 并发布下一个版本。旧私钥疑似泄露时停止自动发布，撤销仓库 secrets，并要求用户依据独立渠道核对新公钥后手工安装。
