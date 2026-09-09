# ADR-0015: Narrow MSRV-compatible MCP adapter

- Status: accepted / implemented
- Class: architecture

## Problem

Knowledge-Brain must expose its shared application operations over MCP stdio without raising the published Rust 1.85 minimum or creating a second implementation of Vault rules. The official Rust MCP SDK main line required Rust 1.88 when this decision was made, while older SDK releases did not publish a dependable MSRV contract.

## Decision

A dedicated `kb-mcp` crate implements only the JSON-RPC lifecycle and tools required by the product. The adapter owns bounded stdio framing, tool schemas and protocol error conversion; every tool calls `kb-app`. The CLI resolves and fixes one absolute Vault root at startup, and the application layer rechecks its identity for every call. The adapter omits the apply tool unless `--allow-write` is explicit and writes no diagnostics to protocol stdout.

The wire and tool registry remain isolated from `kb-core` and `kb-app`, so a later official SDK implementation can replace the adapter without changing application requests or public tool names.

## Alternatives considered

**Raise the MSRV and use the current official SDK.** This reduces protocol code but breaks the approved Rust 1.85 and older-platform contract for an optional adapter.

**Pin an older official SDK.** Older releases might compile on Rust 1.85, but they did not state a stable MSRV and would freeze the product on an older protocol implementation with a larger dependency surface.

**Build a general-purpose MCP framework.** Supporting clients, HTTP transports, prompts, resources and every protocol extension would duplicate the official SDK and exceed Knowledge-Brain's fixed-Vault tool requirement.

## Consequences

- A real `kb mcp` process supports initialize, ping, tools/list and tools/call over newline-delimited stdio.
- Default tool discovery cannot execute an operation; explicit write authority remains constrained to operations owned by the fixed Vault.
- Frames are capped at 1 MiB and malformed input does not stop later requests.
- Compatibility claims are limited to the exercised lifecycle and tools; unsupported MCP features are not advertised.
- Native Rust 1.85 evidence still depends on CI because the implementation host currently provides Rust 1.95.
