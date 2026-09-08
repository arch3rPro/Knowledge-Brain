# CLI 更新参考

本页定义官方 Knowledge-Brain CLI 的检查、验证和替换行为。发布维护步骤见[发布 CLI 版本](../guides/release-a-cli-version.md)。

## 命令

```text
kb update check [--json]
kb update [--json]
```

`kb update check` 只读取 GitHub 最新稳定 Release，不写入安装目录。`kb update` 在存在更高版本时下载、验证并安排替换；命令返回 `status: scheduled` 后，旧进程退出，辅助进程才替换可执行文件。两条命令都只在用户明确调用时联网，不随其他 `kb` 命令启动，不在后台轮询。

JSON 响应包含 `current`、`latest`、`update_available` 和 `status`。没有更高版本时，`status` 为 `checked`，安装内容不变。

## 支持的安装来源

自更新只适用于 Knowledge-Brain GitHub Releases 发布的官方二进制。官方构建内嵌目标平台和 Minisign public key，并在 `kb version --json` 的 `distribution` 字段中报告 `official_release: true`。

通过 Cargo、源码编译或第三方包管理器安装的程序由原安装工具管理。它们运行更新命令时返回 `capability_unavailable`，不会访问更新端点或修改文件。

## 验证顺序

更新器只接受最新、非 draft、非 prerelease 的稳定 Release，并按以下顺序处理：

1. 使用内嵌 public key 验证 `SHA256SUMS.minisig` 的 Minisign 签名。
2. 从已经验证的 `SHA256SUMS` 中读取当前平台归档的唯一 SHA-256。
3. 下载归档并核对完整摘要。
4. 只解压 `kb` 或 `kb.exe`、`LICENSE`、`INSTALL.md` 三个普通文件；拒绝额外文件、重复条目、链接和路径逃逸。
5. 运行暂存的 `kb version --json`，核对版本、官方发行标记和目标平台。
6. 旧进程退出后保留当前程序的同目录备份，放入新程序，再核对最终可执行文件摘要。全部成功后才删除备份。

签名、摘要和 GitHub build provenance 解决不同问题：Minisign 授权更新清单，SHA-256 检测字节变化，provenance 记录 GitHub Actions 的构建来源。当前版本没有 macOS notarization 或 Windows Authenticode；操作系统仍可能显示未签名程序提示。

## 失败与恢复

签名、摘要、归档内容或暂存程序身份不匹配时返回 `update_verification_failed`，当前程序不会被替换。网络错误、目录权限不足和替换错误使用相应的 I/O 错误响应。

替换辅助进程使用 PID 与进程启动时间识别原进程；Windows 对共享冲突执行有界重试。新程序无法放入目标位置或最终摘要不一致时，辅助进程恢复备份。恢复本身失败时，错误详情会给出保留的备份路径，用户应先保留该文件并检查安装目录权限。

更新需要对当前可执行文件所在目录具有创建、重命名和删除权限。更新阶段目录使用随机名称和私有权限，并由新程序在辅助进程退出后清理。

## 公钥轮换

正常轮换需要一个由旧私钥签名的过渡版本；该版本嵌入新 public key，后续版本再改用新私钥签名。旧私钥疑似泄露时，不能依赖旧信任链自动轮换，用户必须从项目发布页手工安装并独立核对新的公钥和 provenance。
