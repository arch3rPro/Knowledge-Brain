# Knowledge save request

Use one JSON object with `schema_version: "v1.0"` and a non-empty `changes` array. Each change contains:

- `path`: relative to `Wiki/`; only `research/*.md` or `articles/*.md`.
- `before_sha256`: current full SHA-256 when replacing a file, otherwise `null`.
- `summary`: concise description of the intended knowledge change.
- `content`: complete UTF-8 Markdown with valid OKF frontmatter and traceable `sources`.

Write the request to a temporary file outside the Vault. Do not place request files in an admitted directory or interpolate shell text into JSON. The CLI remains the schema authority; if validation rejects the request, correct it and prepare again.
