# Portable Agent Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship an embedded, host-neutral Knowledge-Brain Agent Skill and reviewable `kb skills detect|install|status|uninstall` workflows.

**Architecture:** `kb-app` owns host manifests, target resolution, byte-level preconditions, plans, and application. `kb-cli` only parses commands. The embedded Skill calls MCP when available and otherwise the stable CLI JSON interface; host bridges contain only a bounded pointer to `KB.md` and never replace user files.

**Tech Stack:** Rust 2024, serde/JSON, existing operation storage and atomic file utilities, Markdown Agent Skill assets.

**Spec:** `.superpowers/specs/2026-09-07-knowledge-brain-design.md` sections 13, 16, 17, and 20.

## Global Constraints

- Rust MSRV remains 1.85.
- The default installation mode is `copy`; `symlink` is accepted only when explicitly requested.
- Supported hosts are `codex`, `claude-code`, `gemini-cli`, and `opencode`; `auto` must resolve to one unambiguous host or fail without writing.
- Install and uninstall create reviewable operations and change files only through `kb apply`.
- Existing bridge content is preserved byte-for-byte outside Knowledge-Brain markers.
- Uninstall refuses to remove user-modified managed files or bridge blocks.
- Skill content contains no personal path, operating-system-only instruction, model name, or product-specific business logic.

---

### Task 1: Embedded portable Skill contract

**Files:**
- Create: `skills/knowledge-brain/SKILL.md`
- Create: `skills/knowledge-brain/references/query.md`
- Create: `skills/knowledge-brain/references/review-and-save.md`
- Create: `skills/knowledge-brain/references/maintenance.md`
- Create: `crates/kb-app/src/skill_assets.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Test: `crates/kb-app/tests/skill_assets.rs`

**Interfaces:**
- Produces: `skill_assets() -> &'static [SkillAsset]`, where each asset has one portable relative path, embedded bytes, and SHA-256.

- [ ] Write a failing test asserting the exact four asset paths, valid frontmatter, relative reference links, no personal path, and deterministic digest.
- [ ] Run `cargo test -p kb-app --test skill_assets` and confirm failure because the asset API is absent.
- [ ] Add the four concise Skill files and `skill_assets.rs` using `include_bytes!` plus existing SHA-256 helpers.
- [ ] Re-run the focused test and confirm it passes.
- [ ] Commit as `feat: embed portable knowledge brain skill`.

### Task 2: Host targets and detection

**Files:**
- Create: `crates/kb-core/src/skill.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Create: `crates/kb-app/src/skill_hosts.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Test: `crates/kb-app/tests/skill_hosts.rs`

**Interfaces:**
- Produces: `SkillHost`, `SkillScope`, `SkillInstallMode`, `SkillAction`, `SkillTarget`, and `detect_skill_hosts(root, user_paths)`.
- Canonical vault targets: Codex `.agents/skills/knowledge-brain`, Claude Code `.claude/skills/knowledge-brain`, Gemini CLI `.gemini/skills/knowledge-brain`, OpenCode `.opencode/skills/knowledge-brain`.

- [ ] Write failing table tests for all host/scope targets, auto detection, ambiguous auto detection, and portable path rejection.
- [ ] Run `cargo test -p kb-app --test skill_hosts` and verify the missing API failure.
- [ ] Implement closed enums with serde names and one manifest-driven resolver; make user roots derive from the platform user directory rather than fixed `/Users` or `%APPDATA%` strings.
- [ ] Re-run the focused tests.
- [ ] Commit as `feat: resolve portable skill hosts`.

### Task 3: Reviewable install and uninstall operations

**Files:**
- Create: `crates/kb-app/src/skill_plan.rs`
- Modify: `crates/kb-app/src/operation.rs`
- Modify: `crates/kb-app/src/operation_events.rs`
- Modify: `crates/kb-app/src/app.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Test: `crates/kb-app/tests/skill_plan.rs`
- Test: `crates/kb-app/tests/process_skill_recovery.rs`

**Interfaces:**
- Produces: `SkillRequest::{Detect, Install, Status, Uninstall}` through `AppRequest::Skills`.
- Produces durable `SkillPlan` and `SkillApplyResult` variants inspectable by `operation show` and executable by the existing `apply` entry.

- [ ] Write failing tests showing install only creates a plan, apply writes exactly four assets plus one bounded bridge block, repeat apply is idempotent, and unrelated bytes remain unchanged.
- [ ] Write failing tests showing status distinguishes absent/current/modified and uninstall refuses modified assets or bridge blocks.
- [ ] Write a process test that exits after each managed write and proves retry reaches either the complete old or complete new managed state without touching unrelated bytes.
- [ ] Run only `cargo test -p kb-app --test skill_plan --test process_skill_recovery` and verify the expected missing-variant failures.
- [ ] Implement plans with expected-before hashes, private operation storage, existing Vault locking, atomic replacement, progress, result receipts, and events. Materialize a canonical user copy before creating an explicitly requested directory symlink; never follow an existing link-shaped target.
- [ ] Re-run the two focused tests and commit as `feat: add reviewable skill installation`.

### Task 4: CLI surface and real workflow

**Files:**
- Modify: `crates/kb-cli/src/args.rs`
- Modify: `crates/kb-app/src/capabilities.rs`
- Create: `crates/kb-cli/tests/skills_journey.rs`

**Interfaces:**
- Produces: `kb skills detect`, `install --host <host|auto> --scope <vault|user> --mode <copy|symlink>`, `status`, and `uninstall`, all with `--vault` and `--json`.

- [ ] Write a failing real-binary journey: init Vault, detect Codex from `AGENTS.md`, plan copy install, inspect, apply, reopen status, plan uninstall, apply, and verify the original bridge bytes are restored.
- [ ] Run `cargo test -p kb-cli --test skills_journey` and confirm CLI parsing fails.
- [ ] Add clap parsing that maps directly to typed `AppRequest::Skills`; add `skills` to capabilities.
- [ ] Re-run the journey and commit as `feat: expose portable skill commands`.

### Task 5: Skill documentation and validation

**Files:**
- Create: `docs/reference/agent-skill.md`
- Modify: `docs/reference/commands.md`
- Modify: `README.md`
- Modify: `ROADMAP.md`
- Modify: `crates/kb-cli/tests/docs_contract.rs`

**Interfaces:**
- Documents host targets, plan/apply flow, copy/symlink semantics, cloud-agent disclosure, and safe uninstall.

- [ ] Add failing documentation-contract assertions for every `kb skills` command and the MCP-first/CLI-fallback rule.
- [ ] Run `cargo test -p kb-cli --test docs_contract` and verify failure.
- [ ] Write the reference and links without duplicating the Roadmap status.
- [ ] Validate `skills/knowledge-brain` with the available Skill validator and run the focused docs test.
- [ ] Commit as `docs: publish portable agent skill`.

