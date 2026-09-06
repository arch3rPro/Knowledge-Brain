# ADR-0007: Reviewed plans and all-or-restore writes

- Status: proposed
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Agent suggestions can overwrite human edits, and a process can stop after replacing only some files. Knowledge changes need an inspectable approval boundary and a recovery guarantee that does not depend on Git.

## Proposal

Represent each change as a structured plan containing source versions, original target hashes, configuration context, exact target content, compatibility data, and a unique operation ID. Apply a plan only after explicit selection of that ID. Revalidate every input under a Vault write lock and ensure interrupted multi-file writes resolve to the complete old state or complete new state.

## Alternatives considered

**Direct Agent writes.** Direct access cannot enforce stale-input checks, common permissions, or consistent recovery across hosts.

**Git-only rollback.** Vaults are not required to use Git, and repository history does not by itself coordinate concurrent CLI, MCP, and HTTP operations.

**Save each file independently.** Independent success can expose a mixed knowledge state and append logs for changes that were not fully stored.

## Acceptance criteria

- Any changed plan input rejects the operation before a target write.
- Forced interruption at every replacement boundary recovers to the complete old or new state.
- Repeating a completed operation ID returns its stored result without duplicate effects.

## Risks

- Recovery journals and cross-platform locks add implementation complexity.
- Large plans require storage limits and expiration rules.
