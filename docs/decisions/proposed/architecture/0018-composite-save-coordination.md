# ADR-0018: Hide operation planning behind one user confirmation

- Status: accepted design
- Class: architecture
- Date: 2026-09-08

## Problem

A person asking an Agent to research and save material wants to approve the final changes once. The existing review, plan create and apply primitives expose durable planned operations directly, so an Agent can accidentally make a person manage operation IDs, internal staging and multiple confirmations. The durable plan still protects exact content, stale writes, recovery and auditability, so removing it entirely would weaken the write path.

## Proposal

Knowledge-Brain keeps durable plans as an internal execution mechanism and removes them from the standard user-facing workflow.

A coordinated source save or knowledge save begins with preparation. Preparation validates the existing inputs, creates the existing operation record, and returns a user-facing change_summary plus a machine-only confirmation_token. The token identifies that exact prepared operation; current implementations may use its operation ID, but user-facing copy and Agent replies must not expose or require it.

The Agent or UI shows the user only the combined change summary and requests one final confirmation. After confirmation, it passes the token to the matching coordinated save entry. That entry applies the already prepared operation for the selected Vault through the existing stale-plan, recovery and locking path. A source preparation with no changes returns unchanged and has no token.

A direct CLI request with --yes, or an MCP/HTTP request with apply: true, remains available when the caller has already obtained authorization before invoking Knowledge-Brain. It prepares and applies in one request. It is not the ordinary follow-up after a preview; the ordinary follow-up uses the confirmation token and applies the exact previewed operation.

The primitive review, plan create, operation show and apply commands remain stable advanced interfaces for scripts, delayed work, debugging and manual review. They preserve their current operation-oriented contracts.

The one-confirmation boundary applies to the writes covered by a user request. An Agent must not write a new raw source file directly in an admitted Vault directory before that confirmation and claim that source capture authorized it retroactively. Staging agent-authored raw material outside the Vault, or adding a first-class source-authoring batch request, is outside this decision.

## Alternatives considered

**Make preview non-persistent and recalculate during --yes.** This makes the ordinary workflow shorter, but the user can approve one version and write a later changed version. It also discards the existing recovery and audit record until after writes begin.

**Expose a plan and operation ID to every user.** This retains strict validation but turns internal storage mechanics into a required user concept and lengthens normal Agent conversations.

**Always apply immediately after preparation.** This bypasses the final confirmation boundary for an Agent/UI workflow.

**Delete the primitive operations.** Existing scripts and advanced review workflows need durable, inspectable and retryable operations. A convenience layer must not remove them.

## Acceptance criteria

- A normal Agent/UI workflow presents one final confirmation containing a human-readable summary, not plan creation or operation IDs.
- Confirmation applies the exact prepared operation, preserving stale-plan and recovery checks.
- Source and knowledge preparation produce no user-visible persistence claim beyond a change summary and a machine-only token.
- --yes and apply: true create and apply in one request only when explicit authorization already exists.
- Existing primitive command names, operation formats and operation kinds remain compatible.
- A no-change source save completes without a durable planned operation.
- MCP and HTTP retain their fixed-Vault, write-enable and HTTP authentication protections.

## Risks

- A machine-only token is still persisted state. The boundary is user experience, not elimination of durable recovery data.
- A single user confirmation can authorize multiple prepared writes only when the Agent presents their combined summaries and sends each matching token; those writes are not a new cross-operation atomicity guarantee.
- Agent-authored raw source files require explicit staging discipline until a first-class source-authoring request exists.
