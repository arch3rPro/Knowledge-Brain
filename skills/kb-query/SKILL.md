---
name: kb-query
description: Search and cite a Knowledge-Brain Vault when the user asks to find, recall, compare, or verify stored Wiki knowledge or saved source evidence. Do not use for unsaved files or web research.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tool.
---

# Vault queries

## Search

Resolve the Vault and choose one access mode:

- Local CLI: read `KB.md`, then call `kb query <terms> --scope wiki|sources|all --limit <1..100> --vault <path-or-id> --json`.
- Remote MCP: call `kb_status`, use `kb_read` on its `rules_uri`, then call `kb_query`.

- Use normal relevant matching for discovery.
- Use `--exact` only for case-sensitive literal verification.
- `wiki` searches maintained knowledge; `sources` searches saved evidence; `all` returns both groups.
- Empty results are a valid result. Narrow or revise the query; do not ingest files or search the web unless separately requested.

Search results are candidates, not complete evidence. Select useful results and read each returned `resource_uri` completely with `kb read <RESOURCE_URI>` or MCP `kb_read`; continue with `next_cursor` until `complete=true`. Preserve the resource URI, heading or source location when citing.

In remote MCP mode, `path`, `content_path`, and `path_scope=server_vault` refer to the MCP server's Vault. Never resolve them against the Agent's local working directory. Use only `resource_uri` for follow-up reads.

This Skill is read-only and never creates plans, rebuilds caches, saves sources, or writes files. See [references/query.md](references/query.md) for the full local and remote sequence.
