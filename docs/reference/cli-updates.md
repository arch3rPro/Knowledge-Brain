# 更新参考

本页是 Knowledge-Brain 更新行为的唯一完整参考。发布维护步骤见[发布 CLI 版本](../guides/release-a-cli-version.md)。

## 命令

```text
kb update check [--vault <PATH_OR_ID>] [--exclude-vault <ID>]... [--json]
kb update [--vault <PATH_OR_ID>] [--exclude-vault <ID>]... [--json]
kb update --confirm <TOKEN> --json
kb update status [OPERATION_ID] [--json]
```

`kb update check` 生成完整预览，但不保存操作、不写入 Vault 或安装目录。检查官方新版本时，下载和验证使用临时目录，命令结束后删除临时内容。

裸 `kb update` 检查并显示官方 Release 可执行文件、所选 Vault 的产品模板与可重建搜索索引，以及已有 Knowledge-Brain 管理记录的 Agent Skills、桥接文件和符号链接。

人类输出先显示完整计划。选择多个 Vault 时，可以输入要排除的 Vault ID；命令重新生成并再次显示精确计划。随后只询问一次 `Apply this exact update plan? [y/N]`。拒绝或输入结束会删除未确认预览，不执行任何计划变更。

`--json` 不读取标准输入。第一次调用返回 `phase: preview`、`operation_id`、完整组件列表和 `confirmation_token`；调用方必须把所有变化展示给用户，并在获得确认后把原样 token 传给 `kb update --confirm <TOKEN> --json`。token 绑定完整计划，错误、过期或对应文件变化时不会改用新状态继续执行。

`kb update status` 返回最新持久更新状态；提供操作 ID 时返回该操作。可执行文件替换后，以 status 记录为最终结果，不以辅助进程已启动作为成功。

## Vault 范围

在 Vault 内运行时，默认只选择当前 Vault；在 Vault 外运行时，默认选择本机已注册且路径有效的 Vault。`--vault` 明确选择一个路径或稳定 `vault_id`，`--exclude-vault` 可重复使用。不同设备上的 Vault 路径可以不同，持久计划以稳定 ID 记录身份，并在写入前重新核对本机路径和 Vault ID。

固定 Vault 的 MCP 和 HTTP 适配器只能为自己的 Vault 生成、查询和确认更新，不能通过请求指定或读取另一个 Vault。长时间运行的服务不会替换正在运行的服务程序；其可执行组件明确标为跳过，但仍可使用已验证的新版本程序规划并更新该 Vault 的受管理模板、索引和 Skills。服务程序本身通过本机 CLI 或服务管理方式更新。

## 管理边界

更新只修改计划中列出的 Knowledge-Brain 管理内容：

- `KB.md` 只替换带 `kb:rules` 标记的规则区块；
- `.kb/template.yml` 和其他整文件模板只在内容仍等于已知历史基线时更新；
- 搜索索引可删除并重建，因为它不是知识源；
- Skills 只更新具有有效本机管理记录、且实际文件仍等于记录基线的安装。

主题目录、`admission.yml`、普通笔记、Wiki 正文、Git、`.obsidian/`、外部安装的 Skills 以及未知文件不由更新流程接管。缺失的可选 Skill 不会自动安装；通过 `npx skills add` 或其他工具管理的 Skill 继续由原工具更新。冲突组件会跳过并保留原内容，其他独立组件可以继续；最终状态为 `completed_with_skips`、`partial` 或 `failed`，不会把部分结果报告为完整成功。

通过 Cargo、源码或第三方包管理器安装的可执行文件继续由原安装方式管理。`kb update` 会把可执行组件标为跳过，但仍检查当前版本能够安全管理的 Vault 模板、索引和已有 Skills。

## 官方版本验证

官方构建内嵌目标平台和 Minisign public key，并在 `kb version --json` 中报告 `distribution.official_release: true`。更新器只接受最新、非 draft、非 prerelease 的稳定 Release：

1. 使用内嵌 public key 验证 `SHA256SUMS.minisig`。
2. 从已验证的 `SHA256SUMS` 读取当前平台归档的唯一 SHA-256。
3. 下载归档并核对摘要。
4. 只接受 `kb` 或 `kb.exe`、`LICENSE`、`INSTALL.md`，拒绝额外文件、重复条目、链接和路径逃逸。
5. 运行暂存程序并核对版本、官方发行标记和目标平台。
6. 让已验证的目标程序使用自己的模板和 Skills 生成计划。
7. 确认后，辅助进程核对原程序、备份并替换，再核对安装摘要；失败时恢复备份。
8. 新程序继续处理模板、Skills 和索引，并逐组件保存结果。

Minisign 授权更新清单，SHA-256 检测字节变化，GitHub build provenance 记录构建来源。当前没有 macOS notarization 或 Windows Authenticode，操作系统可能显示未签名程序提示。

## 网络与失败恢复

更新仅在显式运行 `kb update check` 或 `kb update` 时联网，不随其他命令启动，也不后台轮询。请求仅接受 HTTPS，并设有 60 秒请求超时和 128 MiB 下载上限。连接中断、超时、提前 EOF、HTTP 408、429、500、502、503 和 504 最多尝试三次；退避等待为 1 秒和 2 秒，总等待不超过 3 秒。证书错误、404 和非 HTTPS 地址不重试。错误只显示阶段、主机、尝试次数和脱敏原因。

底层 HTTP 客户端使用系统环境中的标准代理设置；可按运行环境配置 `HTTP_PROXY`、`HTTPS_PROXY` 和 `NO_PROXY`。代理和 TLS 失败不会被误报为“设备断网”。

签名、摘要、归档内容或暂存程序身份不匹配时返回 `update_verification_failed`，不会替换当前程序。

确认后的操作和逐组件结果保存在本机状态目录。进程中断后可用 `kb update status` 查看结果，再次运行相同范围的 `kb update` 会返回尚未结束的操作，不会静默创建第二份计划。文件在确认后发生变化时返回 `plan_stale` 并把该组件标记为 `stale`，不会覆盖；索引重建失败标记为 `rebuild_required`。可执行文件替换失败时尽可能恢复旧程序并把操作标记为 `failed`。

## v0.1.3 首次跳转

v0.1.3 使用旧更新器：它只更新 CLI，并返回 legacy `scheduled`，不能预先列出 Vault、Skills 和索引变化，也不能报告统一操作的最终状态。从 v0.1.3 更新到包含本页流程的版本后，再运行一次 `kb update`，用于检查并协调本机受管理组件。

## 公钥轮换

正常轮换先发布由旧私钥签名、但嵌入新 public key 的过渡版本，后续版本再使用新私钥。旧私钥疑似泄露时，停止依赖旧信任链；用户必须从项目发布页手工安装，并通过独立渠道核对新公钥和 provenance。
