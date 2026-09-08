# 发布 CLI 版本

本指南面向有仓库发布权限的维护者。普通 push 和 pull request 不运行 GitHub Actions；验证由手动工作流启动，正式发布只由 annotated `vX.Y.Z` tag 启动。

## 首次密钥设置

1. 在受控的本机环境生成 Minisign 密钥对。私钥应使用强口令保护；不要在仓库目录或 shell 历史中保存私钥内容。
2. 将两行公钥文件保存为 `assets/release/kb-update.minisign.pub` 并提交。仓库中只能包含 public key。
3. 通过 GitHub 仓库的 Actions secret 设置 `KB_UPDATE_SIGNING_KEY`。也可以从不回显内容的标准输入运行 `gh secret set KB_UPDATE_SIGNING_KEY`。
4. 比较仓库公钥的 Minisign key ID 与本机私钥对应的公钥，确认后删除所有临时副本。

发布任务把 secret 写入权限为 `0600` 的临时文件，生成 `SHA256SUMS.minisig` 后删除该文件。日志不得输出 secret、私钥路径内容或解密口令。

## 发布前验证

1. 把根 `Cargo.toml` 的 workspace version 改为目标 `X.Y.Z`，更新 `Cargo.lock`，并提交到 `main`。
2. 在 Actions 中运行 **Verify one native platform**。分别选择 `linux`、`macos` 和 `windows`，或选择 `all`。
3. 确认每个平台完成 workspace 测试、release binary 构建和真实 CLI journey。手动验证不创建归档或 Release。
4. 某个平台失败时，在对应运行中选择 **Re-run failed jobs**。该操作只重跑失败任务及其依赖任务，不需要重新启动已成功的同级平台构建。

## 创建正式版本

1. 确认目标提交已在 `origin/main`，工作区版本为 `X.Y.Z`，三个平台的手动验证均成功。
2. 创建 annotated tag：`git tag -a vX.Y.Z -m "Knowledge-Brain vX.Y.Z"`。
3. 推送该 tag：`git push origin vX.Y.Z`。不要通过普通分支 push 代替发布触发。
4. 观察 **Publish tagged release**。校验任务会拒绝轻量 tag、版本不一致或无法从 `main` 到达的提交。
5. 三个平台各自运行 workspace 测试、官方 release binary 构建、CLI journey、打包、归档检查和 build provenance。发布任务只在三者全部成功后运行。
6. 确认公开 Release 正好包含三个平台归档、`SHA256SUMS` 和 `SHA256SUMS.minisig`，并检查三个 provenance 证明。

归档名称为：

- `knowledge-brain-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `knowledge-brain-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `knowledge-brain-vX.Y.Z-x86_64-pc-windows-msvc.zip`

发布脚本先创建或复用 draft Release。它拒绝修改已经公开的 Release；完整资产上传后才取消 draft。macOS notarization 和 Windows Authenticode 不在当前发布流程中。

## 失败处理

构建或打包失败时使用 **Re-run failed jobs**，不要重新推送或移动 tag。发布任务部分上传失败时可以重跑；脚本只允许覆盖仍处于 draft 状态的资产。

如果公开 Release 的代码或资产有误，不得移动、删除后复用或强推已有 tag。修正代码，把版本增加到新的 patch 版本，重新完成手动验证，并创建新的 annotated tag。

## 密钥轮换

计划内轮换先发布一个由旧私钥签名、但程序内嵌新公钥的过渡版本；确认过渡版本可更新后，再替换 `KB_UPDATE_SIGNING_KEY` 并发布下一个版本。旧私钥疑似泄露时停止自动发布，撤销仓库 secret，并要求用户依据独立渠道核对新公钥后手工安装。
