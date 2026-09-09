---
name: kb-config
description: Inspect or change Knowledge-Brain configuration when the user asks about effective settings, validation, admission directories, or enabling and disabling sources. Do not use for source saving or general Vault maintenance.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH; MCP is optional.
---

# Configuration and admission

## Inspect

Resolve the Vault, read `KB.md`, then use `kb config show --sources`, `kb config get`, `kb config validate`, or `kb config admission list` with `--vault <path-or-id> --json`.

`admission.yml` is a source access list. Adding a directory does not save its files, and removing one does not delete it.

## Change

1. Preview `kb config set|unset` or `kb config admission add|enable|disable|remove` without `--yes`.
2. Show the actual diff and target layer.
3. After one explicit confirmation, repeat the same command with `--yes --json`.
4. Run `kb config validate` and re-read the changed value or admission list.

Do not hand-edit managed configuration to bypass validation. If the preview has drifted, prepare a new preview instead of reusing approval.
