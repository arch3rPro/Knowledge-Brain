# Search, Diagnostics, and Backup Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide explicit literal search, distinguish Vault diagnostics from local runtime state, and return a truthful backup-verification error code.

**Architecture:** `kb-core` owns the new serializable query intent and backup error category. `kb-app` chooses direct literal matching before any BM25F path and emits clearer doctor facts; CLI, MCP and HTTP only decode and expose the shared contracts.

**Tech Stack:** Rust 2024, serde/serde_json, clap, axum, MCP JSON-RPC, assert_cmd, tempfile, zip.

**Spec:** [使用体验与 Agent Skill 套件设计](../specs/2026-09-08-usable-agent-skills-design.md)

## Global Constraints

- `match_mode: relevant` is the backward-compatible default; `exact` is case-sensitive Unicode literal matching.
- Exact matching does not tokenize, rank with BM25F, read/write the BM25F cache, or fail because a strict BM25F backend is unavailable.
- CLI uses `kb query --exact`; MCP and HTTP use the same `match_mode` field.
- Doctor reports `vault_structure` separately from `machine_runtime_directories`; missing lazy runtime directories do not make a healthy explicit Vault warning-only broken.
- `backup_verification_failed` covers malformed, unsafe or byte-inconsistent archives; `restore_failed` covers target validation, publication and write failures after verification.
- Verification failures include `details.legacy_code: "restore_failed"` during the compatibility transition.
- Preserve the Rust 1.85 MSRV and run focused tests only.

---

### Task 1: Add a serializable match mode and exact-search implementation

**Files:**
- Modify: `crates/kb-core/src/search.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Modify: `crates/kb-core/tests/search.rs`
- Modify: `crates/kb-app/src/search.rs`
- Modify: `crates/kb-app/tests/bm25.rs`

**Interfaces:**
- Consumes: `SearchRequest`, `SearchResponse`, `SearchMode`, extracted document blocks and the existing direct/BM25F paths.
- Produces: `SearchMatchMode::{Relevant, Exact}`, `SearchRequest.match_mode`, `SearchResponse.match_mode`, and exact block matching in `kb_app::query`.

- [ ] **Step 1: Write core serialization tests for the default and exact forms**

```rust
assert_eq!(
    serde_json::from_str::<SearchRequest>(
        r#"{"query":"needle","scope":"wiki","limit":10,"strict_backend":false}"#,
    ).unwrap().match_mode,
    SearchMatchMode::Relevant,
);
assert_eq!(
    serde_json::to_value(SearchMatchMode::Exact).unwrap(),
    serde_json::json!("exact"),
);
```

- [ ] **Step 2: Run the core test and verify the new field/type is absent**

Run: `cargo test -p kb-core --test search`

Expected: compilation failure for `SearchMatchMode` and `SearchRequest.match_mode`.

- [ ] **Step 3: Add the type and route exact matching before BM25F**

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMatchMode {
    #[default]
    Relevant,
    Exact,
}

pub struct SearchRequest {
    pub query: String,
    pub scope: SearchScope,
    pub limit: usize,
    pub strict_backend: bool,
    #[serde(default)]
    pub match_mode: SearchMatchMode,
}
```

In `kb-app/src/search.rs`, branch on `SearchMatchMode::Exact` before reading the configured `SearchMode`. Match `block.text.matches(r.query.trim())`, preserve the existing block location/snippet construction, use `SearchBackend::Direct` with no score or explanation, and return no cache warning. Relevant mode retains the current lowercase phrase/term and BM25F behavior.

- [ ] **Step 4: Add app behavior tests that would fail if BM25F is consulted**

```rust
#[test]
fn exact_match_is_case_sensitive_literal_and_ignores_a_stale_bm25_cache() {
    let result = query(&vault, &exact_request("kb-incremental-rebuild-probe"), &bm25_config).unwrap();
    assert_eq!(result.match_mode, SearchMatchMode::Exact);
    assert_eq!(result.groups[0].results.len(), 1);
    assert!(query(&vault, &exact_request("KB-INCREMENTAL-REBUILD-PROBE"), &bm25_config).unwrap().groups[0].results.is_empty());
    assert!(query(&vault, &exact_request_with_strict_backend("kb-incremental-rebuild-probe"), &bm25_config).is_ok());
}
```

Corrupt or remove `.kb/cache/bm25.json` before the exact calls. Assert no `index_stale` warning/error and no result from a document containing only `probe`.

- [ ] **Step 5: Run focused core and app search tests and commit**

Run: `cargo test -p kb-core --test search -p kb-app --test bm25`

Expected: PASS; relevant fallback behavior remains covered and exact mode bypasses the BM25F cache.

```bash
git add crates/kb-core/src/search.rs crates/kb-core/src/lib.rs crates/kb-core/tests/search.rs crates/kb-app/src/search.rs crates/kb-app/tests/bm25.rs
git commit -m "feat: add exact literal search mode"
```

### Task 2: Wire exact search through CLI, MCP, HTTP, and their real entry tests

**Files:**
- Modify: `crates/kb-cli/src/args.rs`
- Modify: `crates/kb-cli/tests/bm25_journey.rs`
- Modify: `crates/kb-mcp/src/server.rs`
- Modify: `crates/kb-mcp/tests/tools.rs`
- Modify: `crates/kb-server/tests/http.rs`
- Modify: `docs/reference/commands.md`
- Modify: `docs/reference/search.md`
- Modify: `docs/reference/mcp.md`
- Modify: `docs/reference/http.md`

**Interfaces:**
- Consumes: `SearchMatchMode` from Task 1.
- Produces: CLI `--exact`, MCP input-schema/`QueryArguments.match_mode`, HTTP-deserialized `SearchRequest`, and responses that always report `match_mode`.

- [ ] **Step 1: Add entry-path assertions before wiring arguments**

```rust
let cli = run(base, &["query", "needle", "--exact", "--vault", path, "--json"]);
assert_eq!(cli["data"]["match_mode"], "exact");

let mcp = call(&mut server, 3, "kb_query", &json!({"query":"needle","match_mode":"exact"}));
assert_eq!(mcp["result"]["structuredContent"]["data"]["match_mode"], "exact");

let (_, http) = request(&server, "POST", "/query", None, r#"{"query":"needle","match_mode":"exact"}"#).await;
assert_eq!(http["data"]["match_mode"], "exact");
```

- [ ] **Step 2: Run the three tests and verify each new input is rejected or absent**

Run: `cargo test -p kb-cli --test bm25_journey -p kb-mcp --test tools -p kb-server --test http`

Expected: FAIL because `--exact` is unknown and MCP rejects the extra property.

- [ ] **Step 3: Decode the same field at each adapter boundary**

Add `#[arg(long)] exact: bool` to `Commands::Query`; set `match_mode` to `Exact` only when it is true. Add `"match_mode":{"type":"string","enum":["relevant","exact"],"default":"relevant"}` to MCP `kb_query` schema, add `#[serde(default)] match_mode: SearchMatchMode` to `QueryArguments`, and copy it into `SearchRequest`. HTTP already deserializes `SearchRequest`, so it receives the field after Task 1 without route-specific branching.

- [ ] **Step 4: Document intent rather than backend internals**

State that `--exact` is case-sensitive literal verification and that default search is relevant discovery. Keep `search.mode` documented as only the relevant-search backend selector; link the cross-adapter contract to the approved spec.

- [ ] **Step 5: Run targeted adapter tests and commit**

Run: `cargo test -p kb-cli --test bm25_journey -p kb-mcp --test tools -p kb-server --test http`

Expected: PASS; all three adapters accept/return the same values and exact works when BM25F cache is absent.

```bash
git add crates/kb-cli/src/args.rs crates/kb-cli/tests/bm25_journey.rs crates/kb-mcp/src/server.rs crates/kb-mcp/tests/tools.rs crates/kb-server/tests/http.rs docs/reference/commands.md docs/reference/search.md docs/reference/mcp.md docs/reference/http.md
git commit -m "feat: expose exact search across adapters"
```

### Task 3: Separate Vault and machine-runtime doctor checks

**Files:**
- Modify: `crates/kb-app/src/doctor.rs`
- Create: `crates/kb-app/tests/doctor.rs`
- Modify: `crates/kb-cli/tests/doctor.rs`
- Modify: `docs/reference/commands.md`

**Interfaces:**
- Consumes: `UserPaths`, selected Vault root and existing `DoctorCheck`/`DoctorReport` serialization.
- Produces: `vault_structure` and `machine_runtime_directories` checks with per-directory purpose and on-demand behavior.

- [ ] **Step 1: Write the app-level doctor fact test**

```rust
let report = doctor(&vault, &unused_user_paths, &ConfigOverrides::default()).unwrap();
let runtime = report.checks.iter().find(|check| check.id == "machine_runtime_directories").unwrap();
assert_eq!(runtime.status, CheckStatus::Ok);
assert!(runtime.message.contains("created on demand"));
assert!(report.checks.iter().any(|check| check.id == "vault_structure"));
```

- [ ] **Step 2: Run the focused test and verify the old combined check fails the assertions**

Run: `cargo test -p kb-app --test doctor`

Expected: FAIL because the report has `standard_directories` and no separate Vault structure check.

- [ ] **Step 3: Replace the combined check with precise facts**

Keep `vault_readability`, schema, admission and writability checks, but add a `vault_structure` check that reports required Vault paths and their actual missing/invalid subject. Replace `standard_directories` with `machine_runtime_directories`; list config/state/cache paths, say that missing paths are created on demand, and return `Ok` for ordinary absence. Return warning/error only when inspection itself fails or an existing path has an unusable type/permission.

- [ ] **Step 4: Verify application and real CLI output**

Run: `cargo test -p kb-app --test doctor -p kb-cli --test doctor`

Expected: PASS; a fresh explicit Vault has an actionable Vault result and does not present missing local cache directories as a generic warning.

- [ ] **Step 5: Update the reference and commit**

Document the two IDs, their inspection subjects, and the fact that machine runtime directories are not Vault contents.

```bash
git add crates/kb-app/src/doctor.rs crates/kb-app/tests/doctor.rs crates/kb-cli/tests/doctor.rs docs/reference/commands.md
git commit -m "feat: clarify vault and runtime diagnostics"
```

### Task 4: Classify archive verification failures separately from restore failures

**Files:**
- Modify: `crates/kb-core/src/error.rs`
- Modify: `crates/kb-app/src/backup.rs`
- Modify: `crates/kb-app/tests/backup.rs`
- Modify: `crates/kb-cli/tests/backup_journey.rs`
- Modify: `docs/reference/backup.md`
- Modify: `docs/reference/http.md`
- Modify: `docs/reference/mcp.md`

**Interfaces:**
- Consumes: `KbError::with_details`, all `invalid_archive`/`archive_io` callers, `validate_restore_target`, and existing JSON error envelopes.
- Produces: `ErrorCode::BackupVerificationFailed` serialized as `backup_verification_failed`; `invalid_archive` errors with `legacy_code` details; `RestoreFailed` only after archive validation.

- [ ] **Step 1: Change test expectations to the two intended categories**

```rust
assert_eq!(verify_backup(&wrong_hash).unwrap_err().code, ErrorCode::BackupVerificationFailed);
assert_eq!(verify_backup(&malicious).unwrap_err().code, ErrorCode::BackupVerificationFailed);
assert_eq!(restore_backup(&valid_archive, &existing_nonempty_target).unwrap_err().code, ErrorCode::RestoreFailed);
```

Also assert `error.details.unwrap()["legacy_code"] == "restore_failed"` for an invalid archive.

- [ ] **Step 2: Run the focused backup test and verify the changed expectations fail**

Run: `cargo test -p kb-app --test backup`

Expected: FAIL because archive validation still returns `RestoreFailed`.

- [ ] **Step 3: Add the code and repair target-path classification**

Add `BackupVerificationFailed` to `ErrorCode`. Make `invalid_archive` construct that code and attach both `reason` and `legacy_code`. Replace `invalid_archive("restore target has no parent")` with a `RestoreFailed` construction because the target is not archive evidence. Keep `archive_io` mapped to `invalid_archive`; leave stage extraction/publish/target errors on their existing restore/write categories unless they are caused by invalid archive bytes.

- [ ] **Step 4: Verify app and real CLI envelopes**

Run: `cargo test -p kb-app --test backup -p kb-cli --test backup_journey`

Expected: PASS; corrupt archives fail `kb backup verify` with `backup_verification_failed`, while a valid archive rejected for its target retains the restore category.

- [ ] **Step 5: Document the stable code and commit**

Describe the archive-versus-target distinction, `legacy_code` transition detail, and shared CLI/MCP/HTTP meaning without claiming old clients automatically understand the new enum value.

```bash
git add crates/kb-core/src/error.rs crates/kb-app/src/backup.rs crates/kb-app/tests/backup.rs crates/kb-cli/tests/backup_journey.rs docs/reference/backup.md docs/reference/http.md docs/reference/mcp.md
git commit -m "fix: classify backup verification failures"
```
