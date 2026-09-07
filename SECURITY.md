# Security

## Authority and trust boundaries

Knowledge-Brain is a local CLI. The operating-system account and filesystem permissions define its authority: the program can read or change only paths the current process can access. It does not provide a privilege boundary between people sharing the same OS account.

Vault Markdown, YAML, JSON, filenames and source text are untrusted data. They are parsed as data, not instructions. Managed relative paths are revalidated after deserialization and before filesystem access. Symbolic links, Windows junctions/reparse points, path traversal, reserved names and cross-platform case or Unicode collisions are rejected at managed boundaries. These path checks do not sandbox a hostile same-account writer racing filesystem operations.

`admission.yml` is the explicit discovery boundary for user topic directories. Adding a record never creates the directory, and removing a record never deletes its contents. Disabling admission does not revoke or delete previously captured evidence. Framework files under `Wiki/` and `.kb/` are not admissible user topics.

## Writes and recovery

Configuration changes are previewed unless `--yes` is supplied and are written through atomic single-file replacement. Adoption requires a separately stored plan and operation ID. Apply rechecks reviewed inputs, records its own progress, and only restores paths that the same operation recorded and whose bytes still match generated hashes. Changed or unrecorded files are preserved and reported for inspection.

OS file locks decide concurrent ownership. `.kb/runtime/vault-lock-info.json` is explanatory metadata and must not be treated as a lock authority. Git is optional and is never initialized automatically in a user Vault.

Source capture saves original objects, source records and managed log entries together. An unfinished capture blocks knowledge reads and configuration writes until its operation is retried. Keep the user-state plan and progress journal, and the Vault recovery marker, until recovery finishes; they are not disposable caches. SHA-256 detects changed bytes, not source authenticity. Current source queries verify object hashes; history verification is explicit. Process-exit recovery is tested separately from power-loss durability, which depends on the filesystem and is not guaranteed across all storage devices.

Backup archives are untrusted input. Verification rejects path traversal, links and special entries, duplicate or undeclared ZIP entries, portable path collisions, out-of-scope content, mismatched Vault identity, sizes and hashes. Restore writes only through a private sibling staging directory and publishes only to a nonexistent or empty real target. SHA-256 detects bytes changed after backup creation; it does not authenticate who created an archive. Backups are not encrypted, so protect ZIP files with the same access controls as the Vault.

## Network and privacy

The binary has no telemetry, automatic outbound networking, MCP server, cloud account, built-in LLM, source downloader or synchronization. Its optional `kb serve` HTTP adapter defaults to loopback and read-only operation. Non-loopback binding and write-enabled operation require an explicit token file; when a token is configured, every route requires it. The process is fixed to one Vault identity, and operation inspection/apply rejects IDs belonging to another Vault.

The HTTP adapter is plain text and sends no permissive CORS headers. Use non-loopback binding only on a trusted network or behind a user-managed TLS reverse proxy. A bearer token protects application access but does not encrypt Vault content, URLs, headers or responses in transit. Keep token files outside the Vault and version control, protect them with operating-system permissions, and do not pass tokens in command arguments or URLs. HTTP request bodies are bounded, but the service has not undergone denial-of-service hardening or hostile-network testing. See the [HTTP reference](docs/reference/http.md).

Operation SSE exposes only the selected Vault's machine-local event metadata after the same authentication check. Event logs exclude content, diffs, tokens and target paths; operation IDs and timing can still be sensitive usage metadata. Reconnect cursors never execute or cancel an operation. Corrupt logs are rejected rather than partially streamed.

Do not include private Vault content, credentials, personal paths or source documents in a vulnerability report. Provide the smallest synthetic reproduction, affected version, operating system, expected boundary and observed result. Until a private reporting channel is published, do not open a public report containing sensitive data.

## Current assurance boundary

Platform evidence and implementation status are recorded in [Roadmap](ROADMAP.md). Native CI jobs are configured for Windows, macOS and Linux; configuration alone is not successful native evidence. This repository is not yet described as release-ready or cross-platform verified.
