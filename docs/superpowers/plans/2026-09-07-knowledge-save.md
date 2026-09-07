# Knowledge Save Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement reviewed knowledge plans and all-or-restore saving for managed research/article Markdown with derived index and log updates.

**Architecture:** Typed request and plan values live in `kb-core`; `kb-app` validates requests, derives index/log outputs, persists plans, and runs the recoverable save workflow. `kb-cli` only loads request JSON, maps commands, and renders the shared application response.

**Tech Stack:** Rust 2024, existing OKF parser, SHA-256, serde JSON/YAML, atomic file replacement, OS file locks.

**Spec:** `docs/superpowers/specs/2026-09-07-knowledge-save.md`

## Global Constraints

- Rust MSRV remains 1.85.
- Only `Wiki/research/**/*.md` and `Wiki/articles/**/*.md` are direct request targets.
- Source records and original objects are never modified by a knowledge plan.
- Every save either reaches the complete planned state or restores the complete prior state.
- CLI, future MCP/HTTP, and future GUI/WebUI share the same application request and report.
- Run only focused tests for this task; do not run the full workspace suite.

---

### Task 1: Knowledge request and plan contract

**Files:**
- Modify: `crates/kb-core/src/operation.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Test: `crates/kb-core/tests/knowledge_plan.rs`

**Interfaces:**
- Produces: `KnowledgePlanRequest`, `KnowledgeChangeRequest`, `KnowledgePlan`, `KnowledgeWrite`, `KnowledgePlanResult`, and `OperationKind::SaveKnowledge`.
- Produces: validation for schema, portable target paths, hashes, summaries, duplicate targets, and size/count limits.

- [x] Write failing contract tests for valid create/update requests and every rejected path/hash/summary boundary.
- [x] Run `cargo test -p kb-core --test knowledge_plan` and confirm the API is missing.
- [x] Implement the typed values and deterministic request validation.
- [x] Run the focused core test until it passes.
- [x] Commit the contract and tests.

### Task 2: Plan creation, managed index, and managed log

**Files:**
- Create: `crates/kb-app/src/knowledge_plan.rs`
- Create: `crates/kb-app/src/managed_markdown.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Modify: `crates/kb-app/src/operation.rs`
- Test: `crates/kb-app/tests/knowledge_plan.rs`

**Interfaces:**
- Consumes: OKF parsing/validation, source records, read budgets, operation storage.
- Produces: `create_knowledge_plan(root, user_paths, config, request, now) -> Result<KnowledgePlan, KbError>`.
- Produces: deterministic managed-region rendering that preserves bytes outside the markers.

- [x] Write failing tests for create/update preconditions, strict managed documents, exact source history, marker corruption, deterministic index ordering, log entries, preserved human regions, stored plan digest, and readable diff.
- [x] Run `cargo test -p kb-app --test knowledge_plan` and confirm failure from missing workflow.
- [x] Implement request preflight and candidate document validation.
- [x] Implement managed index/log derivation and exact write snapshots.
- [x] Persist the plan and digest in the user operation directory.
- [x] Run the focused app test until it passes.
- [x] Commit plan creation and tests.

### Task 3: All-or-restore knowledge apply

**Files:**
- Create: `crates/kb-app/src/knowledge_apply.rs`
- Modify: `crates/kb-app/src/source_apply.rs`
- Modify: `crates/kb-app/src/operation.rs`
- Modify: `crates/kb-app/src/app.rs`
- Test: `crates/kb-app/tests/knowledge_apply.rs`
- Test: focused internal crash-boundary tests in `crates/kb-app/src/knowledge_apply.rs`

**Interfaces:**
- Consumes: `KnowledgePlan`, existing lock/storage/operation helpers.
- Produces: `apply_knowledge(user_paths, operation_id, overrides) -> Result<KnowledgePlanResult, KbError>`.
- Extends: pending-state guard to reject both source and knowledge pending markers.

- [x] Write failing tests for stale targets, stale sources, expired/tampered plans, duplicate completed apply, cache invalidation warning, and unrelated-file preservation.
- [x] Add child-process crash tests at pending marker, every target write, result receipt, and recovery conflict.
- [x] Run the focused app tests and confirm failure from missing apply behavior.
- [x] Implement preflight, durable progress, replacement, verification, reverse restoration, receipt, and cleanup.
- [x] Route operation inspection and shared `kb apply` dispatch by explicit operation kind.
- [x] Run the focused app tests until they pass.
- [x] Commit apply and recovery tests.

### Task 4: CLI request adapter and real user journey

**Files:**
- Modify: `crates/kb-cli/src/args.rs`
- Modify: `crates/kb-app/src/app.rs`
- Modify: `crates/kb-app/src/capabilities.rs`
- Create: `crates/kb-cli/tests/knowledge_journey.rs`
- Modify: `crates/kb-cli/tests/help.rs`
- Modify: `crates/kb-cli/tests/json_contract.rs`

**Interfaces:**
- Produces: `kb plan create <REQUEST_JSON> [--vault ...] [--json]`.
- Reuses: `kb operation show` and `kb apply`.

- [x] Write a failing real-binary journey covering init, request file, plan creation without Vault mutation, operation show, apply, query, lint, reload, duplicate apply, index/log human-region preservation, and machine-readable failures.
- [x] Run `cargo test -p kb-cli --test knowledge_journey` and confirm the command is missing.
- [x] Implement UTF-8 request loading, JSON validation, command mapping, and capability reporting.
- [x] Run the journey plus adjacent help/JSON tests until they pass.
- [x] Commit the CLI workflow and tests.

### Task 5: Reference docs, ADR lifecycle, and focused verification

**Files:**
- Create: `docs/reference/knowledge-plans.md`
- Modify: `docs/reference/commands.md`
- Modify: `docs/architecture/overview.md`
- Modify: `ROADMAP.md`
- Move: `docs/decisions/proposed/architecture/0007-reviewed-plans-and-all-or-restore-writes.md` to `docs/decisions/accepted/architecture/`

**Interfaces:**
- Documents request/plan/result formats, failure semantics, recovery, and current limitations in one reference home.

- [x] Publish the reference and link summaries without duplicating field definitions.
- [x] Move ADR 0007 to accepted/implemented present-tense reality with alternatives and consequences.
- [x] Mark Stage 3B implemented with exact local evidence and remaining platform gaps.
- [x] Run formatting, related crate Clippy, core/app/CLI focused tests, and diff checks; do not run the full suite.
- [x] Inspect the complete diff and commit the documentation.
