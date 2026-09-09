---
name: kb-connect
description: Connect Knowledge-Brain to an Agent host when the user asks to install or remove Skills, start MCP, start HTTP, expose a remote service, or inspect integration status. Do not trigger for normal Vault content work.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH; host support varies and must be detected.
---

# Agent connections

## Skills

1. Inspect with `kb skills detect --vault <path-or-id> --json` and `kb skills status --host <host> --scope vault|user --vault <path-or-id> --json`.
2. Preview `kb skills install|uninstall`; state the host, scope, mode, target paths, and conflicts.
3. Apply only after one explicit confirmation, then repeat `status` and verify discovery from the actual Agent host when available.

Never write host directories or bridge files directly. An external `npx skills add` installation is not owned by `kb skills`.

## MCP and HTTP

Start fixed-Vault MCP with `kb mcp --vault <path-or-id>`; writes additionally require `--allow-write` and approval of the specific change. Start HTTP with `kb serve --vault <path-or-id>`; non-loopback binding requires authentication. Do not expose a network listener, enable writes, or change the selected Vault without explicit approval.
