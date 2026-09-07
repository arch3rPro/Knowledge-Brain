# Verified ZIP Backup Implementation Plan

**Goal:** Implement standard ZIP create, verify and empty-target restore through the shared application layer.

**Architecture:** `kb-core` owns serializable manifest/request/report contracts. `kb-app` owns safe collection, streaming ZIP IO, hostile-archive validation and staged restore. `kb-cli` owns only command argument translation and rendering.

**Spec:** `docs/superpowers/specs/2026-09-07-verified-zip-backup.md`

**Verification constraint:** Run focused tests only; do not run the workspace-wide suite.

### Task 1: Manifest and request contracts

- [ ] Add versioned manifest entries and create/verify/restore reports.
- [ ] Define stable validation for sorted unique portable paths, sizes and hashes.
- [ ] Cover serialization and malformed contracts with focused core tests.

### Task 2: Backup collection and creation

- [ ] Write failing tests for exact include/exclude scope, disabled and empty directories, compact source evidence, collisions, links, output collision and file changes.
- [ ] Collect only the specified Vault roots without applying source include/exclude globs.
- [ ] Stream a temporary standard ZIP and publish it without replacing an existing file.

### Task 3: Hostile archive verification and staged restore

- [ ] Write failing tests for valid archives, changed bytes, missing/extra/duplicate entries, traversal, links, collisions and invalid targets.
- [ ] Validate metadata and stream every digest before reporting success.
- [ ] Extract into a private sibling staging directory and publish only after complete revalidation.

### Task 4: Shared application and CLI journey

- [ ] Add backup requests to `AppRequest` and preserve Vault lock/recovery boundaries.
- [ ] Add `kb backup create|verify|restore` and a real CLI JSON journey.
- [ ] Verify restore after moving the archive and without machine registration.

### Task 5: Documentation and focused verification

- [ ] Publish backup reference, command/README links and Stage 5B status.
- [ ] Move ADR 0009 to accepted with consequences and alternatives.
- [ ] Run format, related crate Clippy, focused core/app/CLI tests and diff checks.
- [ ] Record unverified platform/release/full-suite limits explicitly.
