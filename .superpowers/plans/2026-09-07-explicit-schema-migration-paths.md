# Explicit Schema Migration Paths Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make schema migration availability depend on an explicit, complete migration path instead of numeric version ordering.

**Architecture:** `kb-core` separates version relation from a validated directed migration catalog. `kb-app` owns the empty production catalog and provides the single Vault compatibility classifier used by status, doctor and mutation guards. No file transformation or `kb migrate` command is introduced because no historical Vault schema has been released.

**Tech Stack:** Rust 2024, serde, existing `kb-core` / `kb-app` / `kb-cli` crate boundaries, assert_cmd integration tests.

**Spec:** `.superpowers/specs/2026-09-07-explicit-schema-migration-paths.md`

## Global Constraints

- Keep `CURRENT_SCHEMA_VERSION` at `v1.0`.
- Do not invent a `v0.9` file layout or rewrite any Vault files.
- A future migration route must have a legacy parser and executable transformation; route metadata alone never makes a production version migratable.
- Preserve current `v1.0`, newer-minor and newer-major behavior except for the additive `older_unsupported` compatibility value and `migration_unavailable` error code.
- Do not add `kb migrate`, MCP tools or HTTP routes in this slice.
- Run focused tests only; do not run the workspace test suite for this feature.

---

## File Structure

- `crates/kb-core/src/version.rs`: canonical version relation plus validated migration catalog and path resolver.
- `crates/kb-core/src/error.rs`: additive stable `migration_unavailable` error code.
- `crates/kb-core/src/lib.rs`: re-export migration catalog vocabulary used by the application crate.
- `crates/kb-core/tests/version.rs`: public core contracts for relations and catalog validation/path resolution.
- `crates/kb-app/src/schema.rs`: empty production catalog, the only Vault compatibility classifier and its private unit tests.
- `crates/kb-app/src/lib.rs`: expose the internal schema module to sibling application modules only.
- `crates/kb-app/src/status.rs`, `crates/kb-app/src/doctor.rs`, `crates/kb-app/src/app.rs`: consume the shared classifier; do not compare numeric versions directly.
- `crates/kb-app/src/config/load.rs`: accept only current schemas in the current generic config parser.
- `crates/kb-cli/tests/json_contract.rs`, `crates/kb-cli/tests/doctor.rs`: real binary JSON contracts for older-unsupported status, diagnostic and rejected mutation.
- `docs/architecture/overview.md`, `docs/reference/commands.md`: current user-facing compatibility and error behavior.
- `docs/decisions/proposed/architecture/0016-explicit-schema-migration-paths.md`: move to accepted after implementation and change its lifecycle wording to present tense.
- `ROADMAP.md`: mark the schema-compatibility slice implemented and state remaining work is the first real schema transformation.

## Task 1: Core version relation and migration catalog

**Files:**

- Modify: `crates/kb-core/src/version.rs:7-105`
- Modify: `crates/kb-core/src/error.rs:5-30`
- Modify: `crates/kb-core/src/lib.rs:50-56`
- Modify: `crates/kb-core/tests/version.rs:1-37`

**Interfaces:**

- Produces `SchemaRelation::{Current, Older, NewerMinor, NewerMajor}` for numeric comparison only.
- Produces `MigrationStep { from: SchemaVersion, to: SchemaVersion }` and `MigrationCatalog`.
- Produces `MigrationCatalog::classify(version, current) -> SchemaCompatibility`, where `OlderMigratable` requires a complete directed path and otherwise returns `OlderUnsupported`.
- Produces `ErrorCode::MigrationUnavailable` for an older Vault with no registered route.

- [ ] **Step 1: Write failing core tests for relation and path resolution**

Replace the ordering-only compatibility assertion with relation assertions, then add catalog cases in `crates/kb-core/tests/version.rs`:

```rust
#[test]
fn migration_catalog_requires_a_complete_forward_path() {
    let current = SchemaVersion::new(1, 2);
    let catalog = MigrationCatalog::new([
        MigrationStep::new(SchemaVersion::new(1, 0), SchemaVersion::new(1, 1)),
        MigrationStep::new(SchemaVersion::new(1, 1), current),
    ]).unwrap();

    assert_eq!(
        catalog.classify(SchemaVersion::new(1, 0), current),
        SchemaCompatibility::OlderMigratable,
    );
    assert_eq!(
        MigrationCatalog::empty().classify(SchemaVersion::new(1, 0), current),
        SchemaCompatibility::OlderUnsupported,
    );
}

#[test]
fn migration_catalog_rejects_non_deterministic_or_non_forward_steps() {
    let v1_0 = SchemaVersion::new(1, 0);
    let v1_1 = SchemaVersion::new(1, 1);
    assert!(MigrationCatalog::new([MigrationStep::new(v1_0, v1_0)]).is_err());
    assert!(MigrationCatalog::new([MigrationStep::new(v1_1, v1_0)]).is_err());
    assert!(MigrationCatalog::new([
        MigrationStep::new(v1_0, v1_1),
        MigrationStep::new(v1_0, SchemaVersion::new(1, 2)),
    ]).is_err());
}
```

- [ ] **Step 2: Run the core tests to prove the API is absent**

Run: `cargo test -p kb-core --test version`

Expected: compilation failure because `SchemaRelation`, `MigrationStep` and `MigrationCatalog` do not exist, and existing tests still refer to `compatibility_with`.

- [ ] **Step 3: Implement the minimal public core vocabulary**

In `version.rs`, retain canonical parsing and ordering. Replace `compatibility_with` with `relation_to`. Add the serializable compatibility value `OlderUnsupported`. Add a deterministic catalog backed by ordered `MigrationStep` values; `MigrationCatalog::new` validates self, backward and duplicate-source steps, `MigrationCatalog::empty` creates no steps, and `classify` walks at most the number of registered steps until it reaches `current` or finds no edge.

Use a dedicated catalog-validation error type rather than `KbError`; catalog construction is a typed same-process boundary. Re-export `MigrationCatalog`, `MigrationStep`, `SchemaCompatibility`, `SchemaRelation` and `SchemaVersion` from `kb-core/src/lib.rs`.

Add this error variant without changing existing serialized variant names:

```rust
pub enum ErrorCode {
    // existing variants
    MigrationRequired,
    MigrationUnavailable,
    SchemaTooNew,
    // existing variants
}
```

- [ ] **Step 4: Run focused core verification**

Run: `cargo test -p kb-core --test version`

Expected: all version parsing, relation and catalog tests pass.

- [ ] **Step 5: Commit the core contract**

```bash
git add crates/kb-core/src/version.rs crates/kb-core/src/error.rs crates/kb-core/src/lib.rs crates/kb-core/tests/version.rs
git commit -m "feat: require explicit schema migration paths"
```

## Task 2: Application-owned empty migration catalog

**Files:**

- Create: `crates/kb-app/src/schema.rs`
- Modify: `crates/kb-app/src/lib.rs:1-35`

**Interfaces:**

- Consumes `MigrationCatalog`, `SchemaCompatibility`, `SchemaVersion` and `CURRENT_SCHEMA_VERSION` from `kb-core`.
- Produces `pub(crate) fn vault_schema_compatibility(version: SchemaVersion) -> SchemaCompatibility`.
- Production behavior uses an empty catalog and classifies `v0.9` as `OlderUnsupported`.

- [ ] **Step 1: Write the failing application unit test**

Create a `#[cfg(test)] mod tests` in `crates/kb-app/src/schema.rs` with this private assertion:

```rust
#[test]
fn production_catalog_does_not_invent_legacy_migrations() {
    assert_eq!(
        vault_schema_compatibility(SchemaVersion::new(0, 9)),
        SchemaCompatibility::OlderUnsupported,
    );
    assert_eq!(
        vault_schema_compatibility(CURRENT_SCHEMA_VERSION),
        SchemaCompatibility::Current,
    );
}
```

Keep the classifier `pub(crate)` for application siblings and do not widen the public `kb-app` API merely for this test.

- [ ] **Step 2: Run the application test to prove the shared classifier is absent**

Run: `cargo test -p kb-app schema_migration`

Expected: compilation failure because `vault_schema_compatibility` does not exist.

- [ ] **Step 3: Add the application classifier**

Create `schema.rs` with a private empty catalog constructor and one classifier:

```rust
pub(crate) fn vault_schema_compatibility(version: SchemaVersion) -> SchemaCompatibility {
    MigrationCatalog::empty().classify(version, CURRENT_SCHEMA_VERSION)
}
```

Do not expose a mutable registry, configuration field or adapter-specific switch. The empty catalog is a fixed product contract until a real migration implementation is added.

- [ ] **Step 4: Run focused application verification**

Run: `cargo test -p kb-app schema_migration`

Expected: the empty-catalog and current-schema assertions pass.

- [ ] **Step 5: Commit the application classifier**

```bash
git add crates/kb-app/src/schema.rs crates/kb-app/src/lib.rs
git commit -m "feat: classify vault schemas through migration catalog"
```

## Task 3: Route every compatibility consumer through the classifier

**Files:**

- Modify: `crates/kb-app/src/status.rs:51-99`
- Modify: `crates/kb-app/src/doctor.rs:43-84`
- Modify: `crates/kb-app/src/app.rs:736-765`
- Modify: `crates/kb-app/src/config/load.rs:128-166`
- Modify: `crates/kb-app/src/schema.rs`

**Interfaces:**

- Consumes `crate::schema::vault_schema_compatibility`.
- `status` emits `older_unsupported` without interpreting current-schema configuration or admission fields.
- `doctor` emits a configuration warning that no migration path exists for the reported schema.
- `ensure_mutation_allowed` returns `MigrationUnavailable` for `OlderUnsupported` and retains `MigrationRequired` for `OlderMigratable`.
- Generic configuration loading accepts only `Current`; a future migration must use its own legacy parser.

- [ ] **Step 1: Add failing application behavior tests**

Extend the `schema.rs` unit tests using a temporary initialized Vault whose `.kb/config.yml` replaces only its first `v1.0` with `v0.9`. Assert:

```rust
let status = vault_status(&vault, &paths, &ConfigOverrides::default()).unwrap();
assert_eq!(status.schema.compatibility, SchemaCompatibility::OlderUnsupported);
assert_eq!(status.configuration, ValidationState::NotInterpreted);
assert_eq!(status.admission.enabled, None);

let error = ensure_mutation_allowed(&vault).unwrap_err();
assert_eq!(error.code, ErrorCode::MigrationUnavailable);
assert!(error.message.contains("v0.9"));

let doctor = doctor(&vault, &paths, &ConfigOverrides::default()).unwrap();
assert!(doctor.checks.iter().any(|check| {
    check.id == "configuration" && check.message.contains("no migration path")
}));
```

Also assert `load_effective_config` rejects the same old configuration rather than merging it as a current schema.

- [ ] **Step 2: Run the new application behavior test and confirm it fails**

Run: `cargo test -p kb-app schema_migration`

Expected: assertions fail because `v0.9` is still `OlderMigratable`, mutation returns `MigrationRequired`, and the generic loader accepts the old Vault layer.

- [ ] **Step 3: Implement shared-consumer integration**

Replace every direct call to `SchemaVersion::compatibility_with(CURRENT_SCHEMA_VERSION)` in `status.rs`, `doctor.rs` and `ensure_mutation_allowed` with `vault_schema_compatibility`.

Use explicit mutation branches:

```rust
SchemaCompatibility::Current => Ok(()),
SchemaCompatibility::OlderMigratable => Err(migration_required_error(identity.schema_version)),
SchemaCompatibility::OlderUnsupported => Err(KbError::new(
    ErrorCode::MigrationUnavailable,
    format!("Vault schema {} has no migration path to {}.", identity.schema_version, CURRENT_SCHEMA_VERSION),
    false,
    "Use a Knowledge-Brain version that explicitly supports this Vault schema, or preserve the Vault with an external byte-for-byte backup.",
)),
SchemaCompatibility::NewerMinorReadOnly | SchemaCompatibility::NewerMajorDiagnosticOnly => Err(schema_too_new_error(identity.schema_version)),
```

For `doctor`, distinguish the older unsupported message from a future older migratable message. For `config/load.rs`, make `load_file` accept `SchemaCompatibility::Current` only for every parsed layer; do not use current `PartialConfig` as a legacy parser.

- [ ] **Step 4: Run focused application verification**

Run: `cargo test -p kb-app schema_migration && cargo test -p kb-app --test config_layers`

Expected: old Vault diagnostics and mutation errors pass; current layered configuration behavior remains unchanged.

- [ ] **Step 5: Commit the shared application behavior**

```bash
git add crates/kb-app/src/status.rs crates/kb-app/src/doctor.rs crates/kb-app/src/app.rs crates/kb-app/src/config/load.rs crates/kb-app/src/schema.rs
git commit -m "feat: reject vault schemas without migration paths"
```

## Task 4: CLI contracts, documentation and real-entry verification

**Files:**

- Modify: `crates/kb-cli/tests/json_contract.rs:80-151`
- Modify: `crates/kb-cli/tests/doctor.rs`
- Modify: `docs/architecture/overview.md:43-46`
- Modify: `docs/reference/commands.md:90-100`
- Move: `docs/decisions/proposed/architecture/0016-explicit-schema-migration-paths.md` to `docs/decisions/accepted/architecture/0016-explicit-schema-migration-paths.md`
- Modify: `ROADMAP.md:126-155`

**Interfaces:**

- Consumes the `kb` binary and its stable JSON envelope.
- Produces real-entry evidence that `v0.9` is `older_unsupported`, mutations return `migration_unavailable`, and `v1.0` remains writable.
- Documents current behavior without advertising a `kb migrate` command.

- [ ] **Step 1: Update CLI tests before changing CLI-visible expectations**

In `json_contract.rs`, change the expected `v0.9` status compatibility and mutation error:

```rust
("v0.9", "older_unsupported"),
// ...
("v0.9", "migration_unavailable"),
```

Change `older_schema_allows_reading_while_newer_schema_is_diagnostic_only` so `config show` rejects both `v0.9` and `v1.1`; assert its JSON error for `v0.9` is `invalid_config` and assert it does not alter the config file. Add a `doctor.rs` test that invokes the actual binary and asserts the `configuration` check has `warn` status and a `no migration path` message.

- [ ] **Step 2: Run real-entry CLI tests and confirm they fail**

Run: `cargo test -p kb-cli --test json_contract && cargo test -p kb-cli --test doctor`

Expected: `v0.9` compatibility and mutation assertions fail before Task 3 changes reach the binary.

- [ ] **Step 3: Update documentation and ADR lifecycle**

Update the architecture compatibility table to distinguish “older with a registered path” from “older without a registered path.” State that the current product has no historical migration route and does not expose `kb migrate`.

Update the command reference so `status` explains `older_unsupported` and so mutation commands explain `migration_unavailable`. Keep detailed rationale in ADR-0016 rather than duplicating it.

Move ADR-0016 to the accepted architecture directory. Change its status to `accepted / implemented`, replace proposal language with present-tense decisions, replace acceptance criteria with consequences, and retain all alternatives considered.

Update Stage 5 to mark explicit schema-path classification implemented while naming executable transforms, `kb migrate`, and cross-version fixture testing as work for the first actual schema change.

- [ ] **Step 4: Run focused automated and real CLI verification**

Run:

```bash
cargo test -p kb-core --test version
cargo test -p kb-app schema_migration
cargo test -p kb-app --test config_layers
cargo test -p kb-cli --test json_contract
cargo test -p kb-cli --test doctor
```

Then perform a real binary check in a new temporary Vault:

```bash
kb init /tmp/kb-schema-check --json
# replace only schema_version in /tmp/kb-schema-check/.kb/config.yml with v0.9
kb status --vault /tmp/kb-schema-check --json
kb config set search.mode bm25 --vault /tmp/kb-schema-check --yes --json
kb doctor --vault /tmp/kb-schema-check --json
```

Verify externally that status returns `older_unsupported`, config mutation returns `migration_unavailable`, doctor mentions no migration path, and `.kb/config.yml` remains byte-identical after the rejected mutation. Recreate a clean `v1.0` temporary Vault and run `status`, `query`, and a harmless `config set` to prove current behavior remains writable.

- [ ] **Step 5: Commit the adapter contracts and documentation**

```bash
git mv docs/decisions/proposed/architecture/0016-explicit-schema-migration-paths.md docs/decisions/accepted/architecture/0016-explicit-schema-migration-paths.md
git add crates/kb-cli/tests/json_contract.rs crates/kb-cli/tests/doctor.rs docs/architecture/overview.md docs/reference/commands.md ROADMAP.md
git commit -m "docs: clarify schema migration availability"
```

## Plan Self-Review

- Spec coverage: Tasks 1–3 implement separation, catalog ownership, empty production catalog, classification and stable errors. Task 4 covers adapters, documentation, ADR lifecycle and real-entry validation. No executable transformation or migration command is planned because the spec explicitly excludes both.
- Placeholder scan: the plan names every file, public interface, test command and expected behavior; no deferred implementation markers remain.
- Type consistency: `MigrationCatalog`, `MigrationStep`, `SchemaRelation`, `SchemaCompatibility::OlderUnsupported`, `vault_schema_compatibility`, `MigrationUnavailable` and `migration_unavailable` use the same names across all tasks.
