# Maintenance

Use factual checks for distinct questions:

```text
kb status --vault <vault> --json
kb doctor --vault <vault> --json
kb lint --vault <vault> --json
kb source verify --vault <vault> --json
```

`status` reports Vault state, `doctor` reports environment and configuration checks, `lint` reports Wiki structure and references, and `source verify` checks saved object hashes. Do not collapse them into one health score.

Direct search does not depend on caches. Run `kb cache rebuild` only to refresh navigation metadata or an enabled BM25 index. A cache is disposable and is never a backup.

Use `kb backup create`, `verify`, and `restore` for portable archives. Restore only to a nonexistent or genuinely empty directory. Do not describe external Agent processing as fully local: returned snippets may enter that Agent's model context.

