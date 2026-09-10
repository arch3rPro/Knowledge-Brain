---
name: kb-save
description: Save structured research or article knowledge only when the user explicitly asks to write or organize maintained Wiki content. Do not use for ordinary notes, research, source ingestion, query, or maintenance.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tools.
---

# Knowledge saving

## Prepare

First distinguish the request from ordinary content creation. “Research”, “organize notes”, “write a report”, or “update a note” does not authorize a managed Wiki save. Continue only when the user explicitly requests Wiki content or organizing material into the knowledge base.

Resolve the Vault, read `KB.md`, and construct the structured request described in [references/request-format.md](references/request-format.md). Read that reference only when preparing a save.

Run MCP `kb_knowledge_save` or `kb knowledge save <request.json> --vault <path-or-id> --json`. Check target paths, source URIs, `before_sha256`, managed index/log changes, conflicts, and `change_summary`. Keep the confirmation token internal.

## Save and verify

After one explicit confirmation of the displayed changes, submit only the matching token through MCP or `kb knowledge save --confirm <token> --vault <path-or-id> --json`. `--yes` is valid only when the user has already explicitly authorized this exact Wiki save; it never supplies that authorization. A stale confirmation requires a new preview.

Re-read or query the saved page and run `kb lint`. Do not write Wiki files, index, or log directly. Use `kb plan create`, operation IDs, and `kb apply` only for an explicitly requested delayed workflow.
