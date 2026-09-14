# Knowledge save request

Use one JSON object with `schema_version: "v1.0"` and a non-empty `changes` array. Existing create and replace requests omit `kind` and remain `upsert` changes. Every change contains `path`, `before_sha256`, and a one-line `summary`.

- `upsert`: omit `kind`; use the current full SHA-256 when replacing a file or `null` when creating it, plus complete `content`.
- `delete`: set `kind: "delete"` and the current full SHA-256. Omit `content`; the target must be a managed Wiki concept.
- `move`: set `kind: "move"`, `from_path`, destination `path`, and the source file's current full SHA-256. Provide the complete managed Markdown at the destination, including any title or link changes. The destination must not exist.

Delete and move are explicit operations. Never use empty content as deletion or modify `Wiki/index.md` and `Wiki/log.md` directly.

Declare the content origin inside the managed `kb` mapping:

- `origin: external_research` for knowledge derived from external material. At least one `sources[].resource` must be an existing exact `kb-source://...?...sha256=...` version. A URL alone is not admitted evidence.
- `origin: original` for user-authored views or decisions that do not claim an external evidence basis. `sources` may be omitted; do not invent a URL or source record.
- Omit `origin` only when updating legacy managed content whose provenance cannot yet be classified. Do not omit it for a new Agent-authored page.

```yaml
kb:
  managed: true
  origin: external_research
```

Write the request to a temporary file outside the Vault. Do not place request files in an admitted directory or interpolate shell text into JSON. The CLI remains the schema authority; if validation rejects the request, correct it and prepare again.
