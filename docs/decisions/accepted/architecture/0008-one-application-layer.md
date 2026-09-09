# ADR-0008: One application layer

- Status: accepted / implemented
- Class: architecture
- Spec: [Knowledge-Brain design](../../../../.superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

CLI, MCP, HTTP, WebUI, and GUI entry points can drift when each owns filesystem, permission, or business behavior. A daemon-only core would also make basic local commands depend on a long-running process.

## Decision

Complete use cases live in `kb-app` over rules from `kb-core`. The public `AppRequest → AppResponse` dispatcher owns Vault selection, compatibility checks, locks, configuration, registration, planning, status and diagnostics. `kb-cli` maps arguments into requests and renders responses; it does not read or write Vault files.

The CLI calls the application layer in process. Future MCP, HTTP, GUI and WebUI adapters must host the same application operations rather than reimplementing them.

## Alternatives considered

**Route every CLI command through a daemon.** This centralizes behavior but adds service lifecycle and availability to local file operations.

**Implement business rules in each adapter.** This reduces early abstraction work but creates different validation, errors and persistence behavior over time.

## Consequences

- The implemented CLI command surface has one shared location for filesystem effects, schema restrictions and stable errors.
- A future adapter can invoke typed application requests without parsing CLI output.
- Request and response types are compatibility-bearing interfaces and require deliberate evolution.
- Adapter-specific authentication and presentation remain outside use cases, while validation remains inside them.
- Equivalence across multiple external adapters cannot be tested until those adapters exist; adding one requires contract tests against the same application request.

## Risks

- A broad dispatcher can become difficult to maintain unless request families stay separated by use case.
- Adapter-specific concerns may leak into the application layer if boundaries are not reviewed.
