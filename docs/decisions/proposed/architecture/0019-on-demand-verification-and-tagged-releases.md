# ADR-0019: Run verification on demand and publish tagged binary releases

- Status: proposed
- Class: architecture
- Date: 2026-09-08

## Problem

Continuous verification on every push and pull request consumes GitHub Actions time even when a maintainer is still iterating locally. The repository also validates release builds without publishing versioned, downloadable CLI artifacts. A public cross-platform CLI needs a deliberate trigger, exact version identity, verifiable artifacts, and a failure path that does not rebuild unaffected platforms.

## Proposal

Knowledge-Brain uses manual verification and annotated release tags as its only GitHub Actions triggers. Ordinary pushes and pull requests do not start workflows.

A manual verification workflow accepts one platform selection: Linux x86_64, macOS ARM64, Windows x86_64, or all three. It runs quality checks and only the selected native jobs. A failed native job is retried through GitHub's failed-job rerun action, which reruns that job rather than its sibling platforms.

Pushing an annotated `vX.Y.Z` tag triggers the release workflow. The tag commit must be reachable from `main`, and `X.Y.Z` must equal the workspace package version. The workflow runs quality checks and three native build jobs: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`. Each job runs workspace tests, builds the release CLI, runs the real CLI journey, packages the binary with the license and installation text, uploads an internal build artifact, and creates build provenance.

After all build jobs succeed, one publication job generates `SHA256SUMS`, creates a temporary draft GitHub Release, uploads all packages and the checksum file, then publishes the Release. Release notes are generated from commits. No macOS notarization or Windows Authenticode signing is attempted until the project has the required Apple and code-signing credentials.

## Alternatives considered

**Run CI on every push and pull request.** This supplies frequent remote feedback but spends hosted runner time on intermediate commits and rebuilds every platform after a known single-platform failure.

**Use one workflow with trigger-dependent conditional jobs.** This reduces file count but combines manual verification and privileged publication conditions in one difficult-to-audit workflow.

**Maintain separate duplicated verification and release build steps.** This makes each workflow superficially simple but risks test, packaging, and target drift between the two paths.

**Require macOS notarization and Windows Authenticode before releasing.** These improve operating-system trust prompts but require credentials the project does not hold and would block the first public binary release.

**Publish artifacts without checksums or provenance.** This shortens the workflow but leaves users unable to verify artifact integrity and build origin.

## Acceptance criteria

- Ordinary pushes and pull requests do not trigger GitHub Actions.
- Manual verification selects one platform or all platforms without starting unselected native jobs.
- A release tag is annotated, reachable from `main`, and exactly matches the Cargo workspace version.
- A successful release publishes Linux x86_64, macOS ARM64, and Windows x86_64 CLI archives with `SHA256SUMS` and GitHub build provenance.
- Each release archive is tested as the packaged release binary before publication.
- Re-running a failed native job does not rebuild unaffected native jobs.
- The publication job creates no public release until all assets are present.
- Documentation states the download, checksum, provenance, retry, and unsigned-platform boundaries.

## Risks

- Removing automatic push and pull-request workflows shifts verification responsibility to maintainers before they create a release tag.
- GitHub-hosted runners and artifact attestation availability remain an external dependency.
- Unsigned macOS and Windows binaries can display platform trust warnings despite checksum and provenance verification.
- A tag is a public release trigger and must be protected by maintainer discipline or repository tag-protection rules.
