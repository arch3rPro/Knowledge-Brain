# Review and save

`admission.yml` is the authoritative list of source directories Knowledge-Brain may inspect. Do not search other directories to enlarge the source set.

Use the MCP source-review tool or this CLI fallback:

```text
kb review --vault <vault> --json
kb operation show <operation-id> --json
```

Summarize added, changed, missing, and possible-move entries. Applying the returned operation saves exact source versions; it does not modify the admitted files.

For a research or article change, submit the structured request through the MCP knowledge-plan tool or:

```text
kb plan create --request <request.json> --vault <vault> --json
kb operation show <operation-id> --json
```

Check exact source URIs, target paths, managed index/log changes, and conflicts. Creating or inspecting a plan is non-authorizing. Run `kb apply <operation-id> --json` only after the user explicitly approves that operation ID. If the plan is stale, create a new plan instead of bypassing validation.

