# On-Demand Release and Verified CLI Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace push-triggered CI with manual verification and tagged binary releases, then let only official release binaries explicitly check and install a newer verified stable release.

**Architecture:** A reusable GitHub Actions workflow owns quality checks and the selected native build matrix; a manual caller selects one or all platforms, and a tag caller packages, attests, signs, and publishes all three assets. A new `kb-update` library owns release parsing, download verification, archive validation, and executable replacement; `kb-cli` owns command parsing, rendering, and the hidden replacement-helper entry point.

**Tech Stack:** Rust 1.85, `ureq` with Rustls, `minisign`, `semver`, existing `zip` and `tar`/gzip support, GitHub Actions, GitHub CLI, GitHub artifact attestations, Minisign.

**Spec:** `docs/superpowers/specs/2026-09-08-release-automation-design.md`

## Global Constraints

- Ordinary push and pull-request events must not start a workflow.
- Manual verification selects `linux`, `macos`, `windows`, or `all`; unselected native jobs do not start.
- A release is an annotated `vX.Y.Z` tag whose commit is reachable from `main` and whose version equals root `Cargo.toml`.
- Release targets are Linux `x86_64-unknown-linux-gnu`, macOS `aarch64-apple-darwin`, and Windows `x86_64-pc-windows-msvc`.
- Every release target runs workspace tests, release-binary CLI journey, package smoke test, and target-specific archive creation.
- The updater never polls, never accepts a prerelease, and never changes a source or Cargo installation.
- An update verifies an embedded-public-key Minisign signature before trusting `SHA256SUMS`, preserves the current executable on every failure, and always uses a copied helper after the parent process exits for replacement.
- No macOS notarization or Windows Authenticode is added.
- Do not create a public release tag or set GitHub Secrets without explicit user authorization.

---

### Task 1: Define release artifact conventions and testable package scripts

**Files:**
- Create: `assets/release/INSTALL.md`
- Create: `assets/release/kb-update.minisign.pub`
- Create: `scripts/package-release.sh`
- Create: `scripts/package-release.ps1`
- Create: `scripts/verify-release-package.sh`
- Create: `scripts/verify-release-package.ps1`
- Create: `crates/kb-cli/tests/release_package.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: a release binary path, `KB_RELEASE_VERSION`, and `KB_RELEASE_TARGET`.
- Produces: exactly one archive named `knowledge-brain-v<version>-<target>.tar.gz` on Unix or `.zip` on Windows, containing `kb`/`kb.exe`, `LICENSE`, and `INSTALL.md`.
- Produces: an embedded-public-key source file in standard Minisign public-key format; the committed value is a real public key, never a placeholder or private key.

- [ ] **Step 1: Write failing package-contract tests**

```rust
#[test]
fn release_archive_name_and_entries_are_target_specific() {
    let report = package_report("0.1.0", "x86_64-unknown-linux-gnu");
    assert_eq!(report.name, "knowledge-brain-v0.1.0-x86_64-unknown-linux-gnu.tar.gz");
    assert_eq!(report.entries, ["INSTALL.md", "LICENSE", "kb"]);
}

#[test]
fn release_package_rejects_version_or_binary_name_mismatch() {
    assert!(package_release("0.1.0", "x86_64-pc-windows-msvc", Path::new("kb")).is_err());
}
```

- [ ] **Step 2: Run the contract target and confirm it fails because packaging support does not exist**

Run: `cargo test -p kb-cli --test release_package`

Expected: FAIL because `package_report` and `package_release` do not exist.

- [ ] **Step 3: Add platform package scripts and the Rust test harness**

```sh
# scripts/package-release.sh
set -euo pipefail
stage="$1"
asset="$2"
tar -C "$stage" -czf "$asset" INSTALL.md LICENSE kb
```

```powershell
# scripts/package-release.ps1
Compress-Archive -Path "$Stage/kb.exe", "$Stage/LICENSE", "$Stage/INSTALL.md" -DestinationPath $Asset -Force
```

The test harness creates synthetic staged files, invokes the matching script, lists archive entries with the project archive reader, and rejects wrong executable names. Add the production Minisign public key only after a maintainer has generated a keypair; add a test fixture public key under `crates/kb-cli/tests/fixtures/` for unit tests.

- [ ] **Step 4: Run package-contract tests and both local package verification scripts**

Run: `cargo test -p kb-cli --test release_package && bash scripts/verify-release-package.sh`

Expected: PASS on Unix; the PowerShell script is verified by the Windows manual workflow in Task 5.

- [ ] **Step 5: Commit the artifact convention**

```bash
git add assets/release scripts crates/kb-cli/tests/release_package.rs Cargo.toml
git commit -m "feat: define release package artifacts"
```

### Task 2: Add an isolated updater library and build identity

**Files:**
- Create: `crates/kb-update/Cargo.toml`
- Create: `crates/kb-update/src/lib.rs`
- Create: `crates/kb-update/src/identity.rs`
- Create: `crates/kb-update/src/release.rs`
- Create: `crates/kb-update/tests/release_contract.rs`
- Modify: `Cargo.toml`
- Modify: `crates/kb-cli/Cargo.toml`
- Modify: `crates/kb-cli/build.rs`
- Modify: `crates/kb-cli/src/main.rs`
- Modify: `crates/kb-cli/tests/json_contract.rs`

**Interfaces:**
- Produces `kb_update::BuildIdentity { version: semver::Version, target: Option<ReleaseTarget>, public_key: Option<minisign::PublicKey> }`.
- Produces `ReleaseTarget::{LinuxX64, MacosArm64, WindowsX64}` with `asset_name(&Version) -> String` and `executable_name() -> &'static str`.
- Consumes compile-time `KB_RELEASE_TARGET`; absence means a non-official installation.
- Produces additive `distribution` data from `kb version --json`: `{ "official_release": bool, "target": "..." | null }`.
- Adds the stable CLI error code `update_verification_failed` for a signature, checksum, archive-content, or staged-binary identity failure. Unsupported installations and no available update use the existing non-mutating unavailable-result path.

- [ ] **Step 1: Write failing identity tests**

```rust
#[test]
fn official_identity_requires_a_supported_target_and_valid_public_key() {
    assert!(BuildIdentity::official("0.1.0", "aarch64-apple-darwin", TEST_KEY).is_ok());
    assert!(BuildIdentity::official("0.1.0", "darwin-x64", TEST_KEY).is_err());
}

#[test]
fn non_release_identity_cannot_be_an_update_target() {
    assert!(!BuildIdentity::development("0.1.0").can_update());
}
```

- [ ] **Step 2: Run identity tests and confirm they fail**

Run: `cargo test -p kb-update --test release_contract`

Expected: FAIL because the `kb-update` package and identity API do not exist.

- [ ] **Step 3: Implement the new library and CLI build marker**

```rust
pub struct BuildIdentity {
    pub version: semver::Version,
    pub target: Option<ReleaseTarget>,
    public_key: Option<minisign::PublicKey>,
}

impl BuildIdentity {
    pub fn can_update(&self) -> bool { self.target.is_some() && self.public_key.is_some() }
}
```

`kb-cli/build.rs` reads only `KB_RELEASE_TARGET`, rejects unsupported values at build time, and exports it with `cargo:rustc-env`. The CLI assembles `BuildIdentity` from its own compiled version, build marker, and `include_str!` public key. It augments only the version response; Vault-facing `kb-app` remains unaware of distribution state.

- [ ] **Step 4: Run identity and version-contract tests**

Run: `cargo test -p kb-update --test release_contract && cargo test -p kb-cli --test json_contract`

Expected: PASS; a normal local build reports `official_release: false`.

- [ ] **Step 5: Commit the updater boundary**

```bash
git add Cargo.toml Cargo.lock crates/kb-update crates/kb-cli
git commit -m "feat: add official release identity"
```

### Task 3: Resolve and verify a stable GitHub Release

**Files:**
- Create: `crates/kb-update/src/http.rs`
- Create: `crates/kb-update/src/verify.rs`
- Create: `crates/kb-update/tests/verification.rs`
- Modify: `crates/kb-update/Cargo.toml`
- Modify: `crates/kb-update/src/lib.rs`

**Interfaces:**
- Produces `UpdateCheck { current: Version, latest: Option<AvailableRelease>, update_available: bool }`.
- Produces `VerifiedArchive { version: Version, target: ReleaseTarget, archive: PathBuf, executable: PathBuf, sha256: String }`.
- Consumes `ReleaseTransport`, a small trait returning JSON or bytes for a URL; tests use an in-memory transport and production uses `ureq` with Rustls.
- Accepts only `https://api.github.com/repos/arch3rPro/Knowledge-Brain/releases/latest` data with a strictly newer stable version and one exact target archive.

- [ ] **Step 1: Write failing resolver and verifier tests**

```rust
#[test]
fn check_ignores_prerelease_and_rejects_missing_target_asset() { /* fixture response */ }

#[test]
fn verifier_rejects_tampered_checksum_signature_before_reading_archive() { /* signed fixture */ }

#[test]
fn verifier_rejects_digest_mismatch_and_archive_path_escape() { /* malicious fixture */ }
```

- [ ] **Step 2: Run verifier tests and confirm they fail**

Run: `cargo test -p kb-update --test verification`

Expected: FAIL because release parsing, Minisign verification, and archive staging do not exist.

- [ ] **Step 3: Implement narrow HTTPS resolution and staged verification**

```rust
pub trait ReleaseTransport {
    fn get_json(&self, url: &str) -> Result<serde_json::Value, UpdateError>;
    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError>;
}

pub fn verify_release(identity: &BuildIdentity, release: AvailableRelease, transport: &dyn ReleaseTransport, stage: &Path) -> Result<VerifiedArchive, UpdateError>;
```

Use `ureq = { version = "3.4.1", default-features = true }`, `semver = "1.0.28"`, and `minisign = "0.9.1"` only after confirming they compile under Rust 1.85. Verify `SHA256SUMS.minisig` against the embedded key before parsing a single digest. Parse checksum lines exactly, require one matching archive filename, enforce digest length and hex syntax, and reject duplicate entries. Extract only the expected three regular files into a private stage; reject absolute paths, `..`, links, extra files, and a wrong executable name. Map any verification failure at the CLI boundary to `update_verification_failed` without exposing key material.

- [ ] **Step 4: Run verification tests and targeted lint**

Run: `cargo test -p kb-update --test verification && cargo clippy -p kb-update --all-targets -- -D warnings`

Expected: PASS; no test contacts GitHub.

- [ ] **Step 5: Commit verified release resolution**

```bash
git add Cargo.toml Cargo.lock crates/kb-update
git commit -m "feat: verify signed update archives"
```

### Task 4: Implement replacement recovery and CLI update commands

**Files:**
- Create: `crates/kb-update/src/replace.rs`
- Create: `crates/kb-update/tests/replacement.rs`
- Modify: `crates/kb-update/src/lib.rs`
- Modify: `crates/kb-cli/src/args.rs`
- Modify: `crates/kb-cli/src/main.rs`
- Modify: `crates/kb-cli/src/render.rs`
- Create: `crates/kb-cli/tests/update_commands.rs`

**Interfaces:**
- Adds `kb update check [--json]` and `kb update [--json]`.
- Adds hidden `kb __replace --parent-pid <PID> --from <PATH> --to <PATH> --backup <PATH> [--json]`.
- Produces `UpdateResponse::{Check(UpdateCheck), Scheduled { version: Version }, Applied { version: Version }}`.
- Consumes only `BuildIdentity::can_update() == true`; source and Cargo builds return a non-mutating installation-boundary error.

- [ ] **Step 1: Write failing command and recovery tests**

```rust
#[test]
fn development_binary_refuses_update_without_network_or_file_writes() { /* run kb update check */ }

#[test]
fn helper_restores_backup_when_replacement_fails() { /* synthetic target and staged binary */ }

#[test]
fn helper_replaces_after_parent_exit_and_removes_backup() { /* child helper fixture */ }
```

- [ ] **Step 2: Run update tests and confirm they fail**

Run: `cargo test -p kb-update --test replacement && cargo test -p kb-cli --test update_commands`

Expected: FAIL because no update commands or replacement helper exist.

- [ ] **Step 3: Implement replacement with platform-specific waiting**

```rust
pub fn replace_after_parent_exit(request: ReplaceRequest) -> Result<(), UpdateError> {
    wait_for_exit(request.parent_pid)?;
    replace_with_backup(&request.from, &request.to, &request.backup, MAX_WINDOWS_RETRIES)
}
```

On every platform, copy the current executable to a private helper path, spawn it, then let the parent exit. The helper waits for its parent (a process handle on Windows; a non-reusable process-existence wait on Unix) before replacement; Windows retries rename only for sharing violations. In every replacement path: rename the old target to a sibling backup, rename the staged verified file into place, verify its final digest, remove the backup only after success, and restore the backup if the second rename or verification fails. JSON mode emits one `scheduled` envelope from the parent and suppresses helper prose.

- [ ] **Step 4: Wire `kb update check` and `kb update`**

`check` calls only the resolver and renders current/latest/asset data. `update` runs resolver, verifier, staged `kb version --json` target/version validation, then schedules the copied helper on every platform. Neither command is invoked by normal CLI startup.

- [ ] **Step 5: Run update tests, existing CLI contracts, and format checks**

Run: `cargo test -p kb-update && cargo test -p kb-cli --test update_commands && cargo test -p kb-cli --test json_contract && cargo fmt --all -- --check`

Expected: PASS. The Windows subset is exercised by the manual Windows workflow after Task 5.

- [ ] **Step 6: Commit explicit CLI updates**

```bash
git add crates/kb-update crates/kb-cli Cargo.toml Cargo.lock
git commit -m "feat: add verified cli updates"
```

### Task 5: Replace push CI with reusable manual and release workflows

**Files:**
- Delete: `.github/workflows/ci.yml`
- Create: `.github/workflows/native-build.yml`
- Create: `.github/workflows/verify.yml`
- Create: `.github/workflows/release.yml`
- Create: `.github/workflows/workflow-contract.yml`
- Create: `scripts/check-release-tag.sh`
- Create: `scripts/create-release-checksums.sh`
- Create: `scripts/publish-release.sh`
- Create: `scripts/test-workflows.sh`
- Test: `scripts/test-workflows.sh`

**Interfaces:**
- `native-build.yml` exposes `workflow_call` inputs `targets_json`, `package`, `attest`, and `expected_version`.
- `verify.yml` exposes `workflow_dispatch.inputs.platform` with `linux`, `macos`, `windows`, and `all`.
- `release.yml` is triggered only by `push.tags: ['v*']`, validates the tag, calls the reusable build with all targets, and publishes only after it succeeds.
- `workflow-contract.yml` is `workflow_dispatch` only and validates YAML/trigger/permission assumptions without running native builds.

- [ ] **Step 1: Write failing workflow-contract assertions**

```sh
assert_not_contains .github/workflows/verify.yml 'pull_request:'
assert_not_contains .github/workflows/verify.yml 'push:'
assert_contains .github/workflows/release.yml "tags: ['v*']"
assert_contains .github/workflows/native-build.yml 'fail-fast: false'
assert_contains .github/workflows/release.yml 'KB_UPDATE_SIGNING_KEY'
```

- [ ] **Step 2: Run the workflow contract and confirm it fails against the current CI**

Run: `bash scripts/test-workflows.sh`

Expected: FAIL because `.github/workflows/ci.yml` still listens for push and pull requests and no reusable release workflow exists.

- [ ] **Step 3: Implement the reusable native build workflow**

```yaml
strategy:
  fail-fast: false
  matrix:
    include: ${{ fromJSON(inputs.targets_json) }}
```

The quality job runs once. Each native job checks out the exact ref, runs `cargo test --workspace`, builds `kb-cli --release` with `KB_RELEASE_TARGET`, runs the release CLI journey against that binary, packages and smoke-tests when `package` is true, uploads a target-named artifact, and runs `actions/attest-build-provenance` only when `attest` is true. Pin every third-party action to a full commit SHA and grant only `contents: read`, `attestations: write`, and `id-token: write` where needed.

- [ ] **Step 4: Implement manual verification and tag publication workflows**

`verify.yml` maps the selected platform to one matrix JSON array and calls `native-build.yml` with `package: false` and `attest: false`. `release.yml` runs `scripts/check-release-tag.sh` with a full git history, supplies the all-target matrix plus expected version to the reusable workflow, and permits its final publication job only after that reusable job succeeds.

`scripts/publish-release.sh` downloads only artifacts from the current run, creates `SHA256SUMS`, writes the secret to a mode-600 temporary file, runs `minisign -S -W`, removes the secret file, creates or reuses only a draft Release, uploads the five assets, and makes the draft public. The publication job installs a pinned Minisign package before invoking the script. It refuses a pre-existing public Release and never uses `--clobber` for a public asset.

- [ ] **Step 5: Run static workflow contracts**

Run: `bash scripts/test-workflows.sh`

Expected: PASS.

- [ ] **Step 6: Commit, push, and dispatch a manual single-platform verification**

```bash
git add .github scripts
git commit -m "ci: add manual verification and tagged releases"
```

Push this commit to `main`, then run `gh workflow run verify.yml --repo arch3rPro/Knowledge-Brain -f platform=linux`.

Expected: the GitHub run shows Quality and Linux native jobs only. Do not create a release tag.

### Task 6: Publish release and updater documentation

**Files:**
- Create: `docs/guides/release-a-cli-version.md`
- Create: `docs/reference/cli-updates.md`
- Modify: `README.md`
- Modify: `docs/reference/commands.md`
- Modify: `ROADMAP.md`
- Modify: `SECURITY.md`
- Modify: `docs/decisions/proposed/architecture/0019-on-demand-verification-and-tagged-releases.md`
- Modify: `docs/decisions/proposed/architecture/0020-explicit-verified-cli-updates.md`
- Test: `crates/kb-cli/tests/docs_contract.rs`

**Interfaces:**
- The release guide owns maintainer steps: version edit, manual verification, key Secret provisioning, annotated tag, release observation, failed-job retry, and patch-version correction.
- The update reference owns user commands, JSON fields, supported-installation boundary, verification model, failure handling, backup recovery, and key rotation.
- README owns only user download/install and links to those two owners.

- [ ] **Step 1: Write failing documentation-contract tests**

```rust
#[test]
fn references_name_update_commands_and_release_verification_files() {
    let commands = read("docs/reference/commands.md");
    assert!(commands.contains("kb update check"));
    assert!(commands.contains("kb update"));
    let guide = read("docs/guides/release-a-cli-version.md");
    assert!(guide.contains("SHA256SUMS.minisig"));
    assert!(guide.contains("Re-run failed jobs"));
}
```

- [ ] **Step 2: Run the docs contract and confirm it fails**

Run: `cargo test -p kb-cli --test docs_contract`

Expected: FAIL because the update commands and release guide are not documented.

- [ ] **Step 3: Write the reference and guide at their owning detail level**

The guide states how to generate a Minisign keypair locally, commit only the public key, set `KB_UPDATE_SIGNING_KEY` with GitHub's secret UI or `gh secret set`, and rotate keys without exposing a private key. The reference distinguishes checksum, updater signature, GitHub provenance, and absent platform code signing. README supplies exact asset naming and PATH steps without duplicating release-maintainer procedures.

Move ADR-0019 and ADR-0020 to `docs/decisions/accepted/architecture/` only after the workflows, updater, docs, and a successful real release all exist; rewrite status and decision sections in present tense at that time.

- [ ] **Step 4: Run docs and focused behavior contracts**

Run: `cargo test -p kb-cli --test docs_contract && cargo test -p kb-cli --test update_commands && cargo test -p kb-update`

Expected: PASS.

- [ ] **Step 5: Commit release documentation**

```bash
git add README.md ROADMAP.md SECURITY.md docs crates/kb-cli/tests/docs_contract.rs
git commit -m "docs: publish cli release and update guides"
```

### Task 7: Final evidence and controlled first release handoff

**Files:**
- Modify: `docs/superpowers/plans/2026-09-08-release-and-verified-cli-updates.md`
- Test: all targets named below

**Interfaces:**
- Consumes: implementation commits from Tasks 1–6, a committed official update public key, and a GitHub Secret named `KB_UPDATE_SIGNING_KEY`.
- Produces: documented evidence for manual Linux, macOS ARM64, and Windows x86_64 verification; no public tag unless the user explicitly authorizes one.

- [ ] **Step 1: Run the smallest complete local checks**

Run: `cargo test -p kb-update && cargo test -p kb-cli --test update_commands && cargo test -p kb-cli --test release_package && cargo test -p kb-cli --test docs_contract && cargo fmt --all -- --check && git diff --check`

Expected: PASS; do not run an unrelated workspace-wide local suite.

- [ ] **Step 2: Run manual remote verification for each platform**

Run:

```bash
gh workflow run verify.yml --repo arch3rPro/Knowledge-Brain -f platform=linux
gh workflow run verify.yml --repo arch3rPro/Knowledge-Brain -f platform=macos
gh workflow run verify.yml --repo arch3rPro/Knowledge-Brain -f platform=windows
```

Expected: each run starts only Quality plus the requested native job; each native job runs workspace tests and release CLI journey for its target.

- [ ] **Step 3: Verify selective retry evidence**

Use a deliberately failed workflow-contract fixture or a safely failed non-release manual run, then invoke GitHub's `Re-run failed jobs` control. Confirm the rerun includes only the failed native job and does not restart successful sibling native jobs. Remove any deliberate-failure fixture before commit.

- [ ] **Step 4: Request release authority instead of creating a tag**

Report the exact candidate version, commit SHA, three verified manual run URLs, public-key fingerprint, and whether `KB_UPDATE_SIGNING_KEY` exists. Ask the user for explicit authorization before creating or pushing `vX.Y.Z`; do not infer it from implementation approval.

- [ ] **Step 5: Commit final plan evidence**

```bash
git add docs/superpowers/plans/2026-09-08-release-and-verified-cli-updates.md
git commit -m "test: verify release automation"
```
