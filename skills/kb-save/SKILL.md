---
name: kb-save
description: Save structured research or article knowledge when the user asks to create, update, supersede, or organize maintained Wiki content. Do not use for ordinary notes, source ingestion, query, or maintenance.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tools.
---

# Knowledge saving

## Prepare

Resolve the Vault, read `KB.md`, and construct the structured request described in [references/request-format.md](references/request-format.md). Read that reference only when preparing a save.

Run MCP `kb_knowledge_save` or `kb knowledge save <request.json> --vault <path-or-id> --json`. Check target paths, source URIs, `before_sha256`, managed index/log changes, conflicts, and `change_summary`. Keep the confirmation token internal.

## Save and verify

After one explicit confirmation of the displayed changes, submit only the matching token through MCP or `kb knowledge save --confirm <token> --vault <path-or-id> --json`. A stale confirmation requires a new preview.

Re-read or query the saved page and run `kb lint`. Do not write Wiki files, index, or log directly. Use `kb plan create`, operation IDs, and `kb apply` only for an explicitly requested delayed workflow.
