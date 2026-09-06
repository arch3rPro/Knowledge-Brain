# ADR-0008: One application layer

- Status: proposed
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

CLI, MCP, HTTP, WebUI, and GUI entry points can drift when each owns filesystem, permission, or business behavior. A daemon-only core would also make basic local commands depend on a long-running process.

## Proposal

Put complete use cases in `kb-app` over rules from `kb-core`. Keep `kb-cli`, `kb-mcp`, `kb-server`, and future user interfaces as parameter, protocol, authentication, and presentation adapters. Let the CLI call the application layer in process; let MCP and HTTP host the same application operations.

## Alternatives considered

**Route every CLI command through a daemon.** This centralizes behavior but adds service lifecycle and availability to local file operations.

**Implement business rules in each adapter.** This reduces early abstraction work but guarantees different validation, errors, and persistence behavior over time.

## Acceptance criteria

- Adapters do not access Vault files except through application operations.
- Equivalent requests return equivalent business data and stable error codes across implemented entries.

## Risks

- Application request and response types require disciplined compatibility management.
- Adapter-specific concerns must remain outside use cases without duplicating validation.
