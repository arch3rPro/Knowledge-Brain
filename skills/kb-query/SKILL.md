---
name: kb-query
description: Search and cite a Knowledge-Brain Vault when the user asks to find, recall, compare, or verify stored Wiki knowledge or saved source evidence. Do not use for unsaved files or web research.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tool.
---

# Vault queries

## Search

Resolve the Vault, read `KB.md`, and call MCP `kb_query` or `kb query <terms> --scope wiki|sources|all --limit <1..100> --vault <path-or-id> --json`.

- Use normal relevant matching for discovery.
- Use `--exact` only for case-sensitive literal verification.
- `wiki` searches maintained knowledge; `sources` searches saved evidence; `all` returns both groups.
- Empty results are a valid result. Narrow or revise the query; do not ingest files or search the web unless separately requested.

Preserve returned paths, headings or locations, and source URIs when citing. If a snippet is insufficient, read the referenced file within the Vault boundary. This Skill is read-only and never creates plans, rebuilds caches, or writes files.
