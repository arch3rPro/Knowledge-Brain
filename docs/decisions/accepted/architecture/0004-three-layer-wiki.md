# ADR-0004: Three-layer Wiki

- Status: accepted
- Class: architecture

## Problem

Original evidence, developing analysis, and reusable knowledge have different durability and confidence semantics. Storing them together makes provenance and maturity difficult for people and tools to interpret.

## Decision

Use `Wiki/external-sources/`, `Wiki/research/`, and `Wiki/articles/` as the three fixed content layers. Use `Wiki/index.md` for human navigation and `Wiki/log.md` for successfully saved knowledge changes. Topic classification remains inside documents and user-defined directories rather than framework-created topic folders.

## Alternatives considered

**One flat Wiki directory.** A flat structure is simple but does not distinguish evidence, work in progress, and reusable conclusions.

**Many entity-type directories.** A taxonomy of people, projects, tools, and concepts constrains user topics and becomes difficult to evolve.

**A fixed `hot.md`.** Recent context is derived runtime state, while a user's current focus is ordinary research; combining them makes transient state look authoritative.

## Acceptance criteria

- Each layer has independent validation and search scope.
- The default Vault contains no personal or generic topic directory.
- Source-to-article and source-to-research-to-article flows are both valid.

## Risks

- Users must understand the maturity distinction between research and articles.
- Some documents may need explicit human judgment when moving between layers.
