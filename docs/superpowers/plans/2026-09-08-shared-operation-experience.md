# Shared Operation Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every operation-producing adapter the same additive summary while making CLI text output concise and Hash-aware.

**Architecture:** `kb-app` derives typed `OperationSummary` values from the persisted operation variants and attaches them to existing JSON objects without changing their existing fields. `kb-cli` consumes that shared data in a text renderer; JSON output remains the canonical machine protocol and is never shortened.

**Tech Stack:** Rust 2024, serde/serde_json, clap, assert_cmd, tempfile.

**Spec:** [使用体验与 Agent Skill 套件设计](../specs/2026-09-08-usable-agent-skills-design.md)

## Global Constraints

- Keep complete SHA-256 values, operation IDs, Vault IDs and paths in JSON, MCP, HTTP and persistent records.
- Default human text displays the first 12 hexadecimal characters of SHA-256; `--full-hashes` displays all 64 characters.
- Add `operation_summary` at the JSON root; do not rename, wrap or delete existing response fields.
- A planned summary sets `requires_confirmation: true`; an applied summary sets `can_apply: false`.
- `kb apply <operation-id>` remains non-interactive.
- Preserve the Rust 1.85 MSRV and do not add a runtime dependency on Node.js.
- Run focused crate and real CLI tests only; do not run the workspace-wide suite for this change.

---

### Task 1: Model operation summaries in `kb-app`

**Files:**
- Create: `crates/kb-app/src/operation_summary.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Modify: `crates/kb-app/src/app.rs`
- Test: `crates/kb-app/tests/operation_summary.rs`

**Interfaces:**
- Consumes: `OperationState` from `crates/kb-app/src/operation.rs`, the adoption/source/knowledge/Skill plan and result structs, and `OperationId`/`Uuid` from `kb-core`.
- Produces: `OperationSummary`, `summary_for_state(&OperationState)`, `summary_for_*_plan(...)`, and `attach_operation_summary(Value, OperationSummary)` for application routes.

- [ ] **Step 1: Write failing summary-contract tests**

```rust
#[test]
fn planned_knowledge_summary_is_actionable_and_uses_vault_relative_paths() {
    let summary = summary_for_knowledge_plan(&plan_with_changes([
        "articles/one.md", "research/two.md",
    ]));
    assert_eq!(summary.operation_state, "planned");
    assert!(summary.requires_confirmation);
    assert!(summary.can_apply);
    assert_eq!(summary.change_count, 2);
    assert_eq!(summary.affected_paths, vec!["Wiki/articles/one.md", "Wiki/research/two.md"]);
}

#[test]
fn applied_summary_keeps_the_full_id_but_cannot_be_applied_again() {
    let summary = summary_for_state(&OperationState::AppliedKnowledge(result));
    assert_eq!(summary.operation_id.to_string().len(), 36);
    assert_eq!(summary.operation_state, "applied");
    assert!(!summary.requires_confirmation);
    assert!(!summary.can_apply);
}
```

- [ ] **Step 2: Run the new test target and verify it fails because the module is absent**

Run: `cargo test -p kb-app --test operation_summary`

Expected: compilation failure naming `operation_summary` or its missing exported functions.

- [ ] **Step 3: Implement typed summaries and additive attachment**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct OperationSummary {
    pub operation_id: OperationId,
    pub operation_kind: String,
    pub operation_state: String,
    pub vault_id: Uuid,
    pub vault_root: PathBuf,
    pub change_count: usize,
    pub affected_paths: Vec<String>,
    pub summary: String,
    pub requires_confirmation: bool,
    pub can_apply: bool,
}

pub fn attach_operation_summary(
    mut response: serde_json::Value,
    summary: OperationSummary,
) -> Result<serde_json::Value, KbError> {
    response.as_object_mut()
        .ok_or_else(|| KbError::invalid_config("operation response", "expected object"))?
        .insert("operation_summary".to_owned(), serde_json::to_value(summary)
            .map_err(|error| KbError::invalid_config("operation summary", error.to_string()))?);
    Ok(response)
}
```

Implement one constructor for each plan/result variant. Use Vault-relative `Wiki/...` or source-relative paths for Vault writes; use host-relative paths such as `.agents/skills/kb-query/SKILL.md` for Skill changes. Set `requires_confirmation` and `can_apply` only for planned states.

- [ ] **Step 4: Attach summaries to every planned operation response and operation inspection**

In `run`, attach a summary after creating an adoption plan, a source review with `operation_id`, a knowledge plan, and a Skill install/uninstall plan. In `run_operation`, first obtain `OperationState`, derive a summary from the same state, preserve the existing `state` plus `plan`/`result` object, and append `operation_summary`.

```rust
let state = inspect_operation(context.user_paths()?, operation_id)?;
let summary = summary_for_state(&state);
let response = match state {
    OperationState::Planned(plan) => json!({"state":"planned", "plan": plan}),
    OperationState::Applied(result) => json!({"state":"applied", "result": result}),
    // retain the existing source, knowledge and Skill branches
};
attach_operation_summary(response, summary)
```

- [ ] **Step 5: Run the focused app tests and commit**

Run: `cargo test -p kb-app --test operation_summary --test knowledge_plan --test skill_plan`

Expected: PASS; existing plan fields remain readable and every asserted plan/show response has one root-level `operation_summary`.

```bash
git add crates/kb-app/src/operation_summary.rs crates/kb-app/src/lib.rs crates/kb-app/src/app.rs crates/kb-app/tests/operation_summary.rs
git commit -m "feat: add shared operation summaries"
```

### Task 2: Render concise human CLI output with optional full Hashes

**Files:**
- Modify: `crates/kb-cli/src/args.rs`
- Modify: `crates/kb-cli/src/main.rs`
- Modify: `crates/kb-cli/src/render.rs`
- Create: `crates/kb-cli/tests/human_output.rs`

**Interfaces:**
- Consumes: root-level `operation_summary`, query `groups`, doctor `checks`, generic JSON values and the CLI-global `full_hashes` flag.
- Produces: `render::success(value, json_output, fail_on_findings, full_hashes)` and `render::human(value, full_hashes) -> Result<String, KbError>`.

- [ ] **Step 1: Write real CLI tests before adding the flag**

```rust
#[test]
fn human_source_verification_uses_short_hashes_and_json_keeps_full_hashes() {
    let text = command(&base).args(["source", "verify", "--vault", vault]).output().unwrap();
    assert!(text.status.success());
    assert!(!String::from_utf8_lossy(&text.stdout).contains("\"sha256\""));
    assert!(String::from_utf8_lossy(&text.stdout).contains("a1b2c3d4e5f6"));

    let json = run_json(&base, &["source", "verify", "--vault", vault, "--json"]);
    assert_eq!(json["data"]["versions"][0]["sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn full_hashes_expands_only_human_output() {
    let output = command(&base).args(["source", "verify", "--full-hashes", "--vault", vault]).output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains(&full_sha256));
}
```

- [ ] **Step 2: Run the test and verify `--full-hashes` is rejected**

Run: `cargo test -p kb-cli --test human_output`

Expected: FAIL because clap reports an unexpected argument or the expected renderer is not present.

- [ ] **Step 3: Add a global flag and a deterministic text renderer**

Add `#[arg(long, global = true)] full_hashes: bool` to `Cli`, carry it through every `ParsedCommand::App` execution path, and pass it to `render::success`.

```rust
fn display_hash(value: &str, full_hashes: bool) -> &str {
    if !full_hashes && value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        &value[..12]
    } else {
        value
    }
}
```

Keep `diff` output byte-for-byte. Render operation summaries as labeled lines, query groups as path/title/snippet lines, doctor checks as `status id: message`, and other objects with an indentation-based key/value renderer that has no JSON braces or quoted keys. Only shorten values that are exactly 64 hexadecimal characters; never shorten UUIDs, paths, operation IDs or JSON output.

- [ ] **Step 4: Exercise the real command paths**

Run: `cargo test -p kb-cli --test human_output --test json_contract --test phase2_journey`

Expected: PASS; human output is non-JSON and concise, while `--json` still emits the existing `kb_protocol::Envelope` with full canonical identities.

- [ ] **Step 5: Commit the CLI surface**

```bash
git add crates/kb-cli/src/args.rs crates/kb-cli/src/main.rs crates/kb-cli/src/render.rs crates/kb-cli/tests/human_output.rs
git commit -m "feat: render concise human command output"
```

### Task 3: Verify adapter compatibility and document the operation contract

**Files:**
- Modify: `crates/kb-cli/tests/json_contract.rs`
- Modify: `crates/kb-mcp/tests/tools.rs`
- Modify: `crates/kb-server/tests/http.rs`
- Modify: `docs/reference/commands.md`
- Modify: `docs/reference/agent-skill.md`
- Modify: `docs/reference/mcp.md`
- Modify: `docs/reference/http.md`

**Interfaces:**
- Consumes: the summary-augmented `kb-app` responses through CLI, MCP `kb_operation_show`, and HTTP `GET /operations/{operation_id}`.
- Produces: compatibility tests proving old root fields coexist with `operation_summary`, and public reference wording that assigns one user-confirmation point to the outer Agent/UI.

- [ ] **Step 1: Write adapter assertions that preserve the old shape**

```rust
assert_eq!(shown["data"]["state"], "planned");
assert!(shown["data"]["plan"].is_object());
assert_eq!(shown["data"]["operation_summary"]["requires_confirmation"], true);
assert_eq!(shown["data"]["operation_summary"]["operation_id"], operation_id);
```

Add this assertion style to a CLI `operation show`, MCP `kb_operation_show`, and HTTP `GET /operations/{id}` test. Add an applied-operation assertion that `can_apply` is false.

- [ ] **Step 2: Run targeted adapter tests through the real adapter boundaries**

Run: `cargo test -p kb-cli --test json_contract -p kb-mcp --test tools -p kb-server --test http`

Expected: PASS because Task 1 is the prerequisite; a failure proves an adapter has bypassed the shared `kb-app` response instead of preserving the additive field.

- [ ] **Step 3: Update the public references without duplicating the design**

Document `--full-hashes`, the default 12-character display, additive `operation_summary`, and the rule that direct CLI apply is explicit while Agent/UI calls obtain one final confirmation. Link the detailed rationale to the approved spec rather than restating its alternatives.

- [ ] **Step 4: Run documentation and adapter contract tests**

Run: `cargo test -p kb-cli --test docs_contract --test json_contract -p kb-mcp --test tools -p kb-server --test http`

Expected: PASS; MCP and HTTP expose complete IDs and the same summary facts as CLI JSON.

- [ ] **Step 5: Commit the compatibility and documentation work**

```bash
git add crates/kb-cli/tests/json_contract.rs crates/kb-mcp/tests/tools.rs crates/kb-server/tests/http.rs docs/reference/commands.md docs/reference/agent-skill.md docs/reference/mcp.md docs/reference/http.md
git commit -m "docs: describe shared operation confirmation"
```
