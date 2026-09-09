---
name: kb-ops
description: Inspect Knowledge-Brain operational state when the user asks for maintenance, health checks, diagnostics, lint, source-change status, recovery, or cache rebuilding. Read-only maintenance must not become ingestion or Wiki generation.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH or the equivalent fixed-Vault MCP tools.
---

# Vault operations

## Read-only maintenance

Resolve the Vault, read `KB.md`, then use `kb maintain --vault <path-or-id> --json` or MCP `kb_maintenance`. This one report contains status, admitted source changes, lint, and doctor results without creating an operation plan.

Summarize only returned facts. Distinguish added, modified, deleted, skipped, and unchanged sources; do not call every source change “待入库”. Report lint findings and diagnostic statuses without inventing link counts, an `ok` field, or a health score. Empty or unavailable fields can be omitted.

Maintenance ends with the report. Never call source or knowledge save, apply, repair, or cache rebuild as an implied next step.

## Explicit follow-up

Use `kb operation show`, `kb source verify`, or `kb cache rebuild` only when the user separately requests that action. Cache rebuilding changes disposable derived state; it does not create knowledge and is not a backup.
