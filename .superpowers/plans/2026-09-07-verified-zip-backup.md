# Verified ZIP Backup Implementation Plan

**Goal:** Implement standard ZIP create, verify and empty-target restore through the shared application layer.

**Architecture:** `kb-core` owns serializable manifest/request/report contracts. `kb-app` owns safe collection, streaming ZIP IO, hostile-archive validation and staged restore. `kb-cli` owns only command argument translation and rendering.

**Spec:** `.superpowers/specs/2026-09-07-verified-zip-backup.md`

**Verification constraint:** Run focused tests only; do not run the workspace-wide suite.

### Task 1: Manifest and request contracts

- [x] Add versioned manifest entries and create/verify/restore reports.
- [x] Define stable validation for sorted unique portable paths, sizes and hashes.
- [x] Cover serialization and malformed contracts with focused core tests.

### Task 2: Backup collection and creation

- [x] Write failing tests for exact include/exclude scope, disabled and empty directories, compact source evidence, collisions, links and output boundaries.
- [x] Collect only the specified Vault roots without applying source include/exclude globs.
- [x] Stream a temporary standard ZIP, detect changed input and publish without replacing an existing file.

### Task 3: Hostile archive verification and staged restore

- [x] Write failing tests for valid archives, changed bytes, extra/traversal/link entries, collisions and invalid targets.
- [x] Validate metadata, required/missing/duplicate entries and stream every digest before reporting success.
- [x] Extract into a private sibling staging directory and publish only after complete revalidation.

### Task 4: Shared application and CLI journey

- [x] Add backup requests to `AppRequest` and preserve Vault lock/recovery boundaries.
- [x] Add `kb backup create|verify|restore` and a real CLI JSON journey.
- [x] Verify restore after moving the archive and without machine registration.

### Task 5: Documentation and focused verification

- [x] Publish backup reference, command/README links and Stage 5B status.
- [x] Move ADR 0009 to accepted with consequences and alternatives.
- [x] Run format, related crate Clippy, focused core/app/CLI tests and diff checks.
- [x] Record unverified platform/release/full-suite limits explicitly.
