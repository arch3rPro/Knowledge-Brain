# ADR-0017: Share Agent experience semantics and distribute task-scoped Skills

- Status: proposed
- Class: architecture
- Date: 2026-09-08

## Problem

CLI, MCP, HTTP and future Agent hosts expose the same Vault operations through different surfaces. Without shared user-visible facts, each surface can truncate identifiers differently, request authorization at different times, or reinterpret operation state. A single broad Skill also mixes unrelated tasks and collides conceptually with the `kb` CLI name, while unmanaged and managed Skill installers can overwrite each other's files.

## Proposal

`kb-app` remains the source of operation, query, diagnostic and error facts. Its adapter responses add an operation summary without changing existing root fields. Human renderers use 12-character Hash displays while machine protocols retain canonical identities. A shared `match_mode` separates relevant search from exact literal search, and shared diagnostics/error codes distinguish Vault state from machine runtime state and backup verification from restore failure.

Knowledge-Brain distributes eight task-scoped `kb-*` Skills instead of a visible root Skill. `kb skills install` owns its recorded suite, bridge blocks and migration from an unmodified legacy Skill. `npx skills add` distributes the same Skill content through an external installer; it owns its installation files. Neither installer silently overwrites or removes the other's files.

## Alternatives considered

**Keep presentation and confirmation behavior inside each adapter.** This leaves CLI, MCP, HTTP and future UI clients free to diverge on the same operation facts and makes compatibility testing multiply by adapter.

**Add a single interactive command or CLI wizard.** A wizard hides composable primitives, does not help MCP or HTTP callers, and introduces a second interaction model before the existing operation model is exhausted.

**Keep one `knowledge-brain` Skill.** A broad Skill makes task boundaries unclear, is harder for an Agent to select, and does not provide concise entry points for initialization, query, ingestion, backup and connection work.

**Use only `npx skills add`.** This cannot manage Vault-specific bridge blocks, operation review, migration or safe uninstall for the built-in `kb` workflow.

**Let both installers write the same directories.** Shared ownership makes it impossible to determine whether overwrite or uninstall is safe after either installer changes a file.

## Acceptance criteria

- CLI, MCP and HTTP consume shared operation summaries, query intent, doctor facts and backup error categories.
- Human text output uses short Hashes without changing machine-readable canonical identities.
- Every task-scoped Skill preserves the same plan and confirmation rules.
- Both installers can be exercised in isolated directories without one deleting or overwriting files owned by the other.
- Legacy or manually modified Skill installations are preserved unless an explicit reviewed action proves they are safe to change.

## Risks

- Additive response fields and a new error code require clients to tolerate new values and fields.
- Eight Skills increase packaging and synchronization work; a canonical content source and asset checks are required.
- Exact matching may be slower than indexed relevant search because it intentionally bypasses BM25F.
- Host Skill conventions vary, so native host loading behavior requires platform- and host-specific verification.
