---
name: kb-ingest
description: Save or verify admitted source evidence only when the user explicitly asks to ingest material or save source versions. Do not trigger for research, writing a note, changing admission, querying evidence, or read-only maintenance.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tools.
---

# Source ingestion

## Prepare

First distinguish the request from ordinary content creation. “Research”, “organize notes”, “write a report”, or “update a note” does not authorize source saving, even when the resulting file is in an admitted directory. Continue only when the user explicitly requests source ingestion, source capture, or saving a source version.

Resolve the Vault, read `KB.md`, and use `kb source save --vault <path-or-id> --json` or MCP `kb_source_save`. Only enabled `admission.yml` directories are in scope.

- No changes: report that result; do not request confirmation.
- Changes: summarize the returned added, modified, deleted, skipped, and possible-move entries. Keep the confirmation token internal.
- Invalid or over-limit source: report the exact rejected path and reason; do not read around the admission boundary.

## Save and verify

After one explicit confirmation of the displayed changes, submit the same token with `kb source save --confirm <token> --vault <path-or-id> --json` or MCP `kb_source_save`. `--yes` is valid only when the user has already explicitly authorized this exact ingestion; it never supplies that authorization. Then run `kb source verify` and an exact source query for a distinctive term.

Do not alter source files, records, or objects. `kb review`, operation IDs, and `kb apply` are only for an explicitly requested delayed workflow.
