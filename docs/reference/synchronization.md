# 外部同步兼容

Knowledge-Brain 不提供内置同步服务。Git 是推荐但可选的同步来源；命令行 Git、桌面 Git 客户端和 Obsidian Git 插件遵循相同的 Vault 文件规则。Obsidian Sync、Syncthing 和其他目录同步工具也可以使用。`kb` 不会替用户初始化仓库，也不会执行拉取、提交、推送、合并或冲突取舍。

## 同步检查

```text
kb sync check [--vault <PATH_OR_ID>] [--json]
```

该命令只读取 Vault 和可用的 Git 状态，检查冲突标记、Git 未合并条目、本机文件是否被 Git 跟踪、跨平台文本规则和 Wiki 错误。Git 不可用或目标不是 Git 仓库时，Vault 文件检查仍会完成；非 Git Vault 不是错误。Git 仓库没有配置 upstream 时按本地历史处理，也不会阻止写入。

`kb sync check` 不执行网络请求。外部 `git fetch` 刷新远程跟踪引用后，检查会比较当前 `HEAD` 与 upstream：落后报告 `git_branch_behind`，本地与上游各有提交报告 `git_branch_diverged`，两者都会阻止 Knowledge-Brain 的共享写入；仅本地领先不阻止写入。无法比较已配置的 upstream 时也会保守阻止写入并报告 `git_upstream_state_unavailable`。

检查结果只能说明本机最近一次 fetch 所知的状态，不能永久证明远程仓库是最新版本，也不会修改 `.gitignore`、`.gitattributes`、Git 索引或 Vault 文件。推荐在共享写入前 fetch、整合并检查，在写入后再次 fetch，再用普通 push 作为最后的并发检查。Knowledge-Brain 不会替用户 pull、merge、rebase、commit、push 或 force push。

## 跨设备路径

`.kb/config.yml` 中的 `vault_id` 是可同步身份。Vault 根目录的绝对路径只保存在每台设备自己的注册表中，不写入共享配置。相同 Vault 可以位于不同路径和盘符：

```text
macOS:  /Users/name/Documents/Knowledge-Vault
Linux:  /home/name/notes/Knowledge-Vault
Windows: D:\Notes\Knowledge-Vault
```

新设备取得 Vault 后，可以直接在 Vault 内运行命令，也可以登记本机路径：

```text
kb vault register <LOCAL_VAULT_PATH>
kb sync check --vault <LOCAL_VAULT_PATH>
```

同一设备移动 Vault 后使用 `kb vault rebind <VAULT_ID> <NEW_LOCAL_PATH>`。登记和重新绑定都不修改 Vault 内容。

## 共享内容与本机内容

应同步的内容包括 `KB.md`、`admission.yml`、获准主题目录、`Wiki/`、`.kb/config.yml`、`.kb/template.yml`、便携 schema、来源记录和原始来源对象。用户可以选择同步 Obsidian 的通用设置、主题、片段和插件配置。

下列内容默认只属于当前设备：

- `.kb/config.local.yml`
- `.kb/cache/`，包括 BM25 索引
- `.kb/runtime/` 和未完成操作状态
- 用户级 Vault 注册表、操作计划、凭据和宿主缓存
- `.obsidian/workspace.json`、`workspace-mobile.json`、`workspaces.json`
- `.trash/` 和操作系统临时文件

外部同步改变知识内容后，直接 Markdown 查询仍可使用。BM25 是本机派生索引，需要时在当前设备运行 `kb cache rebuild`。

## 推荐 Git 规则

推荐在 Vault 根目录的 `.gitignore` 中加入：

```gitignore
/.kb/config.local.yml
/.kb/cache/
/.kb/runtime/
/.obsidian/workspace.json
/.obsidian/workspace-mobile.json
/.obsidian/workspaces.json
/.trash/
.DS_Store
Thumbs.db
Desktop.ini
```

推荐在 `.gitattributes` 中固定便携文本的换行符，并禁止 Git 改写原始来源对象：

```gitattributes
*.md text eol=lf
*.yml text eol=lf
*.yaml text eol=lf
*.json text eol=lf
/Wiki/external-sources/.objects/** -text
```

给已有仓库增加换行规则可能产生大量规范化差异。Knowledge-Brain 只报告建议，不自动执行 Git 重新规范化。

## Obsidian

Vault 可以直接在 Obsidian 中打开。Knowledge-Brain 生成普通 Markdown 链接，不要求 Obsidian 专用语法。Obsidian Git 插件只是 Git 操作入口，不需要 Knowledge-Brain 特殊接管。

用户可以同时安装或启用其他同步工具。检测到可识别的并发写入风险时，Knowledge-Brain 给出警告而不禁止用户行为。Obsidian 和外部同步工具不参与 Knowledge-Brain 的本机文件锁，因此确认写入前仍会重新检查目标内容；目标变化会使原计划失效。

## 冲突处理

- 普通笔记由用户通过选定的同步工具解决冲突。
- `KB.md`、`admission.yml`、共享配置和受管理 Wiki 文件存在冲突时，Knowledge-Brain 最终写入返回 `sync_conflict`。
- 查询、同步检查和写入预览仍然可用；解决冲突后需要重新检查并确认写入。
- 相同哈希路径下的来源对象必须保持完全相同的字节，不能按文本合并。
- Knowledge-Brain 不使用文件时间戳自动选择某台设备的版本。

`kb sync check` 只阻止 Knowledge-Brain 自己可能破坏共享状态的最终写入，不拦截 Git、Obsidian 或其他文件工具。

## 直接修改 Wiki 的检测与恢复

Agent 不应直接写入 `Wiki/`。明确的创建、修改、移动和删除使用 `kb knowledge save`，由同一次受管理保存维护正文、索引和日志。

人类仍可在 Obsidian 或编辑器中修改 Markdown。`kb lint` 会报告受管理分区中的普通页面 `unmanaged_wiki_page`，以及未出现在生成索引中的受管理页面 `index_missing_entry`；冲突标记、重复标题和无效链接沿用现有检查。对有效受管理正文的人工内容修改不会仅因“不是 CLI 写的”而报错，因为 Markdown 保持人可读写。

发现问题后先审阅差异，再通过 `kb knowledge save` 纳入、移动、删除或重写对应页面。Knowledge-Brain 的中断恢复只处理未完成的受管理操作，不会自动撤销一次已经成功的人工作文；需要恢复历史正文时使用已启用的 Git 历史或 Knowledge-Brain 备份。没有 Git 和备份时，工具不会猜测旧内容。
