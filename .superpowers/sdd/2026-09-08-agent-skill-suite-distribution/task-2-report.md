# Task 2 实施报告

## 范围

实现 Plan 3 Task 2 的安装归属和多-Skill suite；未涉及 Task 3、Task 4 或 MCP。

## 实施内容

- `SkillTarget` 以 `skills_root`、`legacy_skill_dir` 和 bridge 文件描述 host 目标；copy 与 symlink 都将八项 `kb-*` 直接安装到可发现的 Skills 根。
- 增加原子 ownership record。Vault scope 写入 `<state>/skill-installations/vault/<vault-id>/<host>.json`，User scope 写入 `<state>/skill-installations/user/<host>.json`。
- 仅有匹配 ownership record 的 suite 才是受管：缺项为 `partial`，字节、bridge、link 或记录不匹配为 `modified`；无记录的 `kb-*` 为 `external` 且拒绝覆盖。
- 对冻结 legacy bundle 实行严格字节匹配：完整 bundle 为 `legacy`，变更即 `modified`。安装会迁移完整 legacy bundle；卸载只删除记录或冻结 legacy 中列出的受管路径。
- `SkillPlan.links` 支持多链接，`all_links()` 同时读取新字段及保留的旧 `link`，并已接入计划摘要、事件进度、预检和 apply。

## TDD 与验证

先扩展 Skill target、canonical asset、发布树和真实 `kb skills` 工作流测试；旧单目录实现先因缺少 `skills_root` / `legacy_skill_dir` 编译失败，随后按 RED→GREEN 实现 suite 和 ownership 行为。

最终运行（未运行全量 workspace 测试）：

```text
cargo test -p kb-app --test skill_assets --test skill_hosts --test skill_plan --test operation_summary --test operation_events
# 33 passed, 0 failed

cargo fmt --all -- --check
git diff --check
```

## 覆盖的工作流

- copy 安装、状态、卸载与 bridge 字节保留；
- 八项顶层 discoverable Skill 及精确发布树；
- symlink suite 的八个独立 links 与卸载；
- External 拒绝且原始 bytes 不变；
- Current / Partial / Modified / Legacy 状态；
- 完整 legacy 迁移、篡改 legacy 拒绝；
- User scope ownership 跨 Vault 选择保持受管；
- 旧单 `link` 计划反序列化和 multi-link 事件/摘要计数。
