# ADR-0018: Coordinate source and knowledge saves without replacing operations

- Status: proposed
- Class: architecture
- Date: 2026-09-08

## Problem

Saving admitted source material and saving curated knowledge each currently require callers to compose planning, operation inspection and apply steps. The primitives are safe and useful, but a person or Agent performing an ordinary save must remember several commands and translate the same workflow separately for CLI, MCP, HTTP and a future UI. A new short entry point must not turn planning into implicit authorization or create a second persistence model.

## Proposal

`kb-app` adds two coordinated requests: source save and knowledge save. Each request first invokes the existing source-review or knowledge-plan behavior and keeps its existing operation record, validation, lock acquisition, stale-plan checks and apply implementation.

The coordinated request has an explicit execution flag. Without it, the request returns a preview and creates a normal planned operation when work exists. With it, the request creates the same plan and then applies that exact operation for the selected Vault. The flag is an explicit write authorization; it never simulates an interactive confirmation. A no-change source review returns an unchanged response and performs no write.

CLI exposes the requests as `kb source save [--yes]` and `kb knowledge save <REQUEST.json> [--yes]`. MCP exposes `kb_source_save` and `kb_knowledge_save` with `apply: false` by default. HTTP exposes `POST /source/save` and `POST /knowledge/save`, also with `apply: false` by default. MCP and HTTP accept execution only when their existing write capability has been explicitly enabled; their fixed-Vault and HTTP authentication rules remain in force.

The new response is a separate, additive composite contract. It contains `phase` (`planned`, `applied`, or `unchanged`), the unmodified planning `preview`, the current `operation_summary` or `null`, and `result` or `null`. Existing `review`, `plan create`, `operation show` and `apply` contracts remain available and unchanged. If coordinated apply fails after a plan is persisted, the error retains its existing code and includes the created operation ID in structured details so callers can inspect or retry the plan.

## Alternatives considered

**Replace `review`, `plan create` and `apply` with one save command.** This would remove useful composable primitives, break existing clients and make recovery or inspection less direct.

**Use top-level `kb ingest` and `kb save`.** These names make it unclear whether a source capture, curated Wiki change, plan, or direct write will occur. Domain-qualified names preserve the distinction users already make between source material and knowledge.

**Leave composition to Agent Skills.** Skills can guide a conversational workflow but do not shorten the CLI, MCP, HTTP or future UI integration. They would duplicate coordination logic across adapters.

**Make every composite request apply automatically.** Automatic application would collapse preview and authorization, bypassing the single explicit confirmation boundary.

## Acceptance criteria

- CLI, MCP, HTTP and a future UI can invoke the same application-level coordinator.
- Default coordinated requests only prepare previews and preserve all existing stale-plan protections.
- Explicit execution applies only the operation created by that invocation and only for the selected fixed Vault.
- Existing primitive commands, operation formats and operation kinds remain compatible.
- Coordinated responses distinguish planned, applied and unchanged outcomes without hiding the planning preview.
- MCP and HTTP deny execution unless their existing write controls are enabled; HTTP preserves token requirements for non-loopback listeners.
- Focused tests exercise CLI, MCP and HTTP previews, authorized execution, no-change source saves, stale-plan failure and selected-Vault isolation.

## Risks

- A coordinated apply still releases the planning lock before the existing apply path reacquires its exclusive lock, so source or Wiki changes can make a new plan stale. This is intentional: preserving stale-plan validation is safer than holding a broad lock across the whole workflow.
- New composite response fields are a new public contract and require adapter parity tests.
- Adding convenience commands can obscure the primitive workflow unless CLI help, Skills and reference material state that preview is the default and `--yes` is the only direct-write form.
