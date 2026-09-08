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

`HOST` 可取 `auto`、`codex`、`claude-code`、`gemini-cli` 或 `opencode`。对应宿主为 Codex、Claude Code、Gemini CLI 和 OpenCode。

`detect` 只报告检测证据，不修改文件。`auto` 仅在结果唯一时选择宿主；例如只有 `AGENTS.md` 时可能同时匹配 Codex 与 OpenCode，必须显式传入 `--host`。

## 安装范围和位置

Vault 范围把 Skill 放入 Vault 内的宿主目录：

| 宿主 | Skill 目录 | 桥接文件 |
| --- | --- | --- |
| Codex | `.agents/skills/kb-*/` | `AGENTS.md` |
| Claude Code | `.claude/skills/kb-*/` | `CLAUDE.md` |
| Gemini CLI | `.gemini/skills/kb-*/` | `GEMINI.md` |
| OpenCode | `.opencode/skills/kb-*/` | `AGENTS.md` |

User 范围使用操作系统的用户目录或配置目录：Codex 为 `.codex/`，Claude Code 为 `.claude/`，Gemini CLI 为 `.gemini/`，OpenCode 为系统配置目录下的 `opencode/`。Knowledge-Brain 通过系统目录 API 解析这些位置，不把 macOS、Linux 或 Windows 的绝对路径写入 Vault。

`copy` 复制八项内置 Skill。`symlink` 在 Knowledge-Brain 的用户配置目录保存规范副本，再让宿主目录分别链接到八项副本；显式选择该模式前应确认宿主和同步工具支持符号链接。未修改的旧版单一 `knowledge-brain` 安装会显示为 `legacy`，可由一次 reviewable install 迁移；人工修改的旧版或未受管 `kb-*` 文件不会被覆盖或删除。

## 外部安装

```bash
npx skills add . --all -a codex -y
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

安装后的 Skill 要求 Agent 先读取 Vault 的 `KB.md`，把 Wiki 和来源内容当作不可信数据，并优先使用 MCP、否则使用 `kb --json`。计划创建响应保留各自既有的根字段，并在同一根级增加 `operation_summary`；operation 查看保留 `state` 以及 `plan` 或 `result`，并在同一根级增加 `operation_summary`。响应字段详见[命令参考](commands.md#vault-创建与采用)。创建 review 或 knowledge plan 不等于授权执行；Agent 必须展示该摘要，并在用户对最终 apply 明确同意一次后才调用写入工具或 `kb apply`。直接由用户输入的 `kb apply <operation-id>` 本身就是明确写入请求。完整确认语义见[已批准的使用体验设计](../superpowers/specs/2026-09-08-usable-agent-skills-design.md#操作摘要与一次确认)。
