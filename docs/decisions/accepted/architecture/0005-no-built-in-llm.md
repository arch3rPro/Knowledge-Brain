# ADR-0005: No built-in LLM

- Status: accepted
- Class: architecture

## Problem

Embedding model clients in the core would require provider selection, credentials, network policy, and nondeterministic behavior in workflows that must remain portable and inspectable.

## Decision

Keep the Knowledge-Brain core deterministic and offline-capable. External Agents may query content and submit structured suggestions through CLI JSON, MCP, or HTTP, but the core validates every suggestion and controls all Vault changes.

## Alternatives considered

**One built-in model provider.** This would make a single vendor and credential format part of the product contract.

**Several built-in model SDKs.** Supporting multiple providers reduces one form of lock-in but multiplies credential, networking, privacy, and compatibility work inside the core.

## Acceptance criteria

- Initialization, configuration, review, extraction, search, lint, backup, and saving operate without model settings or network access.
- Agent-generated content enters the Vault only through validated plans.

## Risks

- Knowledge synthesis requires an external Agent integration.
- Different Agents may produce different suggestions even though storage behavior is deterministic.
