# ADR-0003: Admission list

- Status: accepted
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Knowledge-Brain needs a human-readable declaration of which user directories may be inspected. Combining this authorization with a processing queue or scanning the whole Vault would obscure the user's intended read boundary.

## Decision

Use root-level `admission.yml` as the sole source of admitted top-level directories. Each entry has a stable ID, a one-component relative path, an enabled state, and optional include/exclude filters. Runtime progress and discovered changes remain outside this file.

## Alternatives considered

**`pending.yml`.** The name implies work items waiting for processing rather than a durable authorization boundary.

**A `triage` queue.** Triage describes prioritization and classification, not which directories the application is allowed to inspect.

**Scan the whole Vault.** This expands the read scope without an explicit user decision and mixes framework-owned paths with personal topic directories.

## Acceptance criteria

- Review behavior reads only enabled admission entries and applies their filters.
- No other configuration layer can add or enable admitted directories.
- Nested, absolute, linked, junction, `Wiki`, and `.kb` paths are rejected.

## Risks

- Users must maintain the declaration when topic directories are added or renamed.
- Restricting entries to one top-level component prevents admitting an isolated nested directory directly.
