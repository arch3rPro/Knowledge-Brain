# Verified ZIP Backup Design

## Goal

Provide an offline, cross-platform ZIP backup that can be verified independently and restored without overwriting existing data. Backup, verification and restore are shared `kb-app` operations; CLI and future adapters only translate requests and reports.

## Commands

```text
kb backup create [--output <PATH.zip>] [--without-source-objects] [--vault <PATH_OR_ID>] [--json]
kb backup verify <PATH.zip> [--json]
kb backup restore <PATH.zip> --target <EMPTY_DIRECTORY> [--json]
```

Create never overwrites an output file. An explicit output must be outside the selected Vault. The default is a timestamped sibling of the Vault. Verify and restore do not require a registered Vault.

## Archive contract

The ZIP root contains `manifest.json` plus Vault-relative file and directory entries. `manifest.json` contains:

- `schema_version: v1.0` for the backup manifest format;
- the Vault schema version and `vault_id` read from `.kb/config.yml`;
- UTC creation time and application version;
- `complete_source_evidence`, false only for `--without-source-objects`;
- sorted portable directory paths;
- sorted file entries with portable path, byte size and lowercase SHA-256.

The backup includes `admission.yml`, `KB.md`, `Wiki/`, every admission directory including disabled entries, `.kb/config.yml`, and `.kb/schemas/`. It preserves empty included directories. It excludes `.kb/config.local.yml`, `.kb/cache/`, `.kb/runtime/`, machine-local state and every `.git` entry. The optional compact form additionally excludes `Wiki/external-sources/.objects/` and is visibly reported as incomplete source evidence.

Only regular files and directories are accepted. Symbolic links, junctions/reparse points, non-UTF-8 paths, non-portable names and case/Unicode collisions fail creation rather than being followed or silently omitted. A shared Vault lock prevents an in-process Knowledge-Brain write during collection, and an unfinished source or knowledge save blocks backup.

## Verification

Verification treats the archive and its manifest as untrusted input. It rejects malformed or unsupported manifests, duplicate or extra ZIP entries, missing entries, absolute or relative traversal paths, backslashes, links/special files, path type conflicts, portable case/Unicode collisions, size mismatches and hash mismatches. It streams each file to compute its digest and reports file count, total bytes, Vault identity and source-evidence completeness.

`manifest.json` is archive metadata and is not restored into the Vault. Archive verification does not claim that Markdown or configuration content is semantically valid; `kb status`, `kb doctor` and `kb source verify` remain separate checks after restore.

## Restore

Restore accepts only a nonexistent target below an existing real directory, or an existing real empty directory. It validates archive metadata before extraction, extracts into a private sibling staging directory, rechecks every byte while writing, and only then moves the staged Vault into the target. If the final move fails, the prior empty-target state is recreated when possible and the staged directory is retained or cleaned with an explicit failure report.

Restore does not merge, overwrite, configure synchronization, or copy machine-local registration. The restored Vault keeps its original `vault_id`; users may register it explicitly on the new machine.

## Limits

This stage verifies process-level failures and hostile archive structure. It does not claim power-loss durability, cross-filesystem atomicity, cross-operating-system execution, semantic migration, encryption, incremental backup, cloud synchronization or conflict resolution.
