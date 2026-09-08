# ADR-0021: Build a release once before creating its tag

- Status: proposed
- Class: process
- Date: 2026-09-09
- Supersedes: the tag-first publication trigger in [ADR-0019](../architecture/0019-on-demand-verification-and-tagged-releases.md)

## Problem

The tag-first release flow requires maintainers to choose between publishing an unverified tag and running the same full three-platform test and build workload twice. A formal release needs one deliberate trigger, no public tag for a failed build, exact commit identity, and platform-specific retries without rebuilding successful siblings.

## Proposal

The formal release workflow uses `workflow_dispatch` and runs against the current `main` commit. It reads the version from the workspace `Cargo.toml`, requires `docs/releases/vX.Y.Z.md`, and rejects a public Release or a tag that does not resolve to the same commit as the run. An existing annotated tag at the same commit is accepted only so a failed publication job can retry. Ordinary branch pushes, pull requests, and tag pushes do not trigger the workflow.

One invocation runs the quality gate and the three native test, build, CLI journey, package, upload, and provenance jobs. Only after all jobs succeed does the publication job generate and sign checksums, verify that exactly five release assets are ready, create or verify an annotated `vX.Y.Z` tag at the workflow commit, push a newly created tag, create a draft Release from the committed notes, upload the assets, and publish it.

The separate manual verification workflow remains available for optional single-platform diagnosis. It is not a required release step. GitHub's failed-job rerun action retries a failed platform and its required dependencies without rerunning successful sibling platforms. If publication fails after creating the tag, rerunning only publication must accept that same annotated tag at the same commit and refuse any mismatched or already-public version.

## Alternatives considered

**Keep tag-first publication and skip preflight verification.** This runs the build once, but a failed platform leaves a public version tag that cannot safely be moved or reused.

**Require full manual verification before pushing the release tag.** This protects tag quality but repeats the complete test and build workload because the release run cannot treat a separate workflow's artifacts as its own trusted output.

**Reuse artifacts from a prior manual workflow.** This can avoid rebuilding but adds artifact lookup, expiry, commit matching, permission, provenance, and stale-run selection rules to the privileged publication path.

## Acceptance criteria

- Formal release has one manual trigger and one three-platform build pass.
- The workflow derives the tag from Cargo version and runs only for the current `main` commit.
- No tag or Release is created before quality and all native jobs succeed.
- Release notes are required before the run begins.
- Ordinary pushes, pull requests, and tag pushes do not trigger release work.
- Failed-job reruns do not rebuild successful sibling platform jobs.
- Publication retry accepts only the same annotated tag at the same commit and never modifies an existing public Release.
- Workflow contract tests cover the trigger, ordering, tag creation, notes requirement, and retry guards without starting a remote build.

## Risks

- The publication job needs permission to push a tag and create a Release with the repository token.
- A failure after tag push but before Release publication leaves a recoverable tag-only state; the retry checks must distinguish it from a conflicting tag.
- Maintainers no longer receive a remote full-platform preflight unless they intentionally run the optional verification workflow.
