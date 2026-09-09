# ADR-0001: Independent Rust core

- Status: accepted
- Class: architecture
- Spec: [Knowledge-Brain design](../../../../.superpowers/specs/2026-09-07-knowledge-brain-design.md)

## Problem

Knowledge-Brain must expose the same local knowledge behavior on Windows, macOS, and Linux without requiring users to install a language runtime or keep a service running. Building on a product-specific application or scripting runtime would make installation, embedding, and host-Agent support depend on that upstream stack.

## Decision

Build Knowledge-Brain as an independent Rust workspace that produces one self-contained `kb` executable. The executable directly hosts the CLI and later dispatches MCP and HTTP adapters; core Vault behavior remains available without a daemon.

## Alternatives considered

**Fork OpenKnowledge.** A fork would inherit useful behavior but would also couple the product to its Desktop/WebUI and Node/Bun architecture.

**Extend Python scripts.** Python is portable in principle, but requiring a compatible interpreter and dependency environment conflicts with the single-program installation goal.

**Retain a Node/Bun CLI.** This would keep a JavaScript runtime in the required execution path and make native packaging less uniform across supported systems.

## Acceptance criteria

- A release-built `kb` binary performs every core workflow without Node.js, Python, Java, or a daemon.
- The same Vault format and command semantics pass native Windows, macOS, and Linux tests.

The Rust workspace and self-contained CLI are implemented. Native Windows and Linux acceptance evidence remains pending and is tracked in the Roadmap; accepting this architectural choice does not claim that platform verification has completed.

## Risks

- Rust increases the initial implementation cost for document parsing and host integrations.
- Platform-specific filesystem behavior still requires native testing even with one codebase.
