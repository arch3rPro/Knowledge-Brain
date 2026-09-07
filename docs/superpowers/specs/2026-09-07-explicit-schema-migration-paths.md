# Explicit Schema Migration Paths Design

- Status: approved for implementation
- Date: 2026-09-07
- Scope: schema compatibility classification and migration availability

## Problem

`SchemaVersion::compatibility_with` currently classifies every version older than `v1.0` as `older_migratable`. Knowledge-Brain has never published an older Vault schema and contains no parser or transformation for the synthetic `v0.9` used by compatibility tests. Version ordering therefore makes a migration promise that the application cannot fulfill.

An older version number proves only ordering. Migration availability requires an explicit, implemented sequence of transformations. Status, diagnostics and mutation guards must use the same evidence when describing an older Vault.

## Scope

This change separates version ordering from migration availability and establishes an explicit migration-path catalog. The initial catalog is empty because `v1.0` is the first published Vault schema.

This change does not add `kb migrate`, transform Vault files, define a fictitious `v0.9` layout, change `schema_version`, or alter current `v1.0` Vault behavior. A migration command is added only with the first real migration step.

## Version relation

`SchemaVersion` owns format validation, ordering and structural relation to the current version. Its relation operation returns one of:

- `current`;
- `older`;
- `newer_minor`;
- `newer_major`.

The relation contains no claim about readable fields or available transformations.

## Migration catalog

`kb-core` provides migration metadata and deterministic path resolution. A migration step names an exact `from` version and exact `to` version. A catalog:

- rejects self edges and backward edges;
- rejects more than one outgoing edge for the same version;
- resolves a path only when every step from the source reaches the requested target;
- returns no path when any step is absent.

The production catalog is constructed in `kb-app`, where future filesystem migration implementations belong. The initial production catalog has no steps. Tests may construct synthetic catalogs to prove path resolution without pretending those versions are supported by the product.

Migration execution is a future consumer of the same catalog. Each real step must own its legacy parser, transformation, validation and recovery behavior; the current `PartialConfig` parser is not a substitute for a legacy parser.

## Compatibility classification

Application-level classification combines version relation with the production migration catalog:

| Input | Classification |
| --- | --- |
| `v1.0` | `current` |
| Older version with a complete path to `v1.0` | `older_migratable` |
| Older version without a complete path | `older_unsupported` |
| Newer `v1.x` | `newer_minor_read_only` |
| Newer major version | `newer_major_diagnostic_only` |

All application consumers call this classifier. They do not infer migration availability from numeric ordering.

## Current behavior

For the initial empty catalog:

- `status` reports `v0.9` as `older_unsupported` and leaves current-schema configuration and admission details uninterpreted;
- `doctor` reports that no migration path is available without parsing current-schema fields from the old Vault;
- mutation guards return `migration_unavailable` for `older_unsupported`;
- `migration_required` is reserved for a version with a complete registered path;
- newer minor and newer major behavior remains unchanged;
- current `v1.0` reads and writes remain unchanged.

Generic configuration commands do not interpret unsupported older configuration. Backup compatibility outside the currently understood Vault contract is not expanded by this change.

## Error contract

Add the stable error code `migration_unavailable`. Its message names the Vault version and current application schema. Its next action instructs the user to use a Knowledge-Brain version that explicitly supports that source schema or preserve the Vault with an external byte-for-byte backup.

`migration_required` continues to mean that this build has an executable path but requires an explicit migration before mutation. `schema_too_new` continues to cover newer schema versions.

CLI JSON, MCP and HTTP inherit the same application-layer code and error meaning. No adapter implements separate version logic.

## Future migration execution

The first real schema change must add, in one feature:

1. the exact migration step and legacy parser;
2. a reviewable migration plan;
3. a verified backup prerequisite or equivalent recoverable snapshot;
4. stale-input checks and all-or-restore writes;
5. post-migration validation and cache invalidation;
6. `kb migrate` plus shared application requests for other adapters;
7. fixtures created by the last application version that wrote the source schema.

Adding only catalog metadata without an executable and tested transformation is forbidden.

## Verification

- Core tests distinguish version relation from migration availability.
- Synthetic catalog tests cover a direct path, a multi-step path, a missing step, duplicate outgoing edges, self edges and backward edges.
- Application tests prove that the production empty catalog classifies `v0.9` as `older_unsupported`.
- Real CLI tests prove `status`, `doctor` and a rejected mutation for `v0.9`, including the stable `migration_unavailable` error.
- Existing current, newer-minor and newer-major contract tests continue to pass.
- A real `v1.0` Vault is reopened and queried after the change to prove current behavior is unchanged.

## Documentation ownership

The compatibility table and user-visible behavior belong in `docs/architecture/overview.md` and the relevant command reference. The rationale and rejected alternatives belong in ADR-0016. This specification owns implementation requirements until the change ships.
