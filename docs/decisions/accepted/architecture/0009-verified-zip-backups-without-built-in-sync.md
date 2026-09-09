# ADR-0009: Verified ZIP backups without built-in sync

- Status: accepted
- Class: architecture
- Spec: [Knowledge-Brain design](../../../../.superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Users need a portable backup whose completeness and bytes can be checked on another operating system. Building synchronization into Knowledge-Brain would introduce conflict resolution, accounts, providers, and network behavior beyond local knowledge management.

## Decision

Create standard ZIP backups with a manifest containing Vault identity, versions, paths, sizes, and SHA-256 values. Restore only into a nonexistent or empty target after validating all entries in a private sibling staging directory. Compact backups may omit immutable source objects but must report incomplete source evidence. Keep Git and directory synchronization services as explicit external choices.

## Alternatives considered

**Recommend ordinary directory copies only.** Copies are useful but provide no common manifest for detecting omissions, corruption, or excluded local state.

**Provide a proprietary cloud synchronization service.** This would require accounts, hosted infrastructure, conflict policy, security operations, and continuous networking.

## Consequences

- Every restored byte matches the backup manifest, and unsafe or undeclared archive entries are rejected before publication.
- Backup and restore do not require or configure a synchronization provider or machine-local Vault registration.
- A cross-operating-system restoration claim still requires native evidence on those systems.
- ZIP does not solve concurrent multi-device edits or merge conflicts.
- Source objects can make complete backups large; omitting them produces an explicitly incomplete-evidence archive.
