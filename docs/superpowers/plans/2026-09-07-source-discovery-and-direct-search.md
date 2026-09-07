# Source Discovery and Direct Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the first Stage 2 slice: deterministic discovery of admitted local sources, reviewed content-addressed source capture, built-in UTF-8 text extraction, a rebuildable catalog, and direct Markdown/source queries that continue to work without cache files.

**Architecture:** `kb-core` owns source, extraction, review, query, and operation contracts. `kb-app` owns filesystem traversal, source records, operation persistence/apply, extraction implementations, catalog construction, and direct search. `kb-cli` only maps commands to `AppRequest`; every new workflow remains callable without CLI parsing.

**Tech Stack:** Rust 2024, MSRV 1.85, serde/serde_json/serde_yaml_ng, sha2, globset 0.4.18, time 0.3.45, existing atomic writes and OS file locks.

**Spec:** [Knowledge-Brain design](../specs/2026-09-07-knowledge-brain-design.md), especially sections 5, 7, 8, 10, 11, 13, 17, 25, and 26.

## Global Constraints

- Support native Windows, macOS, and Linux paths; never follow symbolic links, junctions, or reparse points.
- Read only enabled `admission.yml` entries and apply each entry's include/exclude filters.
- Keep Markdown, YAML, JSON, and immutable source objects authoritative; every catalog and extraction artifact must be deletable and rebuildable.
- `kb review` may write derived scan cache and a user-state operation plan, but it must not modify `Wiki/`, `admission.yml`, topic directories, or `Wiki/log.md`.
- When review finds captureable changes, it returns one source-capture `operation_id`; only `kb apply <operation_id>` writes source objects and source records.
- Revalidate the Vault schema, `admission.yml`, relevant configuration, source hashes, and existing destination hashes after acquiring the exclusive apply lock.
- Treat a possible move as a reported relationship between one deletion and one addition; never infer identity from equal content alone.
- Direct query reads actual Markdown or immutable source objects. A missing, stale, or malformed cache cannot prevent direct query.
- `--scope all` returns separate Wiki and source result groups; it never merges them into one ranking.
- No built-in LLM, network access, URL capture, BM25, embedding, reranking, HTML/EPUB/DOCX/PDF text extraction, watcher, daemon, MCP, HTTP, WebUI, or GUI in this plan.
- Unsupported binary files may be captured as immutable objects with `unsupported` extraction status; they do not become searchable text.
- Use schema string `"v1.0"` and stable snake_case machine fields and error codes.
- Every behavior change starts with a failing test, follows the real `AppRequest` path, and ends with a focused commit.

## File Structure

```text
crates/kb-core/src/source.rs             source identity, versions, changes and records
crates/kb-core/src/extraction.rs         extractor input/output contracts
crates/kb-core/src/search.rs             query scope, result groups and catalog contracts
crates/kb-core/src/operation.rs          adoption and source-capture operation types
crates/kb-app/src/discovery.rs           admitted traversal, glob filters, hashing and limits
crates/kb-app/src/extract.rs             built-in UTF-8 extractors and media classification
crates/kb-app/src/source_record.rs       deterministic OKF-compatible source Markdown
crates/kb-app/src/review.rs              inventory comparison and capture-plan creation
crates/kb-app/src/source_apply.rs        verified all-or-restore source capture
crates/kb-app/src/search/catalog.rs      rebuildable catalog generation and validation
crates/kb-app/src/search/direct.rs       actual-file section search and result ordering
crates/kb-app/src/search/mod.rs          search facade
crates/kb-app/src/source_verify.rs       immutable object and record integrity checks
crates/kb-cli/tests/review.rs            real CLI discovery/capture journeys
crates/kb-cli/tests/query.rs             real CLI cache-independent query journeys
crates/kb-cli/tests/source.rs            real CLI source verification
scripts/check-stage-2.sh                 Unix native Stage 2 gate
scripts/check-stage-2.ps1                Windows native Stage 2 gate
```

The first Stage 2 slice intentionally stops at UTF-8 text and structured-text extraction. HTML, EPUB, DOCX, and PDF extraction form a separate Stage 2 plan because their parser dependencies, hostile-input limits, and format-specific location metadata need independent acceptance tests.

---

### Task 1: Define source, extraction, search, and operation contracts

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/kb-core/src/lib.rs`
- Modify: `crates/kb-core/src/error.rs`
- Modify: `crates/kb-core/src/operation.rs`
- Create: `crates/kb-core/src/source.rs`
- Create: `crates/kb-core/src/extraction.rs`
- Create: `crates/kb-core/src/search.rs`
- Create: `crates/kb-core/tests/source.rs`
- Create: `crates/kb-core/tests/search.rs`

**Interfaces:**
- Produces: `SourceId`, `SourceVersion`, `MediaType`, `ExtractionStatus`, `ExtractedDocument`, `SourceChange`, `ReviewReport`, `SearchRequest`, `SearchResponse`, `Catalog`, `SourceCapturePlan`, and `OperationKind::CaptureSources`.
- Consumes: existing `PortableRelativePath`, `OperationId`, `SchemaVersion`, and `CURRENT_SCHEMA_VERSION`.

- [ ] **Step 1: Write failing source identity and search serialization tests**

```rust
#[test]
fn source_identity_has_logical_and_exact_uris() {
    let source = SourceId::new(
        "notes",
        PortableRelativePath::parse("Guides/setup.md").unwrap(),
    )
    .unwrap();
    assert_eq!(source.logical_uri(), "kb-source://notes/Guides/setup.md");
    let version = SourceVersion::new(source, "a".repeat(64)).unwrap();
    assert_eq!(
        version.exact_uri(),
        format!("kb-source://notes/Guides/setup.md?sha256={}", "a".repeat(64))
    );
}

#[test]
fn all_scope_serializes_as_separate_groups() {
    let response = SearchResponse {
        schema_version: CURRENT_SCHEMA_VERSION,
        query: "cache".to_owned(),
        groups: vec![
            SearchGroup { scope: SearchScope::Wiki, results: vec![] },
            SearchGroup { scope: SearchScope::Sources, results: vec![] },
        ],
    };
    let json = serde_json::to_value(response).unwrap();
    assert_eq!(json["groups"][0]["scope"], "wiki");
    assert_eq!(json["groups"][1]["scope"], "sources");
}
```

- [ ] **Step 2: Run the focused tests and verify missing symbols fail**

Run:

```bash
cargo test -p kb-core --test source --test search
```

Expected: compilation fails because the new contract types do not exist.

- [ ] **Step 3: Add exact domain contracts**

Define these public shapes:

```rust
pub struct SourceId {
    pub admission_id: String,
    pub relative_path: PortableRelativePath,
}

pub struct SourceVersion {
    pub source: SourceId,
    pub sha256: String,
}

pub enum MediaType {
    Markdown,
    PlainText,
    Yaml,
    Json,
    Csv,
    Pdf,
    Other,
}

pub enum ExtractionStatus {
    TextReady,
    MetadataOnly,
    Unsupported,
}

pub struct ExtractedBlock {
    pub heading: Option<String>,
    pub text: String,
    pub line_start: Option<u64>,
}

pub struct ExtractedDocument {
    pub status: ExtractionStatus,
    pub extractor_id: String,
    pub extractor_version: String,
    pub blocks: Vec<ExtractedBlock>,
    pub warnings: Vec<String>,
}

pub enum SourceChangeKind {
    Added,
    Modified,
    Deleted,
}

pub struct SourceChange {
    pub kind: SourceChangeKind,
    pub source: SourceId,
    pub previous_sha256: Option<String>,
    pub current_sha256: Option<String>,
    pub possible_move_from: Vec<SourceId>,
}

pub struct ReviewReport {
    pub schema_version: SchemaVersion,
    pub vault_id: Uuid,
    pub operation_id: Option<OperationId>,
    pub changes: Vec<SourceChange>,
    pub unchanged: u64,
    pub skipped: Vec<SkippedSource>,
}
```

`SourceId::logical_uri` percent-encodes the admission ID as one URI authority component and percent-encodes each UTF-8 path segment while preserving `/` separators. Unreserved ASCII bytes remain readable. This preserves Stage 1 admission IDs without treating arbitrary display text as raw URI syntax.

Define search contracts with these fields:

```rust
pub enum SearchScope { Wiki, Sources, All }

pub struct SearchRequest {
    pub query: String,
    pub scope: SearchScope,
    pub limit: usize,
}

pub struct SearchHit {
    pub path: PortableRelativePath,
    pub source_uri: Option<String>,
    pub title: String,
    pub heading: Option<String>,
    pub line_start: Option<u64>,
    pub snippet: String,
    pub match_count: u64,
}

pub struct SearchGroup {
    pub scope: SearchScope,
    pub results: Vec<SearchHit>,
}

pub struct SearchResponse {
    pub schema_version: SchemaVersion,
    pub query: String,
    pub groups: Vec<SearchGroup>,
}

pub struct CatalogEntry {
    pub scope: SearchScope,
    pub path: PortableRelativePath,
    pub sha256: String,
    pub title: String,
    pub headings: Vec<String>,
}

pub struct Catalog {
    pub schema_version: SchemaVersion,
    pub indexer_version: String,
    pub entries: Vec<CatalogEntry>,
}
```

Extend `OperationKind` with `CaptureSources` and define the exact plan contract:

```rust
pub struct SourceCaptureItem {
    pub source: SourceVersion,
    pub size: u64,
    pub media_type: MediaType,
    pub extraction: ExtractedDocument,
    pub record_path: PortableRelativePath,
    pub record_before_sha256: Option<String>,
    pub record_markdown: String,
}

pub struct SourceCapturePlan {
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub admission_sha256: String,
    pub read_config_sha256: String,
    pub items: Vec<SourceCaptureItem>,
    pub created_at: String,
    pub app_version: String,
}

pub struct SourceCaptureResult {
    pub operation_id: OperationId,
    pub vault_id: Uuid,
    pub target: PathBuf,
    pub captured: Vec<String>,
    pub marked_missing: Vec<String>,
    pub warnings: Vec<String>,
}
```

A deleted source uses its last `SourceVersion`, `size`, and media type, sets the generated record's `present` field to `false`, and keeps the prior object. `SourceCaptureItem` needs no separate delete shape because its complete target record is already in `record_markdown`.

Add stable `ErrorCode::LimitExceeded`, `ErrorCode::SourceIntegrityFailed`, and `ErrorCode::InvalidQuery`.

- [ ] **Step 4: Validate constructors at typed boundaries**

`SourceId::new` must reject blank admission IDs and reuse `PortableRelativePath`. `SourceVersion::new` must accept exactly 64 lowercase hexadecimal characters. `SearchRequest::validate` must reject blank queries and limits outside `1..=100`.

- [ ] **Step 5: Run core tests**

Run:

```bash
cargo test -p kb-core --test source --test search
```

Expected: all source and search contract tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/kb-core
git commit -m "feat: define source and direct-search contracts"
```

---

### Task 2: Add deterministic built-in text extraction

**Files:**
- Modify: `crates/kb-app/Cargo.toml`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/src/extract.rs`
- Create: `crates/kb-app/tests/extract.rs`

**Interfaces:**
- Consumes: `MediaType`, `ExtractedBlock`, `ExtractedDocument`, and `ExtractionStatus`.
- Produces: `classify_media_type(path: &Path) -> MediaType` and `extract_bytes(media_type: MediaType, bytes: &[u8]) -> ExtractedDocument`.

- [ ] **Step 1: Write failing extraction tests**

```rust
#[test]
fn markdown_is_split_at_atx_headings_without_losing_text() {
    let result = extract_bytes(
        MediaType::Markdown,
        b"# Install\nRun cargo.\n\n## Verify\nRun tests.\n",
    );
    assert_eq!(result.status, ExtractionStatus::TextReady);
    assert_eq!(result.blocks[0].heading.as_deref(), Some("Install"));
    assert_eq!(result.blocks[1].heading.as_deref(), Some("Verify"));
    assert!(result.blocks.iter().any(|block| block.text.contains("Run cargo.")));
}

#[test]
fn invalid_utf8_is_metadata_only_and_binary_is_unsupported() {
    assert_eq!(
        extract_bytes(MediaType::PlainText, &[0xff]).status,
        ExtractionStatus::MetadataOnly
    );
    assert_eq!(
        extract_bytes(MediaType::Other, b"data").status,
        ExtractionStatus::Unsupported
    );
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test extract
```

Expected: compilation fails because `extract_bytes` does not exist.

- [ ] **Step 3: Implement media classification and extraction**

Map `.md/.markdown`, `.txt`, `.yaml/.yml`, `.json`, `.csv`, and `.pdf` case-insensitively. Treat valid UTF-8 Markdown, text, YAML, JSON, and CSV as `text_ready`; validate JSON and YAML syntax only to add warnings, never infer business meaning. Split Markdown on ATX headings and retain one-based starting lines. Return `metadata_only` for an expected text format with invalid UTF-8 and `unsupported` for every other format, including PDF in this slice.

- [ ] **Step 4: Run extraction tests**

Run:

```bash
cargo test -p kb-app --test extract
```

Expected: all extraction tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/kb-app
git commit -m "feat: extract admitted UTF-8 sources"
```

---

### Task 3: Traverse only admitted files with portable glob and limit rules

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/kb-app/Cargo.toml`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/src/discovery.rs`
- Create: `crates/kb-app/tests/discovery.rs`

**Interfaces:**
- Consumes: `AdmissionDocument`, `EffectiveConfig`, `SourceId`, `MediaType`, and the path/link validators.
- Produces: `discover_sources(root, admission, config) -> Result<DiscoverySnapshot, KbError>`.

- [ ] **Step 1: Write failing boundary tests**

Create a Vault fixture with enabled `Notes/`, disabled `Private/`, included Markdown/text, excluded `Notes/tmp/**`, a hidden file, and a nested link fixture where supported. Assert:

```rust
let snapshot = discover_sources(&vault, &admission, &config).unwrap();
let uris = snapshot
    .sources
    .iter()
    .map(|source| source.version.source.logical_uri())
    .collect::<Vec<_>>();
assert_eq!(uris, vec!["kb-source://notes/guide.md"]);
assert!(snapshot.skipped.iter().any(|item| item.reason == "hidden"));
assert!(snapshot.skipped.iter().any(|item| item.reason == "link"));
```

Add tests that `max_files_per_review`, `max_file_bytes`, and `max_total_read_bytes` fail with `limit_exceeded` before a partial capture plan is saved.

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test discovery
```

Expected: compilation fails because discovery is not implemented.

- [ ] **Step 3: Add the compatible glob dependency**

Pin `globset = "=0.4.18"` in workspace dependencies and consume it from `kb-app`. Build include and exclude `GlobSet` values with literal separators and disabled backslash escaping so serialized Vault patterns always use `/`.

- [ ] **Step 4: Implement deterministic traversal**

Walk enabled admission roots in admission order and entries in normalized portable-path order. Never recurse through links/reparse points. Apply hidden-file policy before include/exclude matching; always exclude `.git` and `.kb` even if a custom include is broad. Read and SHA-256 each accepted file through a 64 KiB buffer. Enforce every configured limit and return a single `limit_exceeded` error with the exact limit, observed value, and offending path.

- [ ] **Step 5: Run discovery and existing admission tests**

Run:

```bash
cargo test -p kb-app --test discovery
cargo test -p kb-core --test admission
```

Expected: all focused tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/kb-app
git commit -m "feat: discover only admitted source files"
```

---

### Task 4: Make source records the durable discovery baseline

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/kb-app/Cargo.toml`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/src/source_record.rs`
- Create: `crates/kb-app/tests/source_record.rs`

**Interfaces:**
- Consumes: `SourceVersion`, `MediaType`, `ExtractionStatus`, and current application version.
- Produces: `SourceRecord`, `render_source_record`, `parse_source_record`, `record_path_for`, and `load_source_inventory`.

Use this owned record shape:

```rust
pub struct SourceRecord {
    pub source: SourceVersion,
    pub title: String,
    pub size: u64,
    pub media_type: MediaType,
    pub extraction_status: ExtractionStatus,
    pub present: bool,
    pub generated_at: String,
}
```

- [ ] **Step 1: Write failing source-record round-trip tests**

```rust
#[test]
fn source_record_is_human_readable_okf_and_round_trips() {
    let source = SourceId::new(
        "notes",
        PortableRelativePath::parse("guide.md").unwrap(),
    )
    .unwrap();
    let record = SourceRecord {
        source: SourceVersion::new(source, "a".repeat(64)).unwrap(),
        title: "guide.md".to_owned(),
        size: 12,
        media_type: MediaType::Markdown,
        extraction_status: ExtractionStatus::TextReady,
        present: true,
        generated_at: "2026-09-07T00:00:00Z".to_owned(),
    };
    let markdown = render_source_record(&record).unwrap();
    assert!(markdown.starts_with("---\ntype: Reference\n"));
    assert!(markdown.contains("resource: kb-source://notes/guide.md"));
    assert!(markdown.contains("sha256:"));
    assert_eq!(parse_source_record(&markdown).unwrap(), record);
}

#[test]
fn record_path_is_stable_and_does_not_trust_admission_id_as_a_path() {
    let source = SourceId::new(
        "../display id",
        PortableRelativePath::parse("guide.md").unwrap(),
    )
    .unwrap();
    let path = record_path_for(&source).unwrap();
    assert!(path.as_str().starts_with("Wiki/external-sources/records/"));
    assert!(!path.as_str().contains(".."));
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test source_record
```

Expected: compilation fails because source-record functions do not exist.

- [ ] **Step 3: Add RFC 3339 formatting**

Pin `time = { version = "=0.3.45", features = ["formatting"] }` in workspace dependencies and consume it from `kb-app`.

- [ ] **Step 4: Implement source Markdown**

Render UTF-8 Markdown with OKF-compatible frontmatter:

```yaml
---
type: Reference
title: guide.md
resource: kb-source://notes/guide.md
generated:
  by: process:knowledge-brain/0.1.0
  at: 2026-09-07T00:00:00Z
kb:
  source:
    admission_id: notes
    relative_path: guide.md
    sha256: <64 lowercase hex>
    size: 123
    media_type: markdown
    extraction_status: text_ready
    present: true
---
```

Use the SHA-256 of the logical source URI as the portable record filename under `Wiki/external-sources/records/<first-two-hex>/<full-hash>.md`. The content remains human-readable while untrusted admission IDs never become filesystem components. Parse only the owned `kb.source` fields needed for inventory and tolerate unrelated OKF fields.

- [ ] **Step 5: Load inventory from Markdown, not cache**

`load_source_inventory` recursively reads source-record Markdown, excludes `.objects/`, rejects duplicate logical URIs, and returns records ordered by logical URI. A malformed managed record returns `invalid_config` with its exact path; it is never silently replaced.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p kb-app --test source_record
```

Expected: all source-record tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/kb-app
git commit -m "feat: represent captured sources as readable records"
```

---

### Task 5: Review changes and persist a source-capture plan

**Files:**
- Modify: `crates/kb-app/src/lib.rs`
- Modify: `crates/kb-app/src/operation.rs`
- Create: `crates/kb-app/src/review.rs`
- Create: `crates/kb-app/tests/review.rs`

**Interfaces:**
- Consumes: discovery snapshots, source inventory, extraction, operation storage, effective configuration, admission, and Vault identity.
- Produces: `review_sources(root, user_paths, overrides) -> Result<ReviewReport, KbError>`.

- [ ] **Step 1: Write failing added/modified/deleted/move tests**

Exercise four consecutive inventories:

1. no records + `Notes/a.md` ⇒ one `added`;
2. applied record with changed bytes ⇒ one `modified`;
3. applied record with missing path ⇒ one `deleted`;
4. deleted `a.md` plus added `b.md` with equal SHA-256 ⇒ separate delete/add changes and `possible_move_from` on the addition.

Assert disabled and excluded files never appear, and assert review leaves a byte-for-byte Vault snapshot unchanged except beneath `.kb/cache/`.

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test review
```

Expected: compilation fails because `review_sources` does not exist.

- [ ] **Step 3: Generalize operation persistence without changing adoption JSON**

Change `save_plan` and `save_result` to generic serializers that receive the operation ID. Inspect `kind` from stored JSON before deserializing into `AdoptionPlan` or `SourceCapturePlan`. Replace the two-variant operation state with:

```rust
pub enum OperationState {
    PlannedAdoption(AdoptionPlan),
    PlannedSourceCapture(SourceCapturePlan),
    AppliedAdoption(AdoptionResult),
    AppliedSourceCapture(SourceCaptureResult),
}
```

Preserve the existing flattened adoption plan/result JSON and every Stage 1 CLI response.

- [ ] **Step 4: Implement deterministic comparison and plan creation**

Compare discovery by logical URI against `load_source_inventory`. A change list must be sorted by logical URI. Build complete source record text at review time, record the SHA-256 of `admission.yml`, the effective config fields that control reads, each current source hash, each existing record hash, and every object destination.

If there are no changes, return `operation_id: null` and save no plan. If there are changes, save one `capture_sources` plan in the user state directory and return its ID. Review may atomically write a hash acceleration cache, but inventory truth remains the source-record Markdown.

- [ ] **Step 5: Verify repeat review behavior**

Before apply, repeating review must continue to report the same changes and may create a new equivalent plan. After apply, repeating review must report zero changes. Deleting all `.kb/cache/` files must not turn captured records into missing durable inventory.

- [ ] **Step 6: Run review and adoption regressions**

Run:

```bash
cargo test -p kb-app --test review
cargo test -p kb-app --test adopt
cargo test -p kb-cli --test adopt
```

Expected: all focused tests pass and Stage 1 adoption JSON remains compatible.

- [ ] **Step 7: Commit**

```bash
git add crates/kb-app crates/kb-cli/tests/adopt.rs
git commit -m "feat: review admitted source changes"
```

---

### Task 6: Apply source capture with all-or-restore behavior

**Files:**
- Modify: `crates/kb-app/src/adopt.rs`
- Modify: `crates/kb-app/src/app.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Modify: `crates/kb-app/src/operation.rs`
- Create: `crates/kb-app/src/source_apply.rs`
- Create: `crates/kb-app/tests/source_apply.rs`

**Interfaces:**
- Consumes: stored `SourceCapturePlan`, Vault locks, atomic replacement, source-record rendering, and extraction.
- Produces: `apply_stored_operation(user_paths, operation_id) -> Result<AppliedOperation, KbError>` and `SourceCaptureResult`.

- [ ] **Step 1: Write failing stale-plan and success tests**

Assert that changing source bytes, `admission.yml`, or an existing source record after review returns `plan_stale` and changes no Vault-owned path. On success assert:

```rust
let object = vault.join(format!(
    "Wiki/external-sources/.objects/sha256/{}/{}",
    &sha256[..2],
    sha256
));
assert_eq!(fs::read(object).unwrap(), original_source_bytes);
assert!(record_path.is_file());
assert!(!vault.join("Wiki/log.md").read_to_string().contains("guide.md"));
```

Repeat the same operation ID and assert an identical durable result with no duplicate record or log entry.

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test source_apply
```

Expected: compilation fails because source apply is not implemented.

- [ ] **Step 3: Implement apply dispatch**

Move adoption-only dispatch out of `adopt.rs`. `apply_stored_operation` reads the operation kind and delegates to adoption or source capture. Keep an adoption wrapper if existing Rust tests need the typed `AdoptionResult`; the `AppRequest::Apply` path uses the generic dispatcher.

- [ ] **Step 4: Implement preflight verification**

Acquire the per-Vault exclusive lock, then verify schema compatibility, Vault ID/root, admission hash, read-controlling config values, current source hashes, record before-hashes, object paths, and generated record hashes. Perform all preflight checks before the first Vault write.

- [ ] **Step 5: Implement recoverable writes**

Store operation progress and original record bytes under the private user-state operation directory. For each item:

1. create an immutable object only when absent;
2. verify an existing object has the expected SHA-256;
3. atomically create or replace the source record;
4. record each completed effect before advancing.

On failure or restart, restore replaced records, remove only objects and records that this operation created and whose hashes still match, and preserve every changed or unrecorded user file. If safe restoration cannot be proven, return `vault_needs_recovery` and block query/write operations.

After durable records and objects verify, write extraction JSON beneath `.kb/cache/extracted/<sha256>/builtin-text-v1.json`. Cache failure adds a warning and leaves the durable result successful.

- [ ] **Step 6: Inject interruption after every durable replacement**

Use the existing adoption interruption-test pattern. For every possible stop point, assert the first call restores the original state or leaves a recognizable completed state; a normal retry must finish; an unrecorded user file must never be removed.

- [ ] **Step 7: Run operation tests**

Run:

```bash
cargo test -p kb-app --test source_apply
cargo test -p kb-app --test adopt
cargo test -p kb-cli --test adopt
```

Expected: source capture and all adoption regressions pass.

- [ ] **Step 8: Commit**

```bash
git add crates/kb-app crates/kb-cli/tests/adopt.rs
git commit -m "feat: apply reviewed source captures"
```

---

### Task 7: Add cache-independent direct Wiki and source search

**Files:**
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/src/search/mod.rs`
- Create: `crates/kb-app/src/search/direct.rs`
- Create: `crates/kb-app/tests/direct_search.rs`

**Interfaces:**
- Consumes: `SearchRequest`, source records, immutable objects, extractors, Vault locks, and read limits.
- Produces: `query(root, request, config) -> Result<SearchResponse, KbError>`.

- [ ] **Step 1: Write failing Wiki search tests**

Create Markdown under `Wiki/research/` and `Wiki/articles/` with repeated terms in titles, headings, and body. Assert direct search returns section-level hits with one-based line numbers, snippets, match counts, deterministic ordering, and no `Wiki/external-sources/` hits in Wiki scope.

- [ ] **Step 2: Write failing source and all-scope tests**

Capture a text source, delete `.kb/cache/`, and assert source scope still searches the immutable object through its record. Assert all scope returns exactly two ordered groups named `wiki` and `sources`, even when one is empty.

- [ ] **Step 3: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test direct_search
```

Expected: compilation fails because direct query is not implemented.

- [ ] **Step 4: Implement actual-file section search**

For Wiki scope, recursively read real Markdown beneath `Wiki/research/` and `Wiki/articles/`, plus `Wiki/index.md`; never use catalog text as content. Split Markdown by ATX headings. Match the lowercased full query and whitespace-separated terms without language detection or hidden mode switches. Rank deterministically by:

1. full-query occurrences descending;
2. number of distinct matched terms descending;
3. title/heading match before body-only match;
4. portable path ascending;
5. line number ascending.

Return at most the validated request limit. Snippets must be bounded and must not include text from a neighboring section.

- [ ] **Step 5: Implement source search from durable objects**

Load source records, verify the referenced object hash before reading, run the built-in extractor on the immutable bytes, and search extracted blocks. Unsupported/metadata-only sources produce no text hit and do not fail unrelated results. A hash mismatch returns `source_integrity_failed`.

- [ ] **Step 6: Run direct search tests**

Run:

```bash
cargo test -p kb-app --test direct_search
```

Expected: all direct search tests pass with and without `.kb/cache/`.

- [ ] **Step 7: Commit**

```bash
git add crates/kb-app
git commit -m "feat: query actual wiki and source text"
```

---

### Task 8: Build a lightweight catalog without making it authoritative

**Files:**
- Modify: `crates/kb-app/src/search/mod.rs`
- Create: `crates/kb-app/src/search/catalog.rs`
- Create: `crates/kb-app/tests/catalog.rs`

**Interfaces:**
- Consumes: real Wiki Markdown, source records, `Catalog`, atomic replacement, and the Vault lock.
- Produces: `rebuild_catalog(root) -> Result<CatalogReport, KbError>` and `load_catalog_if_current(root) -> Option<Catalog>`.

- [ ] **Step 1: Write failing rebuild/deletion/corruption tests**

Assert rebuild writes `.kb/cache/catalog.json` with only paths, hashes, titles, headings, and scope. Assert the file contains no full document body. Delete it and corrupt it in separate cases; direct query must still return the same semantic hits.

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-app --test catalog
```

Expected: compilation fails because catalog functions do not exist.

- [ ] **Step 3: Implement deterministic rebuild**

Read actual files, produce entries sorted by scope and portable path, set `indexer_version: "catalog-v1"`, and atomically replace `.kb/cache/catalog.json`. Validate every path and hash while loading. Return `None` for missing, malformed, unknown-version, or hash-stale catalog data so consumers fall back to actual files.

- [ ] **Step 4: Run catalog and direct-search tests**

Run:

```bash
cargo test -p kb-app --test catalog --test direct_search
```

Expected: all tests pass and cache state never changes query correctness.

- [ ] **Step 5: Commit**

```bash
git add crates/kb-app
git commit -m "feat: build a rebuildable knowledge catalog"
```

---

### Task 9: Expose review, query, cache rebuild, and source verification through the application layer

**Files:**
- Modify: `crates/kb-app/src/app.rs`
- Modify: `crates/kb-app/src/capabilities.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/src/source_verify.rs`
- Modify: `crates/kb-cli/src/args.rs`
- Create: `crates/kb-cli/tests/review.rs`
- Create: `crates/kb-cli/tests/query.rs`
- Create: `crates/kb-cli/tests/source.rs`
- Modify: `crates/kb-cli/tests/json_contract.rs`
- Modify: `crates/kb-cli/tests/docs_contract.rs`

**Interfaces:**
- Produces: `AppRequest::Review`, `AppRequest::Query`, `AppRequest::Cache(CacheRequest::Rebuild)`, and `AppRequest::Source(SourceRequest::Verify)`.
- Consumes: review, direct query, catalog rebuild, source verification, Vault selection, compatibility checks, and shared/exclusive locks.

- [ ] **Step 1: Write failing CLI contract tests**

Exercise:

```text
kb review [--vault <PATH_OR_ID>] [--json]
kb query <QUERY> [--scope wiki|sources|all] [--limit 1..100] [--vault <PATH_OR_ID>] [--json]
kb cache rebuild [--vault <PATH_OR_ID>] [--json]
kb source verify [--vault <PATH_OR_ID>] [--json]
```

Assert JSON stdout is one parseable `v1.0` envelope, failures keep stderr empty in JSON mode, `query --scope all` preserves separate groups, and invalid blank/over-limit queries return `invalid_query`.

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-cli --test review --test query --test source --test json_contract
```

Expected: help/argument tests fail because commands are absent.

- [ ] **Step 3: Add application requests before CLI parsing**

Add typed request families to `AppRequest`. Resolve the Vault and compatibility in `kb-app`; use shared locks for review/query/verify and an exclusive lock for catalog rebuild. Review plan persistence stays in the application workflow. The CLI maps arguments to these requests and adds no filesystem behavior.

- [ ] **Step 4: Implement source verification**

`source verify` loads every managed source record, checks record path validity, object existence, object SHA-256, and record uniqueness, then returns independent pass/fail items without modifying the Vault. Do not fold these checks into `doctor`.

- [ ] **Step 5: Publish honest capabilities**

Set `direct_search: true`, keep `bm25: false`, publish extractor IDs `markdown`, `plain_text`, and `structured_text`, and add `review`, `query`, `cache`, and `source` to the command list.

- [ ] **Step 6: Run CLI and app-layer tests**

Run:

```bash
cargo test -p kb-cli --test review --test query --test source --test json_contract
cargo test -p kb-app --test review --test direct_search --test catalog --test source_apply
```

Expected: all new entry-path tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/kb-app crates/kb-cli
git commit -m "feat: expose source review and direct query commands"
```

---

### Task 10: Document Stage 2 contracts and add the native journey gate

**Files:**
- Modify: `README.md`
- Modify: `ROADMAP.md`
- Modify: `SECURITY.md`
- Modify: `docs/reference/commands.md`
- Create: `docs/reference/sources.md`
- Create: `docs/reference/search.md`
- Create: `docs/guides/review-and-capture-sources.md`
- Modify: `docs/decisions/proposed/architecture/0002-files-are-the-knowledge-source-of-truth.md`
- Modify: `docs/decisions/proposed/architecture/0003-admission-list.md`
- Modify: `docs/decisions/proposed/architecture/0006-direct-search-and-optional-bm25f.md`
- Create: `crates/kb-cli/tests/phase2_journey.rs`
- Create: `scripts/check-stage-2.sh`
- Create: `scripts/check-stage-2.ps1`
- Modify: `.github/workflows/phase-1.yml`

**Interfaces:**
- Consumes: the complete Stage 2A command and JSON contracts.
- Produces: user documentation, a release-binary journey, and native CI evidence hooks.

- [ ] **Step 1: Write the failing real-binary journey**

The journey must:

1. initialize a Vault;
2. create and admit `Notes/`;
3. create Markdown, text, excluded, hidden, and disabled-directory fixtures;
4. run `review` and apply its operation ID;
5. modify one source, delete one, and move one without assuming identity;
6. verify the second review reports exact changes;
7. add manually authored Wiki Markdown;
8. rebuild and then delete/corrupt the catalog;
9. query Wiki, sources, and all scopes after each cache state;
10. run `source verify`;
11. reopen the Vault by stable ID;
12. assert no network listener, Git repository, or user-file rewrite was created.

- [ ] **Step 2: Run and verify failure**

Run:

```bash
cargo test -p kb-cli --test phase2_journey
```

Expected: fails before the new CLI workflows are fully wired.

- [ ] **Step 3: Update documentation by ownership**

Keep README at feature/quick-start depth and link to the source/search references. Put exact source identity, record layout, extraction status, scope, ranking, cache, and failure semantics in reference pages. Put the ordered review/apply workflow in the guide. Keep implementation status only in `ROADMAP.md`.

- [ ] **Step 4: Add native scripts**

`check-stage-2.sh` and `check-stage-2.ps1` must run formatting, Clippy, all tests, release build, and the real release-binary Stage 2 journey with isolated config/state/cache directories. Extend the CI matrix so Linux/macOS call the shell script and Windows calls PowerShell.

- [ ] **Step 5: Run the local Stage 2 gate**

Run:

```bash
bash scripts/check-stage-2.sh
```

Expected on the current machine: formatting, Clippy, all tests, release build, review/apply, source verification, and cache-independent direct queries pass.

- [ ] **Step 6: Evaluate ADRs against evidence**

- Accept ADR-0003 only after real review tests prove disabled, excluded, hidden, linked, and non-admitted paths are not read.
- Keep ADR-0002 proposed until cache deletion and copied-Vault direct reads pass on the required native platforms.
- Keep ADR-0006 proposed because complete optional BM25F is outside this plan, even after direct search ships.

- [ ] **Step 7: Report platform evidence precisely**

Local macOS success does not prove Windows or Linux. Do not mark Stage 2 complete or cross-platform verified until the same release-binary journey succeeds in all three native CI jobs. The Stage 2B document-extractor plan remains required before the full source-extractor scope is complete.

- [ ] **Step 8: Commit**

```bash
git add README.md ROADMAP.md SECURITY.md docs crates/kb-cli/tests/phase2_journey.rs scripts .github/workflows
git commit -m "docs: publish source and direct-search workflows"
```

## Plan Self-Review

- Every Stage 2A behavior has a real `AppRequest` path before a CLI path.
- Admission limits, links, disabled entries, filters, source changes, stale plans, recovery, cache deletion, cache corruption, object integrity, scope separation, and JSON envelopes have named tests.
- Durable source records and objects are independent of discovery, extraction, and catalog caches.
- Existing adoption plan/result JSON remains compatible.
- BM25 and complex document extraction are explicitly excluded rather than partially implemented.
- Stage 2 completion remains blocked on the separate complex-extractor plan and native Windows/Linux evidence.
