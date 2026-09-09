# ADR-0002: Files are the knowledge source of truth

- Status: accepted
- Class: architecture

## Problem

A knowledge system can become unreadable or unrecoverable when its durable meaning exists only in a proprietary database or index. Users must be able to inspect, edit, copy, and recover their knowledge with ordinary file tools.

## Decision

Store portable knowledge and configuration in Markdown, YAML, and JSON. Treat search indexes and extraction outputs as derived state that can be deleted and rebuilt. Keep original source bytes as content-addressed files inside the Vault. Unfinished operation journals and recovery markers are machine-local recovery dependencies, not disposable caches; finish recovery before discarding or moving them.

## Alternatives considered

**SQLite as the primary store.** SQLite provides reliable queries and updates, but ordinary Markdown would become a projection that is insufficient for recovery.

**A vector database as the primary store.** Vector retrieval can improve semantic recall, but it introduces model-derived state, opaque reconstruction requirements, and a dependency unrelated to basic knowledge ownership.

## Acceptance criteria

- Deleting `.kb/cache/` does not remove knowledge, provenance, configuration, or direct-read capability.
- A copied Vault can be opened using only its portable files and the `kb` executable.

## Risks

- File-backed queries and multi-file updates require more explicit indexing and recovery logic.
- Human edits can violate schemas and must be diagnosed without silently replacing user content.
