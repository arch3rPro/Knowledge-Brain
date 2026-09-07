# ADR-0007: Reviewed plans and all-or-restore writes

- Status: accepted / implemented
- Class: architecture
- Spec: [Knowledge save](../../../superpowers/specs/2026-09-07-knowledge-save.md)

## Problem

Agent suggestions can overwrite human edits, and a process can stop after replacing only some files. Knowledge changes need an inspectable approval boundary and a recovery guarantee that does not depend on Git.

## Decision

Represent every knowledge save as a structured plan containing exact source versions, original target hashes, configuration and admission context, complete target content, compatibility data, and a unique operation ID. Creating a plan does not change the Vault. Applying an explicitly selected ID revalidates every input under a Vault write lock.

Before exposing each replacement, persist its original bytes and expected new hash. A durable result receipt marks the complete new state. Without that receipt, retry restores operation-owned writes before revalidating and executing the plan again. Recovery preserves any file that differs from both the recorded old and new states.

## Alternatives considered

**Direct Agent writes.** Direct access cannot enforce stale-input checks, shared permissions, or consistent recovery across CLI and future application hosts.

**Git-only rollback.** Vaults are not required to use Git, and repository history does not coordinate concurrent CLI, MCP, HTTP, WebUI, or GUI operations.

**Save each file independently.** Independent success can expose a mixed knowledge state and append logs for changes that were not fully stored.

**Keep a database transaction as the knowledge authority.** This makes recovery depend on an opaque store and conflicts with Markdown files remaining the portable source of truth.

## Consequences

Changed inputs reject the whole plan before its first target write. Repeating a completed operation ID returns its stored result without duplicate effects. Controlled interruption at every replacement boundary can recover to the complete old or new state, while unrelated edits remain untouched.

Plans contain complete content and original bytes, so user-state storage can be larger than the final change. Expiration and configured count/byte limits bound that cost. Recovery journals must stay with machine-local state until the operation finishes; deleting them during a pending save can make automatic recovery impossible.
