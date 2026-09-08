# Composite Save Entries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (\`- [ ]\`) syntax for tracking.

**Goal:** Add explicit source and knowledge save entry points that preview by default and apply only after a caller passes an explicit execution flag.

**Architecture:** \`kb-app\` owns a coordinator that reuses the existing source review, knowledge-plan creation, selected-Vault apply and operation-summary paths. CLI, MCP and HTTP only translate their local arguments into that coordinator, preserving existing primitive commands and fixed-Vault/write-authorization boundaries.

**Tech Stack:** Rust stable, Clap, Serde JSON, Axum, MCP JSON-RPC, existing \`kb-app\` operation persistence.

**Spec:** \`docs/superpowers/specs/2026-09-08-composite-save-entries-design.md\`

## Global Constraints

- Keep \`schema_version: v1.0\`, existing operation file formats and \`capture_sources\` / \`save_knowledge\` operation kinds unchanged.
- Add no dependencies and no interactive terminal confirmation.
- \`kb source save\` and \`kb knowledge save\` preview by default; only \`--yes\` / \`apply: true\` requests a write.
- Reuse existing stale-plan validation, recovery behavior, locks and selected-Vault checks; do not hold a new lock across planning and apply.
- MCP and HTTP may execute only under their existing \`--allow-write\` policy; HTTP continues to require its existing token policy.
- Keep full identifiers in JSON, MCP and HTTP. Run focused test targets only, then format and diff checks.

---

## File Structure

- \`crates/kb-app/src/app.rs\`: application request variants, selected-Vault coordination and composite response/error helpers.
- \`crates/kb-app/tests/composite_save.rs\`: application-level source/knowledge preview, apply, no-change and stale-error metadata coverage.
- \`crates/kb-cli/src/args.rs\`: \`source save\` and \`knowledge save\` argument parsing mapped to \`kb-app\`.
- \`crates/kb-cli/tests/composite_save_journey.rs\`: real CLI JSON journeys for both new commands.
- \`crates/kb-mcp/src/server.rs\`: new MCP tools, argument decoding and \`allow_write\` enforcement.
- \`crates/kb-mcp/tests/tools.rs\`: tool visibility, preview and write authorization tests.
- \`crates/kb-server/src/lib.rs\`: \`/source/save\` and \`/knowledge/save\` payloads, routes and policy enforcement.
- \`crates/kb-server/tests/http.rs\`: real HTTP preview, denied write and permitted fixed-Vault apply tests.
- \`docs/reference/{commands,mcp,http}.md\`, \`skills/{kb-ingest,kb-save}/SKILL.md\`: public command and Agent usage guidance.
- \`crates/kb-cli/tests/docs_contract.rs\`: source-level contract check for new public names.

### Task 1: Add the shared application coordinator

**Files:**
- Modify: \`crates/kb-app/src/app.rs\`
- Create: \`crates/kb-app/tests/composite_save.rs\`

**Interfaces:**
- Consumes: \`AppRequest::Review\`, \`AppRequest::PlanCreate\`, \`run_apply_for_vault\`, \`OperationSummary\`, \`KbError::with_details\`.
- Produces: \`AppRequest::SourceSave { vault: Option<String>, apply: bool }\` and \`AppRequest::KnowledgeSave { vault: Option<String>, request: KnowledgePlanRequest, apply: bool }\`.
- Produces: composite \`Value\` with \`phase\`, \`preview\`, \`operation_summary\` and \`result\` fields.

- [ ] **Step 1: Write the failing application tests**

Create \`crates/kb-app/tests/composite_save.rs\` with isolated \`AppContext\` paths and these assertions:

\`\`\`rust
#[test]
fn source_save_previews_then_applies_only_when_requested() {
    let preview = run(AppRequest::SourceSave { vault: Some(vault_text.clone()), apply: false }, &context).unwrap();
    assert_eq!(preview["phase"], "planned");
    assert_eq!(preview["result"], serde_json::Value::Null);
    assert_eq!(preview["operation_summary"]["operation_kind"], "capture_sources");

    let applied = run(AppRequest::SourceSave { vault: Some(vault_text), apply: true }, &context).unwrap();
    assert_eq!(applied["phase"], "applied");
    assert_eq!(applied["operation_summary"]["can_apply"], false);
    assert!(applied["result"].is_object());
}

#[test]
fn source_save_without_changes_is_unchanged() {
    let response = run(AppRequest::SourceSave { vault: Some(vault_text), apply: false }, &context).unwrap();
    assert_eq!(response["phase"], "unchanged");
    assert!(response["operation_summary"].is_null());
    assert!(response["result"].is_null());
}
\`\`\`

Add a knowledge request fixture and assert \`KnowledgeSave { apply: false }\` returns \`planned\`, while \`apply: true\` writes the requested Wiki file and returns an applied summary. Add a unit test beside the composite helper that passes a \`PlanStale\` \`KbError\` through the post-plan failure helper and asserts \`error.details["operation_id"]\` equals the complete planned operation ID while \`error.code\` remains \`PlanStale\`.

- [ ] **Step 2: Run the new test target to verify it fails**

Run: \`cargo test -p kb-app --test composite_save\`

Expected: compilation failure because \`SourceSave\` and \`KnowledgeSave\` do not exist.

- [ ] **Step 3: Implement the minimum coordinator**

In \`AppRequest\`, add:

\`\`\`rust
SourceSave { vault: Option<String>, apply: bool },
KnowledgeSave {
    vault: Option<String>,
    request: kb_core::KnowledgePlanRequest,
    apply: bool,
},
\`\`\`

Factor current review and knowledge planning internals into helpers that accept \`&ResolvedVault\`, then let the existing \`Review\` and \`PlanCreate\` request arms select a Vault and call those helpers. Implement:

\`\`\`rust
fn run_source_save(context: &AppContext, vault: Option<String>, apply: bool) -> Result<Value, KbError>;
fn run_knowledge_save(
    context: &AppContext,
    vault: Option<String>,
    request: kb_core::KnowledgePlanRequest,
    apply: bool,
) -> Result<Value, KbError>;
fn coordinated_save_response(
    preview: Value,
    operation_id: Option<OperationId>,
    apply: bool,
    apply_operation: impl FnOnce(OperationId) -> Result<Value, KbError>,
) -> Result<Value, KbError>;
\`\`\`

\`coordinated_save_response\` returns \`{ "phase": "unchanged", "preview": preview, "operation_summary": null, "result": null }\` when no operation ID exists. For a planned ID, retain the full planner response under \`preview\`, copy its planned \`operation_summary\` to the composite root, and return \`result: null\`. When \`apply\` is true, invoke the supplied existing apply path exactly once, inspect the resulting operation state for the applied summary, and return that result under \`result\` with \`phase: "applied"\`.

Create \`run_apply_for_resolved_vault(context, selected: &ResolvedVault, operation_id)\` and have the existing string-selector version call it after selection. The coordinator uses the already selected Vault, so it cannot silently switch Vaults between plan and apply. On apply error, merge \`operation_id\` into an object \`details\` payload; if original details are not an object, preserve them under \`cause\`. Do not change its error code, retryability, message or next action.

- [ ] **Step 4: Run the focused application tests**

Run: \`cargo test -p kb-app --test composite_save\`

Expected: PASS. Confirm the test reads a stored source record after \`apply: true\`, verifies the Wiki file after knowledge apply, and sees no persisted operation for the no-change source case.

- [ ] **Step 5: Commit the coordinator**

\`\`\`bash
git add crates/kb-app/src/app.rs crates/kb-app/tests/composite_save.rs
git commit -m "feat: coordinate source and knowledge saves"
\`\`\`

### Task 2: Expose the two CLI commands

**Files:**
- Modify: \`crates/kb-cli/src/args.rs\`
- Create: \`crates/kb-cli/tests/composite_save_journey.rs\`

**Interfaces:**
- Consumes: \`AppRequest::SourceSave\`, \`AppRequest::KnowledgeSave\`, existing \`KnowledgeRequestArg\` and \`VaultContext\`.
- Produces: \`kb source save [--yes]\` and \`kb knowledge save <REQUEST.json> [--yes]\` real CLI entry points.

- [ ] **Step 1: Write the failing CLI journey**

Create a test that initializes a Vault, configures an admitted \`Notes\` directory, writes one Markdown note, then invokes:

\`\`\`rust
let preview = run(base, &["source", "save", "--vault", vault_text, "--json"]);
assert_eq!(preview["data"]["phase"], "planned");
assert_eq!(preview["data"]["result"], Value::Null);

let saved = run(base, &["source", "save", "--yes", "--vault", vault_text, "--json"]);
assert_eq!(saved["data"]["phase"], "applied");
\`\`\`

Add a knowledge request fixture and assert \`kb knowledge save <request> --json\` leaves \`Wiki/articles/composite.md\` absent, while the identical command with \`--yes\` creates it and returns \`phase: "applied"\`. Run \`kb source save --help\` and \`kb knowledge save --help\`; assert their output contains \`--yes\` and the corresponding domain wording.

- [ ] **Step 2: Run the CLI test to verify it fails**

Run: \`cargo test -p kb-cli --test composite_save_journey\`

Expected: command parsing failure because \`source save\` and \`knowledge save\` are not registered.

- [ ] **Step 3: Parse and map the new commands**

Extend \`SourceCommands\` with:

\`\`\`rust
Save { #[arg(long)] yes: bool, #[command(flatten)] context: VaultContext }
\`\`\`

Add \`Commands::Knowledge { command: KnowledgeCommands }\` and \`KnowledgeCommands::Save { request: KnowledgeRequestArg, #[arg(long)] yes: bool, #[command(flatten)] context: VaultContext }\`. Map valid requests to the two new \`AppRequest\` variants. Preserve \`KnowledgeRequestArg::Invalid\` so missing or invalid request files keep the existing JSON error contract. Set the CLI \`json\` value from \`VaultContext\` and leave \`full_hashes\` handling unchanged.

- [ ] **Step 4: Run the CLI journey**

Run: \`cargo test -p kb-cli --test composite_save_journey\`

Expected: PASS. Inspect filesystem assertions to confirm preview does not write source records or Wiki content, and \`--yes\` writes only through the returned operation.

- [ ] **Step 5: Commit the CLI entry points**

\`\`\`bash
git add crates/kb-cli/src/args.rs crates/kb-cli/tests/composite_save_journey.rs
git commit -m "feat: add composite save CLI commands"
\`\`\`

### Task 3: Add MCP composite tools with existing write policy

**Files:**
- Modify: \`crates/kb-mcp/src/server.rs\`
- Modify: \`crates/kb-mcp/tests/tools.rs\`

**Interfaces:**
- Consumes: \`AppRequest::SourceSave\`, \`AppRequest::KnowledgeSave\`, fixed \`vault_selector\`, \`allow_write\`.
- Produces: \`kb_source_save({ apply? })\` and \`kb_knowledge_save({ request, apply? })\`.

- [ ] **Step 1: Write failing MCP tool tests**

Add a test to the default read/planning MCP server that requires both new names in \`tools/list\`, then calls:

\`\`\`rust
let preview = call(&mut server, 3, "kb_source_save", &json!({}));
assert_eq!(preview["result"]["isError"], false);
assert_eq!(preview["result"]["structuredContent"]["data"]["phase"], "planned");

let denied = call(&mut server, 4, "kb_source_save", &json!({"apply": true}));
assert_eq!(denied["result"]["isError"], true);
assert_eq!(denied["result"]["structuredContent"]["error"]["code"], "auth_denied");
\`\`\`

Add a write-enabled test calling \`kb_knowledge_save\` with \`apply: true\`; assert \`phase: "applied"\`, an applied operation summary, and the expected Wiki file. Add a second Vault fixture and prove a composite tool started for the fixed first Vault never writes the second Vault.

- [ ] **Step 2: Run MCP tests to verify they fail**

Run: \`cargo test -p kb-mcp --test tools\`

Expected: failing tool-list assertion and unknown composite tool error.

- [ ] **Step 3: Register, decode and authorize the MCP requests**

Add both tools to \`McpServer::tools()\` for every server. Give \`kb_source_save\` an object schema with optional boolean \`apply\` defaulting to false. Give \`kb_knowledge_save\` an object schema with required \`request\` and optional boolean \`apply\` defaulting to false. Add \`SourceSaveArguments { #[serde(default)] apply: bool }\` and \`KnowledgeSaveArguments { request: KnowledgePlanRequest, #[serde(default)] apply: bool }\` with \`deny_unknown_fields\`.

Introduce an internal dispatch result so JSON-RPC argument failures remain protocol errors while authorization failures remain stable Knowledge-Brain errors:

\`\`\`rust
enum ToolRequestError {
    InvalidArguments(String),
    Application(KbError),
}
\`\`\`

Make \`app_request\` return \`Result<AppRequest, ToolRequestError>\`. In \`call_tool\`, map \`InvalidArguments\` to the existing \`-32602\` protocol error and map \`Application(error)\` to \`success(id, &tool_error(error))\`. Map \`apply: false\` to the new requests. For \`apply: true\` while \`allow_write\` is false, return \`ToolRequestError::Application(KbError::new(ErrorCode::AuthDenied, ...))\` before calling \`kb_app::run\`; do not create a plan. For a permitted request pass \`Some(self.vault_selector.clone())\` and \`apply: true\`. Keep \`kb_apply_operation\` registration and behavior unchanged.

- [ ] **Step 4: Run MCP tests**

Run: \`cargo test -p kb-mcp --test tools\`

Expected: PASS, including default preview access, denied direct execution and write-enabled fixed-Vault behavior.

- [ ] **Step 5: Commit MCP support**

\`\`\`bash
git add crates/kb-mcp/src/server.rs crates/kb-mcp/tests/tools.rs
git commit -m "feat: expose composite saves over MCP"
\`\`\`

### Task 4: Add HTTP composite routes with existing policy

**Files:**
- Modify: \`crates/kb-server/src/lib.rs\`
- Modify: \`crates/kb-server/tests/http.rs\`

**Interfaces:**
- Consumes: \`AppRequest::SourceSave\`, \`AppRequest::KnowledgeSave\`, \`ServerPolicy::allow_write\`, fixed \`selected(&state)\`.
- Produces: \`POST /source/save\` and \`POST /knowledge/save\` JSON routes.

- [ ] **Step 1: Write failing HTTP route tests**

Add an async test that starts a token-protected, read-only server, posts \`{}\` to \`/source/save\`, and asserts \`200\` plus \`data.phase == "planned"\`. Post \`{"apply":true}\` and assert \`403\`, \`auth_denied\`, and no source record. Add a write-enabled server test:

\`\`\`rust
let body = serde_json::to_string(&json!({"request": knowledge_request(), "apply": true})).unwrap();
let (status, response) = request(&server, "POST", "/knowledge/save", Some("secret"), &body).await;
assert_eq!(status, 200, "{response}");
assert_eq!(response["data"]["phase"], "applied");
assert!(vault.join("Wiki/articles/http.md").is_file());
\`\`\`

Include an invalid body assertion (\`400\`, \`invalid_config\`) and use a second Vault operation to prove the HTTP service cannot target another Vault.

- [ ] **Step 2: Run HTTP tests to verify they fail**

Run: \`cargo test -p kb-server --test http\`

Expected: \`404\` for both new routes before their registration.

- [ ] **Step 3: Add payloads, routes and handlers**

Add routes before the fallback:

\`\`\`rust
.route("/source/save", post(source_save))
.route("/knowledge/save", post(knowledge_save))
\`\`\`

Define request payloads with Serde defaults:

\`\`\`rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSaveBody { #[serde(default)] apply: bool }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnowledgeSaveBody {
    request: KnowledgePlanRequest,
    #[serde(default)]
    apply: bool,
}
\`\`\`

Authenticate before parsing or calling the application. If \`apply\` is true and \`state.policy.allow_write()\` is false, return the same \`403\` authorization style as \`/operations/{id}/apply\` before calling the application. Otherwise call the matching \`AppRequest\` with \`vault: Some(selected(&state))\`. Keep \`/review\`, \`/plans\` and \`/operations/{id}/apply\` unchanged.

- [ ] **Step 4: Run HTTP tests**

Run: \`cargo test -p kb-server --test http\`

Expected: PASS, including token checks, preview-only route behavior, direct write policy and fixed-Vault isolation.

- [ ] **Step 5: Commit HTTP support**

\`\`\`bash
git add crates/kb-server/src/lib.rs crates/kb-server/tests/http.rs
git commit -m "feat: expose composite saves over HTTP"
\`\`\`

### Task 5: Publish the command and Agent contract

**Files:**
- Modify: \`docs/reference/commands.md\`
- Modify: \`docs/reference/mcp.md\`
- Modify: \`docs/reference/http.md\`
- Modify: \`skills/kb-ingest/SKILL.md\`
- Modify: \`skills/kb-save/SKILL.md\`
- Modify: \`crates/kb-cli/tests/docs_contract.rs\`

**Interfaces:**
- Consumes: the implemented CLI commands, MCP tools, HTTP routes and composite response contract.
- Produces: one public usage description that names preview default, explicit execution, unchanged output and retained primitives.

- [ ] **Step 1: Write the failing documentation contract checks**

Extend the documentation tests to require these literal public names:

\`\`\`rust
for contract in [
    "kb source save",
    "kb knowledge save",
    "kb_source_save",
    "kb_knowledge_save",
    "POST /source/save",
    "POST /knowledge/save",
    "apply: true",
] {
    assert!(reference.contains(contract), "missing {contract}");
}
\`\`\`

Use \`kb --help\` output to assert \`knowledge\` is a real top-level command and \`commands.md\` names both composite CLI invocations.

- [ ] **Step 2: Run the documentation contract test to verify it fails**

Run: \`cargo test -p kb-cli --test docs_contract\`

Expected: failing missing-contract assertions until the reference files are updated.

- [ ] **Step 3: Update durable usage guidance**

In \`commands.md\`, add a composite-save subsection that shows both commands, says preview is the default, defines \`--yes\` as the direct-write form, documents \`planned\` / \`applied\` / \`unchanged\`, and points advanced callers to existing primitives. In MCP and HTTP references, list both new interfaces and state that \`apply: true\` is denied without \`--allow-write\` before a plan is created.

Update \`kb-ingest\` to prefer \`kb_source_save\` / \`kb source save\` where available, display its operation summary, and only use its explicit execution flag after user confirmation. Update \`kb-save\` equivalently for \`kb_knowledge_save\` and \`kb knowledge save\`; retain the old primitives as a fallback for inspection or custom workflow. Do not make a Skill claim that a tool call itself is user authorization.

- [ ] **Step 4: Run documentation and command checks**

Run: \`cargo test -p kb-cli --test docs_contract && cargo test -p kb-cli --test composite_save_journey\`

Expected: PASS. Confirm \`kb --help\`, \`kb source save --help\` and \`kb knowledge save --help\` describe the same preview/write distinction as the docs.

- [ ] **Step 5: Commit public contract updates**

\`\`\`bash
git add docs/reference/commands.md docs/reference/mcp.md docs/reference/http.md skills/kb-ingest/SKILL.md skills/kb-save/SKILL.md crates/kb-cli/tests/docs_contract.rs
git commit -m "docs: document composite save workflows"
\`\`\`

### Task 6: Verify the composed user workflows and prepare the change

**Files:**
- Modify only if a focused check exposes a defect in Tasks 1–5.

**Interfaces:**
- Consumes: completed CLI, application, MCP, HTTP and documentation contracts.
- Produces: evidence for the real preview and direct-write paths without a local full-workspace run.

- [ ] **Step 1: Run the targeted verification set**

Run:

\`\`\`bash
cargo test -p kb-app --test composite_save
cargo test -p kb-cli --test composite_save_journey
cargo test -p kb-mcp --test tools
cargo test -p kb-server --test http
cargo test -p kb-cli --test docs_contract
\`\`\`

Expected: every selected test target passes.

- [ ] **Step 2: Run static outgoing checks**

Run:

\`\`\`bash
cargo fmt --all -- --check
git diff --check
git status --short
\`\`\`

Expected: formatter and diff checks pass; status contains only the intentional commits or any documented follow-up fix.

- [ ] **Step 3: Exercise direct CLI entry paths outside the test harness**

Run the release or debug \`kb\` binary in a fresh temporary Vault: initialize it, add one admission entry and note, use \`kb source save --json\`, inspect \`phase: planned\`, then use \`kb source save --yes --json\` and inspect \`phase: applied\`. Create one valid knowledge request file, repeat with \`kb knowledge save\` then \`kb knowledge save --yes\`, and confirm the intended \`Wiki\` file exists. Record any discrepancy before claiming completion.

- [ ] **Step 4: Push the focused, committed implementation**

Run:

\`\`\`bash
git push origin HEAD:main
\`\`\`

Expected: GitHub receives the commits; native CI then remains the evidence source for Linux, macOS and Windows full-workspace coverage.
