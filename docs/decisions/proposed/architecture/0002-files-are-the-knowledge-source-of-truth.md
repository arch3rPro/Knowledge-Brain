# ADR-0002: Files are the knowledge source of truth

- Status: proposed
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

A knowledge system can become unreadable or unrecoverable when its durable meaning exists only in a proprietary database or index. Users must be able to inspect, edit, copy, and recover their knowledge with ordinary file tools.

## Proposal

Store portable knowledge and configuration in Markdown, YAML, and JSON. Treat search indexes, extraction outputs, progress data, and machine logs as derived state that can be deleted and rebuilt. Keep original source bytes as content-addressed files inside the Vault.

## Alternatives considered

**SQLite as the primary store.** SQLite provides reliable queries and updates, but ordinary Markdown would become a projection that is insufficient for recovery.

**A vector database as the primary store.** Vector retrieval can improve semantic recall, but it introduces model-derived state, opaque reconstruction requirements, and a dependency unrelated to basic knowledge ownership.

## Acceptance criteria

- Deleting `.kb/cache/` does not remove knowledge, provenance, configuration, or direct-read capability.
- A copied Vault can be opened using only its portable files and the `kb` executable.

## Risks

- File-backed queries and multi-file updates require more explicit indexing and recovery logic.
- Human edits can violate schemas and must be diagnosed without silently replacing user content.
