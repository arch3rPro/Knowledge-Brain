# Portable Agent Skills

Knowledge-Brain 发布八个可独立选择的可移植 `kb-*` Skills：`kb-vault`、`kb-config`、`kb-ingest`、`kb-query`、`kb-save`、`kb-ops`、`kb-backup` 与 `kb-connect`。它们使用统一的 MCP 或 CLI 契约，并明确区分“生成计划”和“执行计划”。Skill 不包含个人目录、操作系统路径或某个模型专用提示。

有两种安装方式：`kb skills` 创建可审阅、可执行的受管理 suite；`npx skills add` 从本项目的顶层 `skills/` 目录安装外部副本。两者互不接管文件。

## 命令

```text
kb skills detect [--vault <PATH_OR_ID>] [--json]
kb skills install [--host <HOST>] [--scope vault|user] [--mode copy|symlink] [--vault <PATH_OR_ID>] [--json]
kb skills status [--host <HOST>] [--scope vault|user] [--vault <PATH_OR_ID>] [--json]
kb skills uninstall [--host <HOST>] [--scope vault|user] [--vault <PATH_OR_ID>] [--json]
```

`HOST` 可取 `auto`、`codex`、`claude-code`、`gemini-cli`、`opencode`、`openclaw`、`hermes`、`dsh` 或 `pi`。命令也接受 `hermes-agent`、`deepseek-harness` 和 `pi-coding-agent` 别名；状态和受管理记录始终使用短 ID。

`detect` 只报告检测证据，不修改文件。`auto` 仅在结果唯一时选择宿主；例如只有 `AGENTS.md` 时可能同时匹配 Codex 与 OpenCode，必须显式传入 `--host`。

## 安装范围和位置

Vault 范围把 Skill 放入 Vault 内的宿主目录：

| 宿主 | Skill 目录 | 桥接文件 |
| --- | --- | --- |
| Codex | `.agents/skills/kb-*/` | `AGENTS.md` |
| Claude Code | `.claude/skills/kb-*/` | `CLAUDE.md` |
| Gemini CLI | `.gemini/skills/kb-*/` | `GEMINI.md` |
| OpenCode | `.opencode/skills/kb-*/` | `AGENTS.md` |
| OpenClaw | `skills/kb-*/` | 无 |
| DeepSeek Harness | `.dsh/skills/kb-*/` | 无 |
| Pi | `.pi/skills/kb-*/` | 无 |

User 范围使用操作系统用户目录或配置目录：Codex 为 `.codex/skills`，Claude Code 为 `.claude/skills`，Gemini CLI 为 `.gemini/skills`，OpenCode 为系统配置目录下的 `opencode/skills`，OpenClaw 为 `.openclaw/skills`，Hermes Agent 为 `.hermes/skills`，DeepSeek Harness 为 `.dsh/skills`，Pi 为 `.pi/agent/skills`。Knowledge-Brain 通过系统目录 API 解析用户位置，不把某一操作系统的绝对路径写入 Vault。

Hermes 的 Vault 范围不会被伪装成普通目录安装：Hermes 只在受信任的 Git checkout 中启用项目 Skill，而 Knowledge-Brain Vault 不要求 Git，因此 `--host hermes --scope vault` 返回明确的不支持结果并提示使用 User 范围。新增宿主使用其原生 Skill 目录，不创建无官方含义的桥接文件。

`copy` 复制八项内置 Skill 的完整目录，包括实际存在的 `references/`、`assets/` 或 `scripts/`。`symlink` 在 Knowledge-Brain 的用户配置目录保存同一份规范目录，再让宿主目录分别链接到八项副本；显式选择该模式前应确认宿主和同步工具支持符号链接。升级、状态检查与卸载逐文件核对嵌套资源，人工修改的文件不会被覆盖或删除。未修改的旧版单一 `knowledge-brain` 安装会显示为 `legacy`，可由一次 reviewable install 迁移。

## 外部安装

```bash
npx skills add . --skill '*' --agent codex --yes
npx skills add . --skill kb-query -a codex -y
```

`npx` 安装的文件在 `kb skills status` 中显示为 `external`。内置安装器拒绝覆盖或卸载它们；请使用最初的外部安装工具管理这些文件。`kb init` 只创建 Vault，不会安装任何宿主 Skill。

## 审阅与执行

`install` 和 `uninstall` 都只创建 operation 计划，返回 `operation_id`，不会立刻修改宿主文件：

```bash
kb skills install --host gemini-cli --scope vault --vault ./my-knowledge --json
kb operation show <operation-id> --json
kb apply <operation-id> --json
kb skills status --host gemini-cli --scope vault --vault ./my-knowledge --json
```

桥接说明写在有边界标记的受管理区块中，已有内容保持原字节不变。卸载仅移除内容未被修改的受管理文件、链接和桥接区块；检测到人工修改时会拒绝覆盖或删除。中断后可用同一个 operation ID 重试，已经核对并写入的相同文件不会重复产生影响。

## Agent 行为边界

安装后的 Skill 按任务分别要求 Agent 解析 Vault、读取 `KB.md`、选择对应 CLI 或 MCP 入口，并以持久化结果作为完成证据。普通笔记遵循 `KB.md` 的用户定义主题目录和命名规则，不触发 `kb-ingest` 或 `kb-save`；只有明确来源入库才触发 `kb-ingest`，只有明确写入 Wiki 或整理进知识库才触发 `kb-save`。各 Skill 的 `description` 同时声明正向意图和相邻边界；仓库维护中英文正向、相邻和反向请求语料用于真实宿主触发评估。计划创建响应保留各自既有根字段并增加 `operation_summary`；创建计划不等于授权执行。响应字段和确认边界见[命令参考](commands.md#vault-创建与采用)。

宿主目录约定依据各项目公开文档：[OpenClaw Skills](https://docs.openclaw.ai/skills)、[Hermes Agent Skills](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/guides/work-with-skills.md)、[DeepSeek Harness Skills](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/skills.md) 与 [Pi Skills](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)。目录适配完成不等于宿主触发行为已经验证；真实加载状态见路线图。
