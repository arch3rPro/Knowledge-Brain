# Built-in Document Extractors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add dependency-free-at-runtime HTML, EPUB and DOCX source extraction while keeping PDF as metadata-only future scope.

**Architecture:** `kb-core` owns serializable media, link and location contracts. Focused extractors in `kb-app` parse already-bounded bytes; an explicit router selects one extractor and cache code uses the selected extractor identity. EPUB and DOCX share a bounded ZIP reader but retain separate format parsers.

**Tech Stack:** Rust 2024, html2text/html5ever, quick-xml, zip, serde, existing `kb-core`/`kb-app` application path.

**Spec:** `.superpowers/specs/2026-09-07-knowledge-brain-design.md`

## Global Constraints

- Rust version floor remains 1.85 and `unsafe_code` remains forbidden.
- Windows, macOS and Linux use the same parser implementations and serialized contracts.
- No external executable, network request, browser or agent integration is required.
- Extractors receive in-memory bytes and cannot modify the Vault.
- PDF remains metadata-only and PDF text extraction remains future scope.
- Archive entry count, per-entry size, total expanded bytes and emitted text are bounded before allocation or parsing.

---

### Task 1: Extraction contract and routing

**Files:**
- Modify: `crates/kb-core/src/extraction.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Modify: `crates/kb-app/src/extract.rs`
- Modify: `crates/kb-app/tests/extract.rs`

**Interfaces:**
- Produces: `MediaType::{Html,Epub,Docx}`, `ExtractedLink`, `SourceLocation`, document `title`/`links`, and extractor routing through `extract_bytes`.

- [x] **Step 1: Write failing contract tests**

Add literal assertions that `.html/.htm`, `.epub` and `.docx` classify distinctly, PDF remains metadata-only, and serialized documents carry optional title, links and typed location fields.

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p kb-app --test extract`

Expected: compilation fails because the new media and result fields do not exist.

- [x] **Step 3: Add the minimal domain types and explicit router**

Add the media variants and serializable structures. Preserve `line_start` for line-oriented sources and add `location` for document-native positions. Route text formats to `BuiltinTextExtractor`; route each document type to its named extractor; return a `metadata_only` result for PDF and `unsupported` for unknown media.

- [x] **Step 4: Run the focused test and verify GREEN**

Run: `cargo test -p kb-app --test extract`

- [x] **Step 5: Commit**

Run: `git add crates/kb-core crates/kb-app/src/extract.rs crates/kb-app/tests/extract.rs && git commit -m "feat: define document extraction contracts"`

### Task 2: HTML extractor

**Files:**
- Create: `crates/kb-app/src/extract/html.rs`
- Modify: `crates/kb-app/src/extract.rs`
- Modify: `crates/kb-app/Cargo.toml`
- Modify: `Cargo.toml`
- Modify: `crates/kb-app/tests/extract.rs`

**Interfaces:**
- Consumes: bounded HTML bytes and `ExtractedDocument` contracts.
- Produces: `BuiltinHtmlExtractor` with title, readable text, headings and links.

- [x] **Step 1: Write a failing malformed-real-HTML test**

Use HTML containing a title, headings, entities, a relative link, script/style noise and malformed closing tags. Assert visible text and links are retained while script/style content is absent.

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p kb-app --test extract html_ -- --nocapture`

- [x] **Step 3: Implement HTML extraction**

Parse with an HTML5 parser, render visible semantic text without executing content, collect document title and non-empty `href` links, and produce heading blocks with deterministic document-order positions. Invalid or empty output returns `metadata_only` with a diagnostic.

- [x] **Step 4: Run HTML and existing extraction tests**

Run: `cargo test -p kb-app --test extract`

- [x] **Step 5: Commit**

Run: `git add Cargo.toml Cargo.lock crates/kb-app && git commit -m "feat: extract HTML sources"`

### Task 3: Bounded EPUB extraction

**Files:**
- Create: `crates/kb-app/src/extract/archive.rs`
- Create: `crates/kb-app/src/extract/epub.rs`
- Modify: `crates/kb-app/src/extract.rs`
- Modify: `crates/kb-app/Cargo.toml`
- Modify: `Cargo.toml`
- Modify: `crates/kb-app/tests/extract.rs`

**Interfaces:**
- Consumes: EPUB ZIP bytes, OPF manifest/spine and the HTML extraction helper.
- Produces: chapter-ordered blocks with `SourceLocation::Epub { resource, index }`, metadata title and resolved chapter links.

- [x] **Step 1: Write failing EPUB behavior and hostile-archive tests**

Create a minimal EPUB in memory with `container.xml`, OPF metadata, a two-item spine and XHTML chapters stored out of lexical order. Assert metadata title, spine order, chapter links and locations. Add malformed and expanded-size-limit fixtures that must return `metadata_only` without panic.

- [x] **Step 2: Run EPUB tests and verify RED**

Run: `cargo test -p kb-app --test extract epub_ -- --nocapture`

- [x] **Step 3: Implement bounded archive reads and EPUB parsing**

Reject encrypted entries, unsafe names, excessive entries, oversized entries and excessive cumulative expansion before reading. Resolve the rootfile, manifest and spine with quick-xml; normalize archive-relative paths without permitting traversal; parse chapters through the HTML helper.

- [x] **Step 4: Run EPUB and extraction tests**

Run: `cargo test -p kb-app --test extract`

- [x] **Step 5: Commit**

Run: `git add Cargo.toml Cargo.lock crates/kb-app && git commit -m "feat: extract bounded EPUB sources"`

### Task 4: Bounded DOCX extraction

**Files:**
- Create: `crates/kb-app/src/extract/docx.rs`
- Modify: `crates/kb-app/src/extract.rs`
- Modify: `crates/kb-app/tests/extract.rs`

**Interfaces:**
- Consumes: bounded OOXML ZIP entries, relationships, styles and document XML.
- Produces: paragraph and table blocks with `SourceLocation::Docx { paragraph, table }`, heading labels, title metadata and hyperlink targets.

- [x] **Step 1: Write failing DOCX structure tests**

Create a minimal DOCX in memory with core title, heading style, ordinary paragraphs, a table, an external hyperlink relationship and split text runs. Assert text order, heading, table text, title, link and paragraph/table locations. Add corrupt and encrypted archive cases returning `metadata_only`.

- [x] **Step 2: Run DOCX tests and verify RED**

Run: `cargo test -p kb-app --test extract docx_ -- --nocapture`

- [x] **Step 3: Implement OOXML extraction**

Parse core properties, style names, relationships and `word/document.xml` with a streaming XML reader. Join text runs within paragraphs, preserve tabs and line breaks, emit table rows as searchable text and attach external hyperlink targets without attempting visual layout reconstruction.

- [x] **Step 4: Run DOCX and extraction tests**

Run: `cargo test -p kb-app --test extract`

- [x] **Step 5: Commit**

Run: `git add crates/kb-app && git commit -m "feat: extract bounded DOCX sources"`

### Task 5: Application integration, real workflow and documentation

**Files:**
- Modify: `crates/kb-app/src/source_apply.rs`
- Modify: `crates/kb-app/src/search.rs`
- Modify: `crates/kb-app/src/capabilities.rs`
- Modify: `crates/kb-cli/tests/json_contract.rs`
- Modify: `crates/kb-cli/tests/phase2_journey.rs`
- Modify: `docs/reference/sources.md`
- Modify: `docs/reference/search.md`
- Modify: `ROADMAP.md`
- Move: `docs/decisions/proposed/architecture/0010-pure-rust-document-extractors.md` to `docs/decisions/accepted/architecture/0010-pure-rust-document-extractors.md`

**Interfaces:**
- Consumes: routed extraction results and existing `review → apply → query` workflow.
- Produces: actual extractor cache keys, source-search locations and truthful capabilities.

- [x] **Step 1: Write failing integration and release-binary journey tests**

Assert capabilities list all built-in extractor IDs. Exercise a generated HTML source and at least one generated archive source through the real CLI sequence `review → apply → query`; verify `text_ready`, a format-native location, searchable text and an extractor-specific cache file after reopening the Vault.

- [x] **Step 2: Run focused integration tests and verify RED**

Run: `cargo test -p kb-cli --test json_contract --test phase2_journey document_ -- --nocapture`

- [x] **Step 3: Integrate identities, locations and docs**

Build cache paths from the extraction result's ID/version instead of a hardcoded text extractor. Preserve document locations in source search results while retaining `line_start` for Markdown. Document exact supported semantics, limits and PDF metadata-only behavior; mark the ADR accepted only after behavior ships.

- [x] **Step 4: Run task-scoped verification**

Run: `cargo fmt --check`

Run: `cargo test -p kb-core --test admission admission_validates_disabled_entries_and_supplies_effective_filters`

Run: `cargo test -p kb-app --test extract`

Run: `cargo test -p kb-cli --test json_contract version_and_capabilities_are_explicit_contracts`

Run: `cargo test -p kb-cli --test phase2_journey document_sources_survive_review_apply_query_and_reopening`

Run targeted Clippy for the same package and test targets with `-D warnings`. Full workspace tests, release builds and Stage 2 scripts are outside this task's requested verification scope.

- [x] **Step 5: Commit**

Run: `git add . && git commit -m "feat: integrate built-in document extraction"`

## Self-review

- Spec coverage: HTML title/text/links, EPUB title/spine/links, DOCX title/headings/paragraphs/tables/links, archive bounds, PDF future scope, cache identity, source search and application capabilities each have an owning task.
- Placeholder scan: no implementation step delegates unspecified error handling or tests; each malformed-input behavior and verification command is named.
- Type consistency: every format returns the shared `ExtractedDocument`; document-native positions use `SourceLocation`, while existing line-oriented search retains `line_start`.
