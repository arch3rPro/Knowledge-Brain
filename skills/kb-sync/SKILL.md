---
name: kb-sync
description: Use when a Knowledge-Brain Vault already uses a Git upstream and the user asks to check freshness, synchronize, commit, push, or handle a synchronization conflict. Do not use merely because a Vault directory exists or Git is available.
license: MIT
compatibility: Requires the portable Knowledge-Brain kb CLI on PATH and Git for an already configured Git workflow.
---

# Git synchronization

## Establish whether Git synchronization applies

Resolve the Vault and read `KB.md`. Git is optional: do not initialize a repository, add a remote, configure an upstream, or install an Obsidian Git plugin unless the user explicitly requests that separate setup.

Inspect the repository, current branch, working tree, and configured upstream. A non-Git Vault uses no Git workflow. A Git branch without an upstream is local history only; preserve it and do not invent a remote. When an upstream exists and the user expects shared synchronization, preserve all unrelated user changes and continue below.

## Before a shared write

1. Inspect the working tree and any in-progress merge, rebase, or cherry-pick. Stop on unresolved conflicts.
2. Run `git fetch` for the configured upstream. Fetch updates tracking state but does not modify Vault files.
3. Integrate upstream changes only through the user's established safe workflow. Use a fast-forward when possible; do not guess a merge or rebase policy.
4. Run `kb sync check --vault <path-or-id> --json`.

After a fetch, `git_branch_behind` or `git_branch_diverged` blocks Knowledge-Brain shared writes. Resolve or ask the user to choose the integration before preparing the write. A clean result means only that local checked state is safe; it is not a permanent guarantee that the remote cannot change later.

## After a shared write

Verify the actual Knowledge-Brain result first. Review the Git diff, stage only intended Vault files, and commit only when the user requested or authorized that Git action. Fetch again before pushing, then use a normal push as the final remote concurrency check.

Never force-push, discard user changes, auto-resolve conflicts in `KB.md`, `admission.yml`, shared configuration, or managed `Wiki/`, or treat generated files as safe to choose wholesale. If the push is rejected, preserve local work, fetch again, report the exact state, and stop for an integration decision.
