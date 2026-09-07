# ADR-0015: Narrow MSRV-compatible MCP adapter

- Status: proposed
- Class: architecture
- Spec: [Knowledge-Brain design](../../../superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Knowledge-Brain must expose its shared application operations over MCP stdio without raising the published Rust 1.85 minimum or creating a second implementation of Vault rules. The current official Rust MCP SDK main line requires Rust 1.88, while older SDK releases do not publish a dependable MSRV contract.

## Proposal

Add a dedicated `kb-mcp` crate that implements only the JSON-RPC lifecycle and tools required by the product. The adapter owns bounded stdio framing, tool schemas and protocol error conversion; every tool calls `kb-app`. The process fixes one Vault ID at startup, omits execution tools unless `--allow-write` is explicit, and writes no diagnostics to protocol stdout.

The wire and tool registry remain isolated from `kb-core` and `kb-app`, so a later official SDK implementation can replace the adapter without changing application requests or public tool names.

## Alternatives considered

**Raise the MSRV and use the current official SDK.** This reduces protocol code but breaks the approved Rust 1.85 and older-platform contract for an optional adapter.

**Pin an older official SDK.** Older releases may compile on Rust 1.85, but they do not state a stable MSRV and would freeze the product on an older protocol implementation with a larger dependency surface.

**Build a general-purpose MCP framework.** Supporting clients, HTTP transports, prompts, resources and every protocol extension would duplicate the official SDK and exceed Knowledge-Brain's fixed-Vault tool requirement.

## Acceptance criteria

- A real `kb mcp` child process completes initialize, tools/list and tools/call exchanges over stdio.
- Default tool discovery and calls cannot execute an operation.
- Explicit write authority can execute only an operation belonging to the fixed Vault.
- Oversized, malformed and unknown requests settle as bounded protocol errors without corrupting stdout.
- The adapter builds under the repository's Rust 1.85 contract in native CI.

## Risks

- MCP evolution may require updates to the narrow wire layer.
- Compatibility claims are limited to the exercised lifecycle and tools; unsupported MCP features must not be advertised.
- Native Rust 1.85 evidence depends on CI because the current development host provides Rust 1.95.

