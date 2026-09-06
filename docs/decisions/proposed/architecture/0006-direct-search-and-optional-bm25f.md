# ADR-0006: Direct search and optional BM25F

- Status: proposed
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Small Vaults need reliable search without index administration, while larger Vaults need ranked full-text retrieval. Neither use case should make generated index data part of the knowledge truth.

## Proposal

Search actual Markdown in direct mode by default. Maintain a rebuildable lightweight catalog for navigation. Offer a complete, explicitly selected BM25F backend with field weighting, heading-section chunks, CJK 1–3 grams, incremental updates, versioned hashes, explanations, and direct-search fallback.

## Alternatives considered

**File names and grep only.** These are dependable primitives but do not provide structured result blocks or useful field-aware ranking.

**Vector retrieval by default.** This requires embedding models and derived data before basic search can work.

**Automatic switching by document count.** Hidden mode changes make identical commands behave differently as a Vault grows.

## Acceptance criteria

- Direct search works when every cache is absent.
- BM25 mode is user-selected, explainable, incrementally maintainable, and safely falls back unless strict mode is requested.

## Risks

- Maintaining two backends expands the retrieval test matrix.
- CJK n-grams increase index size and require careful relevance tuning.
