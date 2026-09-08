# Query and citations

Use the MCP query tool when available. With CLI fallback, run:

```text
kb query "<terms>" --scope all --vault <vault> --json
```

Use `wiki` for maintained knowledge, `sources` for saved evidence, and `all` when both perspectives matter. Keep the result limit proportional to the question. A direct-search hit is lexical evidence, not semantic agreement.

Preserve each result's `source_uri`, `path`, heading or location when citing it. Request narrower terms when snippets do not establish the claim. Do not read `.objects` paths directly; Knowledge-Brain verifies saved object hashes before returning source matches.

