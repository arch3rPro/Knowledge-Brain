# One-Confirmation Save Entries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Let Agent and UI users approve a prepared source or knowledge save once without exposing operation planning, while retaining durable exact-content checks.

**Architecture:** kb-app keeps the current operation model but introduces a SaveMode coordinator. Prepare persists an existing operation and returns a machine confirmation token plus a user-safe change summary; confirm applies that exact operation; direct execution prepares and applies in one request. CLI, MCP and HTTP only decode their transport inputs and enforce their established write policies.

**Tech Stack:** Rust stable, Clap, Serde JSON, Axum, MCP JSON-RPC, existing kb-app operations.

**Spec:** .superpowers/specs/2026-09-08-composite-save-entries-design.md

## Global Constraints

- Keep schema_version v1.0, existing operation records, capture_sources and save_knowledge kinds unchanged.
- Do not add dependencies, a terminal wizard, or a user requirement to enter an operation ID.
- Default coordinated saves prepare only; confirmation tokens apply the exact prepared operation; --yes and apply: true prepare and apply in one request.
- Do not create a new cross-operation atomicity guarantee.
- Keep stale-plan, recovery, selected-Vault, lock, HTTP token and MCP/HTTP --allow-write protections.
- Run only focused test targets locally, then formatter and diff checks.

---

## File Structure

- crates/kb-app/src/review.rs: split source discovery and plan construction from source plan persistence.
- crates/kb-app/src/knowledge_plan.rs: split knowledge plan construction from persistence.
- crates/kb-app/src/app.rs: SaveMode, source/knowledge coordinator, token validation and change summaries.
- crates/kb-app/tests/composite_save.rs: real application preparation, confirmation, direct execution and no-change cases.
- crates/kb-cli/src/args.rs and crates/kb-cli/tests/composite_save_journey.rs: CLI save, --yes and --confirm routes.
- crates/kb-mcp/src/server.rs and crates/kb-mcp/tests/tools.rs: MCP prepare, confirmation and write-policy behavior.
- crates/kb-server/src/lib.rs and crates/kb-server/tests/http.rs: HTTP request bodies, routes and policy behavior.
- docs/reference/commands.md, docs/reference/mcp.md, docs/reference/http.md, skills/kb-ingest/SKILL.md and skills/kb-save/SKILL.md: public workflow guidance.
- crates/kb-cli/tests/docs_contract.rs: public command/reference boundary coverage.

### Task 1: Separate preparation from persistence

**Files:**
- Modify: crates/kb-app/src/review.rs
- Modify: crates/kb-app/src/knowledge_plan.rs
- Modify: crates/kb-app/src/lib.rs
- Test: crates/kb-app/tests/composite_save.rs

**Interfaces:**
- Produces: PreparedSourceCapture containing a ReviewReport and an optional SourceCapturePlan.
- Produces: build_knowledge_plan(root, config, request, now) returning an unpersisted KnowledgePlan.
- Preserves: review_sources and create_knowledge_plan continue to persist exactly as before for advanced primitive callers.

- [ ] **Step 1: Write the failing preparation tests**

Create crates/kb-app/tests/composite_save.rs. Initialize a real Vault, add an enabled Notes directory and one Markdown file. Call the new preparation functions through the application entry added in Task 2 and assert these externally visible facts:

~~~
default source save:
  phase == awaiting_confirmation
  confirmation_token is a string
  source verify is blocked because the exact prepared operation is pending

confirm with that token:
  phase == applied
  source verify returns one pass check

source save after capture:
  phase == unchanged
  confirmation_token and result are null
~~~

Add a knowledge fixture whose managed Article is valid. Assert default knowledge save leaves the Article path absent and returns awaiting_confirmation; confirmation writes that Article.

- [ ] **Step 2: Run the test target to verify it fails**

Run: cargo test -p kb-app --test composite_save

Expected: compilation failure because AppRequest source and knowledge save variants do not exist.

- [ ] **Step 3: Refactor source and knowledge preparation**

In review.rs, extract the discovery, record-write and SourceCapturePlan construction into:

~~~
pub(crate) struct PreparedSourceCapture {
    pub report: ReviewReport,
    pub plan: Option<SourceCapturePlan>,
}

pub(crate) fn prepare_source_capture(
    root: &Path,
    config: &EffectiveConfig,
) -> Result<PreparedSourceCapture, KbError>;

pub(crate) fn persist_source_capture_plan(
    root: &Path,
    paths: &UserPaths,
    config: &EffectiveConfig,
    plan: &SourceCapturePlan,
) -> Result<(), KbError>;
~~~

The preparation path assigns an OperationId only when there are source changes, but does not create the operation directory. review_sources calls prepare_source_capture, persists the optional plan, and returns the same ReviewReport operation_id semantics it had before.

In knowledge_plan.rs, move the existing validation and KnowledgePlan construction into:

~~~
pub(crate) fn build_knowledge_plan(
    root: &Path,
    config: &EffectiveConfig,
    request: KnowledgePlanRequest,
    now: OffsetDateTime,
) -> Result<KnowledgePlan, KbError>;

pub(crate) fn persist_knowledge_plan(
    user_paths: &UserPaths,
    plan: &KnowledgePlan,
) -> Result<(), KbError>;
~~~

create_knowledge_plan calls build_knowledge_plan followed by persist_knowledge_plan. Keep its public signature and behavior unchanged.

- [ ] **Step 4: Run the existing primitive regression tests**

Run:

~~~
cargo test -p kb-app --test knowledge_plan
cargo test -p kb-app --test operation_summary
~~~

Expected: PASS. Existing review and knowledge-plan users still receive persisted planned operations.

- [ ] **Step 5: Commit the preparation seam**

~~~
git add crates/kb-app/src/review.rs crates/kb-app/src/knowledge_plan.rs crates/kb-app/src/lib.rs
git commit -m "refactor: separate save preparation from persistence"
~~~

### Task 2: Add the application confirmation coordinator

**Files:**
- Modify: crates/kb-app/src/app.rs
- Modify: crates/kb-app/src/lib.rs
- Modify: crates/kb-app/tests/composite_save.rs

**Interfaces:**
- Produces: public SaveMode enum with Prepare, Confirm(OperationId), and ApplyImmediately variants.
- Produces: AppRequest::SourceSave and AppRequest::KnowledgeSave carrying SaveMode.
- Produces: response fields phase, change_summary, confirmation_token, preview and result.

- [ ] **Step 1: Extend the failing application test**

Add a selected-Vault isolation case. Prepare a knowledge save in the second Vault, then attempt SourceSave or KnowledgeSave confirmation with its token while selecting the first Vault. Assert error.code is auth_denied and the second Vault Article is absent.

Add direct execution coverage:

~~~
let applied = run(
    AppRequest::KnowledgeSave {
        vault: Some(vault_text),
        request: knowledge_request(),
        mode: SaveMode::ApplyImmediately,
    },
    &context,
).unwrap();

assert_eq!(applied["phase"], "applied");
assert!(applied["confirmation_token"].is_null());
assert!(article.is_file());
~~~

- [ ] **Step 2: Run the test to verify it still fails**

Run: cargo test -p kb-app --test composite_save

Expected: compilation failure because SaveMode and the two AppRequest variants are absent.

- [ ] **Step 3: Implement SaveMode and the response contract**

Add:

~~~
pub enum SaveMode {
    Prepare,
    Confirm(OperationId),
    ApplyImmediately,
}
~~~

Add AppRequest variants:

~~~
SourceSave { vault: Option<String>, mode: SaveMode },
KnowledgeSave {
    vault: Option<String>,
    request: Option<KnowledgePlanRequest>,
    mode: SaveMode,
},
~~~

For Prepare, select one Vault, enforce mutation compatibility and no pending recovery, invoke the Task 1 builder, persist only when there is a plan, and return:

~~~
{
  "phase": "awaiting_confirmation",
  "change_summary": {
    "operation_kind": "...",
    "change_count": 1,
    "affected_paths": ["..."],
    "summary": "..."
  },
  "confirmation_token": "<complete operation id>",
  "preview": { "...": "source changes or knowledge plan" },
  "result": null
}
~~~

For an empty source preparation, return phase unchanged with null token and null result. For Confirm, require a token, select the fixed Vault, verify that token owns that Vault and matches the requested source or knowledge operation kind, then call the existing selected-Vault apply implementation. For ApplyImmediately, execute the Prepare branch and immediately confirm its returned token; never reprepare.

Derive change_summary from existing operation summary helpers but omit operation_id, vault_id and vault_root. Preserve full identifiers inside machine preview and existing primitive responses.

If apply returns an error after a plan exists, preserve the stable error code and merge confirmation_token into error.details. The detail merge must retain existing object properties; a non-object original detail belongs under cause.

- [ ] **Step 4: Run the application tests**

Run: cargo test -p kb-app --test composite_save

Expected: PASS. The test must prove preview does not write content, confirmation writes the exact pending operation, direct execution writes in one call, no-change has no token, and cross-Vault confirmation is denied.

- [ ] **Step 5: Commit the application workflow**

~~~
git add crates/kb-app/src/app.rs crates/kb-app/src/lib.rs crates/kb-app/tests/composite_save.rs
git commit -m "feat: add one-confirmation save workflow"
~~~

### Task 3: Add CLI save and confirmation commands

**Files:**
- Modify: crates/kb-cli/src/args.rs
- Create: crates/kb-cli/tests/composite_save_journey.rs

**Interfaces:**
- Consumes: SaveMode and the new AppRequest variants.
- Produces: kb source save [--yes | --confirm TOKEN] and kb knowledge save REQUEST [--yes | --confirm TOKEN].

- [ ] **Step 1: Write the failing real CLI journey**

Initialize a Vault and configure Notes. Run:

~~~
kb source save --vault <vault> --json
~~~

Assert data.phase is awaiting_confirmation, preserve data.confirmation_token in the test, and assert source verify fails due to the pending operation. Then run:

~~~
kb source save --confirm <token> --vault <vault> --json
~~~

Assert data.phase is applied and source verify succeeds.

Create a knowledge request file. Assert kb knowledge save REQUEST --json leaves the requested Wiki file absent. Extract its token, then run kb knowledge save --confirm TOKEN --vault <vault> --json and assert the file exists. Add kb knowledge save REQUEST --yes --json and assert one-call applied behavior in a fresh Vault.

- [ ] **Step 2: Run the CLI test to verify it fails**

Run: cargo test -p kb-cli --test composite_save_journey

Expected: parse failure because source save, knowledge and --confirm are not registered.

- [ ] **Step 3: Parse mutually exclusive execution modes**

Extend SourceCommands::Save with a Clap argument group containing --yes and --confirm <OperationId>. Map neither to SaveMode::Prepare, --yes to ApplyImmediately and --confirm to Confirm(token).

Add Commands::Knowledge with KnowledgeCommands::Save. Its request argument is optional only for --confirm. Validate these combinations in the existing ParsedCommand result path:

- a request is required for preparation and --yes;
- --confirm requires no request;
- request plus --confirm returns invalid_config;
- --yes plus --confirm is rejected by Clap before application dispatch.

Keep KnowledgeRequestArg invalid-file handling and JSON envelope behavior unchanged.

- [ ] **Step 4: Run the CLI journey**

Run: cargo test -p kb-cli --test composite_save_journey

Expected: PASS. Also assert human --help shows --yes and --confirm but no help text tells a user to manage operation IDs.

- [ ] **Step 5: Commit CLI support**

~~~
git add crates/kb-cli/src/args.rs crates/kb-cli/tests/composite_save_journey.rs
git commit -m "feat: add confirmation save CLI commands"
~~~

### Task 4: Add MCP confirmation token support

**Files:**
- Modify: crates/kb-mcp/src/server.rs
- Modify: crates/kb-mcp/tests/tools.rs

**Interfaces:**
- Consumes: SaveMode, fixed vault_selector and allow_write.
- Produces: kb_source_save and kb_knowledge_save with prepare, confirm and direct modes.

- [ ] **Step 1: Write failing MCP behavior tests**

On a default MCP server, call kb_source_save with an empty object after creating an admitted source. Assert awaiting_confirmation and retain confirmation_token. Call kb_source_save with confirmation_token and assert a tool result error whose stable code is auth_denied; assert the source was not captured.

On a write-enabled server, call kb_source_save with that token and assert phase applied. Add a write-enabled kb_knowledge_save direct call with request and apply: true; assert the Article exists. Verify a token created for another Vault returns auth_denied.

- [ ] **Step 2: Run MCP tests to verify they fail**

Run: cargo test -p kb-mcp --test tools

Expected: missing tool names or unknown tool dispatch errors.

- [ ] **Step 3: Register and validate MCP modes**

Register both tools on all MCP servers. Source accepts optional apply boolean and confirmation_token string. Knowledge accepts optional request, optional apply and optional confirmation_token. Validate exactly one of these modes:

- no apply and no token: Prepare;
- apply true and no token: ApplyImmediately;
- token and apply absent or false: Confirm;
- token with apply true: invalid JSON-RPC parameters;
- missing knowledge request outside Confirm: invalid JSON-RPC parameters.

For Confirm and ApplyImmediately, return a stable auth_denied tool result before calling kb-app when allow_write is false. Keep malformed arguments as JSON-RPC -32602 errors and preserve kb_apply_operation.

- [ ] **Step 4: Run MCP tests**

Run: cargo test -p kb-mcp --test tools

Expected: PASS for preparation, denied writes, permitted confirmation/direct execution and fixed-Vault isolation.

- [ ] **Step 5: Commit MCP support**

~~~
git add crates/kb-mcp/src/server.rs crates/kb-mcp/tests/tools.rs
git commit -m "feat: confirm prepared saves over MCP"
~~~

### Task 5: Add HTTP confirmation token support

**Files:**
- Modify: crates/kb-server/src/lib.rs
- Modify: crates/kb-server/tests/http.rs

**Interfaces:**
- Consumes: SaveMode, ServerPolicy and fixed selected Vault.
- Produces: POST /source/save and POST /knowledge/save with prepare, confirm and direct modes.

- [ ] **Step 1: Write failing HTTP behavior tests**

Start a token-protected read-only server with an admitted source. POST an empty JSON object to /source/save and assert 200 with awaiting_confirmation. Save the token. POST that token to the same route and assert 403 auth_denied with no source record.

Start a write-enabled server and POST the prepared token; assert applied and a passing source verify. In a fresh Vault, POST a valid knowledge request with apply true and assert applied plus the Article file. Include invalid token-plus-apply and missing knowledge request cases with 400 invalid_config.

- [ ] **Step 2: Run HTTP tests to verify they fail**

Run: cargo test -p kb-server --test http

Expected: 404 for the unregistered routes.

- [ ] **Step 3: Decode, authorize and dispatch HTTP modes**

Register both routes. Deserialize a source body with default apply false and optional confirmation_token. Deserialize a knowledge body with optional request, default apply false and optional confirmation_token. Convert bodies to SaveMode using one shared validation helper.

Authenticate every request first. For confirmation or direct execution, require state.policy.allow_write before calling kb-app. A read-only request returns 403 auth_denied and does not create a new plan. Dispatch with vault: Some(selected(&state)). Keep review, plans and operations apply routes unchanged.

- [ ] **Step 4: Run HTTP tests**

Run: cargo test -p kb-server --test http

Expected: PASS for authenticated prepare, denied writes, enabled confirmation/direct execution, malformed mode validation and fixed-Vault isolation.

- [ ] **Step 5: Commit HTTP support**

~~~
git add crates/kb-server/src/lib.rs crates/kb-server/tests/http.rs
git commit -m "feat: confirm prepared saves over HTTP"
~~~

### Task 6: Publish the one-confirmation workflow

**Files:**
- Modify: docs/reference/commands.md
- Modify: docs/reference/mcp.md
- Modify: docs/reference/http.md
- Modify: skills/kb-ingest/SKILL.md
- Modify: skills/kb-save/SKILL.md
- Modify: crates/kb-cli/tests/docs_contract.rs

**Interfaces:**
- Consumes: implemented save/confirmation contracts.
- Produces: user-facing instructions that show only one confirmation and describe tokens as Agent/UI fields.

- [ ] **Step 1: Write failing contract and behavior checks**

Extend docs_contract to run kb --help and assert knowledge is a top-level command. Require the command reference to name kb source save, kb knowledge save, --yes and --confirm. Require MCP and HTTP references to name kb_source_save, kb_knowledge_save, POST /source/save, POST /knowledge/save and confirmation_token.

- [ ] **Step 2: Run the documentation test to verify it fails**

Run: cargo test -p kb-cli --test docs_contract

Expected: missing command/reference assertions until durable documentation is updated.

- [ ] **Step 3: Update CLI, protocol and Skill guidance**

Document that a normal Agent/UI flow prepares, shows one change summary, obtains one user confirmation and submits the machine token. State that direct --yes or apply: true is for callers that already have authorization. Explain that advanced review/plan/apply remains available but is not the ordinary user workflow.

Update kb-ingest and kb-save so they never expose plan creation or operation IDs to a user. They retain tokens internally, present only change summaries, and make one confirmation request. Do not claim source capture authorizes an Agent’s earlier direct write into an admitted directory.

- [ ] **Step 4: Run documentation and CLI tests**

Run:

~~~
cargo test -p kb-cli --test docs_contract
cargo test -p kb-cli --test composite_save_journey
~~~

Expected: PASS. The help output and documentation use the same prepare, confirmation and direct-execution vocabulary.

- [ ] **Step 5: Commit the public contract**

~~~
git add docs/reference/commands.md docs/reference/mcp.md docs/reference/http.md skills/kb-ingest/SKILL.md skills/kb-save/SKILL.md crates/kb-cli/tests/docs_contract.rs
git commit -m "docs: describe one-confirmation save workflow"
~~~

### Task 7: Verify the visible workflow and publish

**Files:**
- Modify only when a focused check identifies a defect.

- [ ] **Step 1: Run focused automated verification**

Run:

~~~
cargo test -p kb-app --test composite_save
cargo test -p kb-cli --test composite_save_journey
cargo test -p kb-mcp --test tools
cargo test -p kb-server --test http
cargo test -p kb-cli --test docs_contract
~~~

Expected: all five targets pass.

- [ ] **Step 2: Exercise the real CLI flow outside the test harness**

Create a fresh temporary Vault and one admitted source note. Run kb source save --json, read the returned token without showing it as user-facing text, then run kb source save --confirm TOKEN --json. Confirm the result is applied and source verify passes. In a separate fresh Vault, repeat knowledge save preparation and confirmation, then verify the requested Wiki file exists. Finally exercise kb knowledge save REQUEST --yes in a third fresh Vault.

- [ ] **Step 3: Run outgoing static checks and publish**

Run:

~~~
cargo fmt --all -- --check
git diff --check
git status --short
git push origin HEAD:main
~~~

Expected: formatting and diff checks pass. GitHub native CI provides the full Linux, macOS and Windows evidence after the focused commits are pushed.
