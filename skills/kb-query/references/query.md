# Query execution reference

## Local CLI

1. Resolve the selected Vault and read its `KB.md`.
2. Run `kb query "<terms>" --scope wiki|sources|all --vault "<VAULT>" --json`.
3. Treat `snippet` as a preview used to select candidates.
4. Run `kb read "<resource_uri>" --vault "<VAULT>" --json` for each selected result.
5. When `complete=false`, repeat with `--cursor "<next_cursor>"` until complete.
6. Answer from the complete content and cite `resource_uri` plus the returned heading or location.

## Remote MCP

1. Call `kb_status`; confirm `access_mode=remote` and obtain `rules_uri`.
2. Call `kb_read` for `rules_uri` and follow the Vault rules.
3. Call `kb_query` to discover candidate Wiki or saved-source knowledge.
4. Call `kb_read` for each selected result's `resource_uri`, following `next_cursor` when present.
5. Follow returned internal `target_uri` values only when they are relevant.
6. Answer from complete content and cite exact resource URIs.

Remote `path` and `content_path` values belong to the MCP server and are display/diagnostic fields. Do not search for them in the Agent's local filesystem. External links are references only and are not fetched unless the user separately requests web research.

Neither sequence authorizes source saving or Wiki changes.
