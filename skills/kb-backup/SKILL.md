---
name: kb-backup
description: Create, verify, or restore a portable Knowledge-Brain backup when the user explicitly asks about backup archives, integrity, recovery, or migration. Do not treat caches or Git as a Knowledge-Brain backup.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH.
---

# Vault backups

## Choose the action

Resolve the Vault and read `KB.md` before creating a backup. No backup MCP tools are currently exposed.

- Verify: `kb backup verify <archive.zip> --json` before relying on an archive.
- Create: after approval of Vault and new output path, run `kb backup create --output <archive.zip> --vault <path-or-id> --json`; never overwrite an existing archive.
- Restore: after approval of archive and target, run `kb backup restore <archive.zip> --target <target> --json`; the target must be nonexistent or genuinely empty.

Never unpack or copy managed objects manually. After restore, run `kb status`, `kb doctor`, and a representative query against the restored Vault. Report archive path, manifest verification, restored target, and any unverified platform behavior.
