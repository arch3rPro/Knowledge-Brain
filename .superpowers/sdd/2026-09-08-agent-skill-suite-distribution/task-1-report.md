# Task 1 实施报告

## 状态

- 基线：`853527739db3075143a3149550981bef3c679572`
- 实施 commit：`d84d603b5b70dbe1a8f868e11757c61add658ccb`（`feat: publish task-scoped skill sources`）
- 工作树：`/Users/wangyu19/Documents/Knowledge-Brain/.worktrees/usable-agent-experience`
- 范围：仅 Task 1 的 canonical Skill、嵌入资产、旧 bundle 识别 fixture 和专项测试；未实现安装记录、安装器或 MCP 工作。

## 实施内容

1. 在 `skills/` 发布八个可独立安装的规范 Skill：`kb-vault`、`kb-config`、`kb-ingest`、`kb-query`、`kb-save`、`kb-ops`、`kb-backup`、`kb-connect`，顺序由公开的 `SKILL_NAMES` 固定。
2. 每个 Skill 都有 YAML front matter、任务范围、相同的 Vault 不可信输入和明确批准规则、读写边界，以及适用的 `kb`/MCP 动作。
3. `skill_assets()` 只嵌入八个 `kb-*/SKILL.md`；`legacy_skill_assets()` 单独导出四个冻结旧文件，供后续迁移检测。
4. 原 `skills/knowledge-brain` 的 Skill 与 references 以 100% 字节相同的 Git rename 移至 `crates/kb-app/assets/legacy-skill`，因此不再能被 `npx skills add --all` 作为根 Skill 选中。
5. 专项测试验证八项顺序、路径、front matter、安全措辞、可移植性、以及旧 fixture 的精确 SHA-256。

## TDD 记录

### RED：canonical 资产契约

命令：

```text
cargo test -p kb-app --test skill_assets
```

结果：退出码 101。旧实现返回 `SKILL.md` 和三个 `references/*` 文件，而测试要求八个 `kb-*/SKILL.md` 路径。

### GREEN：canonical 资产

同一命令通过：`embedded_skills_are_the_eight_canonical_task_scoped_sources` 与 `legacy_assets_remain_available_only_as_the_frozen_migration_fixture` 均通过。

### RED：冻结 legacy 字节

将 HEAD 旧文件的 SHA-256 写入 fixture 测试后，同一命令退出码 101。诊断显示四个文件都只缺少原件末尾空行，导致散列变化。

### GREEN：冻结 legacy 字节

以 Git 的精确 rename 迁移旧文件后，同一命令通过；四个源文件与 fixture 的 SHA-256 两两相同。

## 最终专项验证

```text
cargo test -p kb-app --test skill_assets
cargo fmt --all -- --check
find skills -mindepth 2 -maxdepth 2 -name SKILL.md | sort
git diff --check
```

结果：资产测试 2 passed、0 failed；Rustfmt 和 diff 检查通过；`find` 只输出八个 `skills/kb-*/SKILL.md` 路径。

## 改动文件

- `skills/kb-vault/SKILL.md`
- `skills/kb-config/SKILL.md`
- `skills/kb-ingest/SKILL.md`
- `skills/kb-query/SKILL.md`
- `skills/kb-save/SKILL.md`
- `skills/kb-ops/SKILL.md`
- `skills/kb-backup/SKILL.md`
- `skills/kb-connect/SKILL.md`
- `crates/kb-app/assets/legacy-skill/SKILL.md`
- `crates/kb-app/assets/legacy-skill/references/maintenance.md`
- `crates/kb-app/assets/legacy-skill/references/query.md`
- `crates/kb-app/assets/legacy-skill/references/review-and-save.md`
- `crates/kb-app/src/skill_assets.rs`
- `crates/kb-app/src/lib.rs`
- `crates/kb-app/tests/skill_assets.rs`

## 残留风险

- 按任务指令未运行 workspace 全量测试，也未进行 `npx skills add`、安装记录、安装器或 MCP 的端到端验证；这些属于后续 Task 2–4。
- 现有安装流程的多-Skill 目录布局与 legacy 迁移识别的运行时接入，由后续任务负责验证；本任务只提供其所需的 canonical 与冻结资产边界。

## Task 2 follow-up: review findings resolved

Task 2 实现已将八份 `kb-*` 安装为各 host 的 `skills/` 根下可发现的顶层目录，并以 durable ownership record 管理 copy 与 symlink suite。冻结 legacy fixture 现已接入运行时识别：完整字节匹配为 `legacy`，任何偏差为 `modified`；完整 legacy bundle 可迁移至新 suite 并可经受管 suite 卸载。资产专项测试也已扩展为逐 Skill canonical bytes、完整共享安全块、专属动作边界，以及发布树精确八项的合同。
