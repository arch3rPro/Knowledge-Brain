# Security

## Authority and trust boundaries

Knowledge-Brain Stage 1 is a local CLI. The operating-system account and filesystem permissions define its authority: the program can read or change only paths the current process can access. It does not provide a privilege boundary between people sharing the same OS account.

Vault Markdown, YAML, JSON, filenames and future source text are untrusted data. They are parsed as data, not instructions. Managed relative paths are revalidated after deserialization and before filesystem access. Symbolic links, Windows junctions/reparse points, path traversal, reserved names and cross-platform case or Unicode collisions are rejected at managed boundaries.

`admission.yml` is the explicit future read boundary for user topic directories. Adding a record never creates the directory, and removing a record never deletes its contents. Framework files under `Wiki/` and `.kb/` are not admissible user topics.

## Writes and recovery

Configuration changes are previewed unless `--yes` is supplied and are written through atomic single-file replacement. Adoption requires a separately stored plan and operation ID. Apply rechecks reviewed inputs, records its own progress, and only restores paths that the same operation recorded and whose bytes still match generated hashes. Changed or unrecorded files are preserved and reported for inspection.

OS file locks decide concurrent ownership. `.kb/runtime/vault-lock-info.json` is explanatory metadata and must not be treated as a lock authority. Git is optional and is never initialized automatically in a user Vault.

## Network and privacy

Stage 1 has no telemetry, automatic networking, HTTP server, MCP server, cloud account, built-in LLM, source downloader or synchronization. Future LAN access is an optional capability and must require explicit binding, authentication and transport policy; it is not enabled by the current binary.

Do not include private Vault content, credentials, personal paths or source documents in a vulnerability report. Provide the smallest synthetic reproduction, affected version, operating system, expected boundary and observed result. Until a private reporting channel is published, do not open a public report containing sensitive data.

## Current assurance boundary

The native macOS Stage 1 gate has been exercised locally. Windows and Linux have native CI jobs configured but remain pending successful native evidence. This repository is not yet described as release-ready or cross-platform verified.
