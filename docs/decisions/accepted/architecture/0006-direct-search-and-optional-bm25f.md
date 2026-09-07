# ADR-0006: Direct search and optional BM25F

- Status: accepted
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Small Vaults need reliable search without index administration, while larger Vaults need ranked full-text retrieval. Neither use case should make generated index data part of the knowledge truth.

## Decision

Search actual Markdown in direct mode by default. Maintain a rebuildable lightweight catalog for navigation. The explicitly selected BM25F backend uses field weighting, heading-section chunks, CJK 1–3 grams, incremental updates, versioned content fingerprints, explanations, and direct-search fallback. A strict request policy turns fallback into a stable error when callers require that backend.

## Alternatives considered

**File names and grep only.** These are dependable primitives but do not provide structured result blocks or useful field-aware ranking.

**Vector retrieval by default.** This requires embedding models and derived data before basic search can work.

**Automatic switching by document count.** Hidden mode changes make identical commands behave differently as a Vault grows.

## Consequences

- Direct search works when every cache is absent.
- BM25 mode remains user-selected, explainable and incrementally maintainable.
- The index is disposable and validated against actual paths and content before use.
- Missing, damaged, incompatible or stale indexes fall back as one request unless strict mode is requested.
- Maintaining two backends expands the retrieval test matrix.
- CJK n-grams increase index size and relevance tuning remains a versioned implementation concern.
