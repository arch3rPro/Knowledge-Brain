---
name: kb-ingest
description: Save or verify admitted source evidence when the user asks to ingest, capture, update, or check source versions. Do not trigger for writing a note, changing admission, querying existing evidence, or read-only maintenance.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tools.
---

# Source ingestion

## Prepare

Resolve the Vault, read `KB.md`, and use `kb source save --vault <path-or-id> --json` or MCP `kb_source_save`. Only enabled `admission.yml` directories are in scope.

- No changes: report that result; do not request confirmation.
- Changes: summarize the returned added, modified, deleted, skipped, and possible-move entries. Keep the confirmation token internal.
- Invalid or over-limit source: report the exact rejected path and reason; do not read around the admission boundary.

## Save and verify

After one explicit confirmation of the displayed changes, submit the same token with `kb source save --confirm <token> --vault <path-or-id> --json` or MCP `kb_source_save`. Then run `kb source verify` and an exact source query for a distinctive term.

Do not alter source files, records, or objects. `kb review`, operation IDs, and `kb apply` are only for an explicitly requested delayed workflow.
