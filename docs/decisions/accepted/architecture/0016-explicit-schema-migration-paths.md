# ADR-0016: Require explicit schema migration paths

- Status: accepted / implemented
- Class: architecture
- Date: 2026-09-07

## Problem

A schema version lower than the current version does not prove that Knowledge-Brain understands its structure or can transform it safely. Treating every older number as migratable creates a false compatibility promise and encourages current-schema parsers to interpret undefined legacy files.

## Decision

Knowledge-Brain separates numeric version relation from application compatibility. It reports an older schema as migratable only when the application registers a complete sequence of implemented migration steps from that exact version to the current version.

The production migration catalog is empty because `v1.0` is the first published Vault schema. Older versions without a complete path are `older_unsupported`; mutations return `migration_unavailable`. The product does not expose an empty `kb migrate` command.

Each future catalog step must ship with its legacy parser, recoverable transformation and fixtures from the application version that produced the source schema. Catalog metadata alone cannot advertise migration support.

## Boundary ownership

`kb-core` owns version relation, migration-step metadata and deterministic path resolution. `kb-app` owns the production catalog and future filesystem transformations. CLI, MCP, HTTP and future UI adapters consume application results and do not classify versions independently.

## Alternatives considered

**Infer migration support from version ordering.** This preserves the current small API but advertises transformations that may not exist. It cannot distinguish a known historical schema from an arbitrary lower number.

**Declare a minimum supported version range.** A range is concise but still cannot prove that every intermediate transformation exists. It also obscures gaps when support for one historical format is intentionally omitted.

**Invent a `v0.9 → v1.0` migration by changing only the version field.** No released `v0.9` contract or fixtures exist. Rewriting its version would certify unknown content as `v1.0` without evidence.

**Expose `kb migrate` before any real migration exists.** An empty command adds interface surface without a useful outcome and implies broader compatibility than the product provides.

## Consequences

- Version ordering does not independently produce `older_migratable`.
- Only a complete registered path produces `older_migratable` and `migration_required`.
- An older version without a path produces `older_unsupported` and `migration_unavailable`.
- The production catalog contains no fictitious legacy migration.
- Current `v1.0` behavior and newer-version safety boundaries remain unchanged.
- All adapters receive compatibility and errors from the shared application layer.

## Risks

- Adding a new compatibility value and error code requires clients to tolerate additive enum values.
- An empty production catalog means old or hand-edited version labels cannot be repaired automatically.
- Future migrations require fixtures and parsers for each source schema, increasing release discipline and test maintenance.
