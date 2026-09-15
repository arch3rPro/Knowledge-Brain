---
name: kb-note
description: Use when creating or updating ordinary research documents, project notes, concept notes, or tutorials in a Knowledge-Brain Vault theme directory. Do not use for source ingestion or managed Wiki content.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH; research tools and network access are optional.
---

# Note writing

## Establish context

Confirm the actual environment instead of relying on generic Skill text:

1. Resolve the Vault with `kb paths --vault <path-or-id> --json`, then read its `KB.md`.
2. Run `kb version --json` and `kb capabilities --json`; use the installed CLI's reported behavior.
3. Read `kb config admission list --vault <path-or-id> --json` to understand source scope. Admission is informational for this task: it neither restricts ordinary note writing nor authorizes ingestion.
4. Inspect existing theme directories and nearby ordinary notes.

If this Vault already has a configured Git upstream and the user expects synchronized work, follow `kb-sync` before writing. A local-only or non-Git Vault does not gain a Git requirement.

For a new research document, project analysis, or likely overlapping topic, run a read-only `kb query <object-and-focus> --scope all --vault <path-or-id> --json`. Use results to avoid duplicate work and identify related Wiki knowledge or saved evidence. Empty results are valid, and finding a source does not authorize saving, refreshing, or linking it as managed evidence.

## Locate the note

Honor a user-specified path. Otherwise choose an existing directory by the knowledge subject, not by task words such as research, tutorial, or notes; ask before creating a new theme directory when no fit is clear.

Search for an existing note with the same object and focus before creating another. Prefer updating it when that preserves one coherent home for the subject. Name a new file and its heading with the object plus focus so both remain meaningful outside the current conversation.

## Write

Read [references/writing-standard.md](references/writing-standard.md) before drafting or substantially revising a note.

- For project research, comparisons, or investigations, adapt [assets/research-note.md](assets/research-note.md).
- For durable concepts, tools, or usage guidance, adapt [assets/evergreen-note.md](assets/evergreen-note.md).

Templates are starting structures, not mandatory schemas. Remove irrelevant sections and follow stronger local conventions. Preserve user-authored content when editing, distinguish evidence from inference, and state important unknowns instead of filling gaps.

Writing an ordinary Markdown note is the requested content action; it does not need an extra Knowledge-Brain confirmation. It does not authorize source ingestion or a managed Wiki save. Do not call `kb source save`, `kb knowledge save`, or `kb apply` unless the user separately requests that action. A later ingestion only captures source versions; a later Wiki save must explicitly choose its source and knowledge relationships rather than infer them from this note task.

Report the written path, what changed, and material limitations or unverified claims.
