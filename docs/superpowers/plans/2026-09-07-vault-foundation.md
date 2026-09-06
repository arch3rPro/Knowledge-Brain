# Knowledge-Brain 阶段 1：Vault 基础实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付可在 Windows、macOS 和 Linux 上创建、采用、移动并重新打开最小 Vault 的 `kb` 命令行基线。

**Architecture:** 本阶段建立 `kb-core`、`kb-app`、`kb-protocol`、`kb-cli` 四个边界清晰的 crate；MCP 和 HTTP 适配 crate 延后到阶段 4。CLI 只解析输入和渲染输出，所有 Vault、配置、路径和采用流程由 `kb-app` 调用 `kb-core` 的领域规则完成。真实数据写入 Markdown/YAML/JSON，用户级注册表和采用计划保存到操作系统标准目录。

**Tech Stack:** Rust 2024 edition（MSRV 1.85）、Cargo workspace、clap、serde/serde_json、serde_yaml_ng、yaml-edit、directories、uuid、sha2、unicode-normalization、atomic-write-file、fs2、assert_cmd、predicates、tempfile。

**Spec:** `docs/superpowers/specs/2026-09-07-knowledge-brain-design.md`

## Global Constraints

- Windows、macOS 和 Linux 使用同一套数据结构和核心行为。
- Markdown、YAML 和 JSON 是数据真相；缓存和运行记录不得成为恢复知识所必需的数据。
- 初始核心不依赖 Node.js、Python、Java、常驻服务或内置 LLM。
- Knowledge-Brain 配置使用 `schema_version: "v1.0"`；OKF 文档使用 `okf_version: "0.2"`。
- 配置优先级固定为 CLI 参数 > 环境变量 > Vault 本机配置 > Vault 共享配置 > 用户配置 > 内置默认值。
- `admission.yml` 是准入目录唯一数据源，只允许 Vault 根目录下的一级相对目录。
- 配置编辑必须保留未知字段、字段顺序和目标节点之外的注释；无法安全编辑时拒绝写入。
- 配置路径使用 `/`，管理文本使用 UTF-8、无 BOM、LF；生成路径遵守三个目标平台的共同限制。
- `kb init` 只接受不存在或空目录，不创建示例主题、Git 仓库、BM25 索引或局域网服务。
- `kb adopt` 先生成采用计划；它不移动已有文件、不自动准入目录、不改写已有 Markdown。
- `kb` 不得替用户的 Vault 自动初始化、提交、拉取或推送 Git。Knowledge-Brain 产品仓库已获用户明确授权，可以初始化 Git 并按任务提交检查点。
- 软件许可证由用户在公开发布前确认；阶段 1 不推断许可证，也不创建 `LICENSE`。
- 阶段 1 不实现来源审查、内容提取、Wiki 查询、OKF 校验、知识修改、MCP、HTTP、BM25、备份或发布。

## Stage 1 File Map

```text
Cargo.toml                         # workspace 成员、统一依赖和 lint
Cargo.lock                         # 可重复依赖解析
rust-toolchain.toml                # stable 通道与 rustfmt/clippy 组件
.cargo/config.toml                 # 跨平台命令别名，不包含本机路径
crates/kb-core/                    # 版本、错误、路径、配置和准入领域类型
crates/kb-protocol/                # 稳定 JSON envelope
crates/kb-app/                     # Vault、配置、注册、采用、状态和诊断用例
crates/kb-cli/                     # 唯一 kb 二进制、参数和人类/JSON 渲染
assets/vault-template/             # 编译进 kb 的最小 Vault 文本资产
schemas/                           # 产品拥有的 JSON Schema 源文件
tests/e2e/                         # 构建后 kb 的真实入口测试
tests/fixtures/                    # 配置、准入和已有目录夹具
docs/decisions/proposed/           # 尚未完整实现的架构决定
docs/reference/                    # 阶段 1 当前行为的查阅文档
docs/guides/                       # 初始化和采用的顺序式教程
```

`docs/superpowers/` 保存设计和执行计划，不承载产品使用手册。Schema 的字段语义以 `schemas/` 为唯一来源；参考页链接 Schema，不复制完整字段目录。

---

### Task 1: Record the architectural decisions owed before implementation

**Files:**

- Create: `docs/decisions/proposed/architecture/0001-independent-rust-core.md`
- Create: `docs/decisions/proposed/architecture/0002-files-are-the-knowledge-source-of-truth.md`
- Create: `docs/decisions/proposed/architecture/0003-admission-list.md`
- Create: `docs/decisions/proposed/architecture/0004-three-layer-wiki.md`
- Create: `docs/decisions/proposed/architecture/0005-no-built-in-llm.md`
- Create: `docs/decisions/proposed/architecture/0006-direct-search-and-optional-bm25f.md`
- Create: `docs/decisions/proposed/architecture/0007-reviewed-plans-and-all-or-restore-writes.md`
- Create: `docs/decisions/proposed/architecture/0008-one-application-layer.md`
- Create: `docs/decisions/proposed/architecture/0009-verified-zip-backups-without-built-in-sync.md`

**Interfaces:**

- Consumes: Design sections 2, 3, 5, 10, 11, 20, 23 and 28.
- Produces: Stable rationale links for crate READMEs and later implementation plans.

- [ ] **Step 1: Create every record with the proposed lifecycle and architecture class**

Each file uses its exact title from the file map, followed by `Status: proposed`,
`Class: architecture`, and a link to the design spec. The body headings are
`Problem`, `Proposal`, `Alternatives considered`, `Acceptance criteria`, and
`Risks`, in that order. The table below supplies the complete propositions;
write them as durable prose rather than copying table syntax.

The exact subjects are:

| ADR and exact title | Problem | Proposal | Alternatives that must be recorded | Acceptance criterion |
| --- | --- | --- | --- | --- |
| 0001: Independent Rust core | Runtime coupling prevents one portable tool | Independent Rust workspace and one `kb` executable | Fork OpenKnowledge; Python scripts; Node/Bun CLI | Core workflows run from a self-contained binary |
| 0002: Files are the knowledge source of truth | A proprietary store can make ordinary files insufficient for recovery | Markdown/YAML/JSON are authoritative; indexes are rebuildable | SQLite as primary store; vector DB as primary store | Deleting caches preserves knowledge and direct reads |
| 0003: Admission list | Configuration and processing queues can be confused | Root `admission.yml` declares enabled top-level directories only | `pending.yml`; `triage`; whole-Vault scans | `kb review` never reads outside enabled entries |
| 0004: Three-layer Wiki | Sources, developing research and reusable knowledge have different maturity | `external-sources`, `research`, `articles`, plus `index.md` and `log.md` | Flat Wiki; entity-type tree; fixed `hot.md` | Each layer is addressable without personal topic folders |
| 0005: No built-in LLM | Built-in model clients add credentials, networking and vendor coupling | Deterministic core accepts suggestions from external Agents | One built-in provider; multiple SDKs | All core commands work offline without model settings |
| 0006: Direct search and optional BM25F | Small and large Vaults need explainable retrieval without changing the source of truth | Direct Markdown is baseline; complete BM25F is opt-in | grep-only; vectors by default; automatic mode switch | Cache loss falls back to direct search |
| 0007: Reviewed plans and all-or-restore writes | Multi-file knowledge edits can overwrite human work or stop halfway | Inspectable plans, explicit operation IDs and all-or-restore writes | Direct Agent writes; Git-only rollback; independent file saves | Stale inputs reject all writes; interrupted saves recover |
| 0008: One application layer | Separate entry implementations drift | CLI, MCP, HTTP and future UIs share `kb-app` | CLI through daemon; business rules in adapters | Equal requests return equal data/error semantics |
| 0009: Verified ZIP backups without built-in sync | Copy-only backups cannot prove integrity and built-in sync expands scope | Verified ZIP restore into an empty target; sync remains external | Plain copy guidance; proprietary cloud sync | Cross-OS restore validates every manifest entry |

- [ ] **Step 2: Check lifecycle wording**

Run:

```bash
rg -n 'Status:|Class:|## Alternatives considered|## Acceptance criteria|## Risks' docs/decisions/proposed
```

Expected: each of the nine files has all five required markers; proposals may use future tense and must not claim shipped behavior.

- [ ] **Step 3: Check that no decision is duplicated as a session narrative**

Run:

```bash
rg -n '\b(previously|now|reviewer)\b|no longer|this PR|本次讨论|刚才|此前' docs/decisions/proposed
```

Expected: no matches.

- [ ] **Step 4: Commit the decision records**

Run `git add docs/decisions/proposed && git commit -m "docs: record Knowledge-Brain architecture decisions"`.

### Task 2: Bootstrap the Rust workspace and the real `kb` entry path

**Files:**

- Create: `Cargo.toml`
- Create: `Cargo.lock`
- Create: `rust-toolchain.toml`
- Create: `.cargo/config.toml`
- Create: `crates/kb-core/Cargo.toml`
- Create: `crates/kb-core/src/lib.rs`
- Create: `crates/kb-app/Cargo.toml`
- Create: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-protocol/Cargo.toml`
- Create: `crates/kb-protocol/src/lib.rs`
- Create: `crates/kb-cli/Cargo.toml`
- Create: `crates/kb-cli/src/main.rs`
- Create: `tests/e2e/Cargo.toml`
- Create: `tests/e2e/src/lib.rs`
- Create: `tests/e2e/tests/help.rs`

**Interfaces:**

- Consumes: no implementation interface.
- Produces: workspace crates and a binary named `kb`; the application request dispatcher is introduced in Task 10.

- [ ] **Step 1: Write the failing real-entry test**

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_identifies_the_portable_cli() {
    Command::cargo_bin("kb")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Knowledge-Brain"))
        .stdout(predicate::str::contains("Usage: kb"));
}
```

- [ ] **Step 2: Run the test and observe the absent binary**

Run:

```bash
cargo test -p kb-e2e --test help
```

Expected: failure because the workspace and `kb` binary do not exist.

- [ ] **Step 3: Create the workspace manifests**

Use a workspace manifest with resolver 3, `edition = "2024"`, `rust-version = "1.85"`, and these dependency ranges:

```toml
[workspace]
members = [
  "crates/kb-core",
  "crates/kb-app",
  "crates/kb-protocol",
  "crates/kb-cli",
  "tests/e2e",
]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
publish = false

[workspace.dependencies]
atomic-write-file = "0.3"
clap = { version = "4.6", features = ["derive"] }
directories = "6.0"
fs2 = "0.4"
hex = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml_ng = "0.10"
sha2 = "0.10"
thiserror = "2"
unicode-normalization = "0.1"
uuid = { version = "1", features = ["v4", "serde"] }
yaml-edit = "0.2"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"
```

The e2e crate uses `assert_cmd = "2"`, `predicates = "3"`, `serde_json = "1"`, and `tempfile = "3"` as dev dependencies. Do not create `kb-mcp` or `kb-server` placeholders in this stage.

- [ ] **Step 4: Implement the minimum CLI**

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "kb", version, about = "Knowledge-Brain portable knowledge vault")]
struct Cli {}

fn main() {
    let _ = Cli::parse();
}
```

- [ ] **Step 5: Run formatting, lint and the real-entry test**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p kb-e2e --test help
```

Expected: all three commands exit 0 and the test launches the built `kb` binary.

- [ ] **Step 6: Commit the workspace baseline**

Run `git add Cargo.toml Cargo.lock rust-toolchain.toml .cargo crates tests/e2e && git commit -m "chore: bootstrap Rust workspace"`.

### Task 3: Define schema versions, stable errors and JSON envelopes

**Files:**

- Create: `crates/kb-core/src/version.rs`
- Create: `crates/kb-core/src/error.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Create: `crates/kb-protocol/src/envelope.rs`
- Modify: `crates/kb-protocol/src/lib.rs`
- Create: `crates/kb-core/tests/version.rs`
- Create: `crates/kb-protocol/tests/envelope.rs`

**Interfaces:**

- Consumes: workspace crates from Task 2.
- Produces: `SchemaVersion`, `CURRENT_SCHEMA_VERSION`, `ErrorCode`, `KbError`, `Envelope<T>`, and `ErrorEnvelope`.

- [ ] **Step 1: Write failing version and envelope tests**

```rust
#[test]
fn schema_version_requires_v_major_minor() {
    assert_eq!("v1.0".parse::<SchemaVersion>().unwrap(), SchemaVersion::new(1, 0));
    assert!("1.0".parse::<SchemaVersion>().is_err());
    assert!("v1".parse::<SchemaVersion>().is_err());
}
```

```rust
#[test]
fn error_envelope_has_stable_machine_fields() {
    let value = serde_json::to_value(ErrorEnvelope::from(KbError::invalid_config(
        "admission.yml",
        "directories must be a sequence",
    )))
    .unwrap();
    assert_eq!(value["schema_version"], "v1.0");
    assert_eq!(value["error"]["code"], "invalid_config");
    assert_eq!(value["error"]["retryable"], false);
    assert!(value["error"]["next_action"].is_string());
}
```

- [ ] **Step 2: Verify both tests fail**

Run:

```bash
cargo test -p kb-core --test version
cargo test -p kb-protocol --test envelope
```

Expected: unresolved imports for the new public types.

- [ ] **Step 3: Implement the public types**

`SchemaVersion` must parse only `v<major>.<minor>`, display the same shape, and expose compatibility classification:

```rust
pub enum SchemaCompatibility {
    Current,
    OlderMigratable,
    NewerMinorReadOnly,
    NewerMajorDiagnosticOnly,
}

pub const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0);
```

`ErrorCode` must serialize to snake case and initially include:

```rust
pub enum ErrorCode {
    InvalidConfig,
    PathNotAdmitted,
    PlanStale,
    WriteBusy,
    VaultNeedsRecovery,
    RestoreFailed,
    IndexStale,
    CapabilityUnavailable,
    AuthDenied,
    VaultNotFound,
    TargetNotEmpty,
    UnsafePath,
    OperationNotFound,
}
```

The protocol types are:

```rust
#[derive(Debug, Serialize)]
pub struct Envelope<T> {
    pub schema_version: SchemaVersion,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub next_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
```

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test -p kb-core --test version
cargo test -p kb-protocol --test envelope
```

Expected: both pass.

- [ ] **Step 5: Commit the schema and protocol types**

Run `git add crates/kb-core crates/kb-protocol && git commit -m "feat: define schema and protocol errors"`.

### Task 4: Enforce portable paths and Vault discovery

**Files:**

- Create: `crates/kb-core/src/path.rs`
- Create: `crates/kb-core/src/platform.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Create: `crates/kb-core/tests/path_rules.rs`
- Create: `tests/e2e/tests/vault_discovery.rs`

**Interfaces:**

- Consumes: `KbError` from Task 3.
- Produces: `PortableRelativePath::parse(&str)`, `validate_generated_path(&Path)`, `validate_admission_directory(&Path)`, `find_vault_root(&Path)`, and `detect_portability_collisions(&[PathBuf])`.

- [ ] **Step 1: Write failing path-rule tests**

```rust
#[test]
fn admission_accepts_one_portable_component_only() {
    assert!(validate_admission_directory(Path::new("Reading")).is_ok());
    for path in ["../Reading", "a/b", "Wiki", ".kb", "CON", "notes.", "notes "] {
        assert!(validate_admission_directory(Path::new(path)).is_err(), "{path}");
    }
}

#[test]
fn equivalent_case_and_unicode_names_collide() {
    let paths = vec![PathBuf::from("Wiki/Café.md"), PathBuf::from("wiki/Cafe\u{301}.md")];
    assert_eq!(detect_portability_collisions(&paths).len(), 1);
}
```

- [ ] **Step 2: Verify the tests fail**

Run `cargo test -p kb-core --test path_rules`.

Expected: unresolved path-rule functions.

- [ ] **Step 3: Implement lexical validation before filesystem access**

Reject absolute paths, prefixes, roots, parent/current components, `Wiki`, `.kb`, Windows reserved basenames (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`), forbidden characters, trailing spaces/dots, NUL, case-fold collisions and Unicode NFC-equivalent collisions. Store portable paths with `/`; convert to native separators only at the filesystem boundary.

Use this normalization key:

```rust
pub fn portability_key(path: &PortableRelativePath) -> String {
    path.as_str().nfc().flat_map(char::to_lowercase).collect()
}
```

- [ ] **Step 4: Add real filesystem rejection for links**

Use `symlink_metadata` for Unix symbolic links. On Windows, inspect `MetadataExt::file_attributes()` and reject any `FILE_ATTRIBUTE_REPARSE_POINT`, covering junctions as well as symbolic links. A missing admitted directory is a configuration error, not an instruction to create it.

- [ ] **Step 5: Test ancestor discovery through the built binary**

Create a real temporary directory containing `.kb/config.yml`, invoke `kb status --vault <nested-directory> --json`, and assert that the response reports the ancestor Vault root. This test remains red until Task 10 wires `status`; mark it `#[ignore = "status command is implemented in Task 10"]` and remove the ignore in Task 10.

- [ ] **Step 6: Run the focused unit suite**

Run `cargo test -p kb-core --test path_rules`.

Expected: all active tests pass on the current OS.

- [ ] **Step 7: Commit portable path enforcement**

Run `git add crates/kb-core tests/e2e/tests/vault_discovery.rs && git commit -m "feat: enforce portable vault paths"`.

### Task 5: Embed and initialize the minimum Vault

**Files:**

- Create: `assets/vault-template/admission.yml`
- Create: `assets/vault-template/KB.md`
- Create: `assets/vault-template/Wiki/index.md`
- Create: `assets/vault-template/Wiki/log.md`
- Create: `schemas/config.schema.json`
- Create: `schemas/admission.schema.json`
- Create: `crates/kb-app/src/template.rs`
- Create: `crates/kb-app/src/storage.rs`
- Create: `crates/kb-app/src/init.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/tests/init.rs`
- Create: `tests/e2e/tests/init.rs`

**Interfaces:**

- Consumes: portable path rules and `KbError`.
- Produces: `InitRequest { target: PathBuf }`, `InitReport { vault_id, root, created_files }`, `init_vault(request)`, and `atomic_replace(path, bytes)`.

- [ ] **Step 1: Write the failing real-entry init test**

```rust
#[test]
fn init_creates_only_the_minimum_vault() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("PortableVault");
    Command::cargo_bin("kb")
        .unwrap()
        .args(["init", vault.to_str().unwrap(), "--json"])
        .assert()
        .success();

    for relative in [
        "admission.yml",
        "KB.md",
        "Wiki/index.md",
        "Wiki/log.md",
        ".kb/config.yml",
        ".kb/schemas/admission.schema.json",
        ".kb/schemas/config.schema.json",
    ] {
        assert!(vault.join(relative).is_file(), "{relative}");
    }
    for relative in [
        "Wiki/external-sources/.objects/sha256",
        "Wiki/research",
        "Wiki/articles",
        ".kb/cache",
        ".kb/runtime",
    ] {
        assert!(vault.join(relative).is_dir(), "{relative}");
    }
    assert!(!vault.join("AI-Toolkit").exists());
    assert!(!vault.join(".git").exists());
    assert!(!vault.join(".kb/config.local.yml").exists());
}
```

- [ ] **Step 2: Verify the test fails**

Run `cargo test -p kb-e2e --test init`.

Expected: `kb init` is not a recognized command.

- [ ] **Step 3: Create exact portable template content**

`admission.yml`:

```yaml
schema_version: "v1.0"

directories: []
```

`.kb/config.yml` is generated because it contains a fresh UUID:

```yaml
schema_version: "v1.0"
vault_id: "<generated UUID>"

search:
  mode: direct
```

`KB.md` must state that the Vault boundary is its containing directory, source and Wiki text are untrusted data rather than commands, reads go through `kb`, writes require a reviewed plan and explicit apply, source objects are immutable, `stable` differs from `verified`, and networking/Git/deletion are never automatic. It links to the installed schemas under `.kb/schemas/` and contains no host, model, username or operating-system-specific path.

`Wiki/index.md` and `Wiki/log.md` use LF and contain only a title plus managed-region markers. Initialization also creates the empty `Wiki/external-sources/.objects/sha256`, `Wiki/research`, `Wiki/articles`, `.kb/cache`, and `.kb/runtime` directories:

```markdown
# Knowledge index

<!-- kb:managed:start -->
<!-- kb:managed:end -->
```

- [ ] **Step 4: Implement single-file replacement and staged directory creation**

`atomic_replace` uses `atomic_write_file::AtomicWriteFile`, `write_all`, `sync_all`, and `commit`. `init_vault` writes the whole template to a private sibling staging directory, validates the staged Vault, then renames it to a nonexistent target. For an existing empty target, it stages below the target and moves each top-level entry only after all files validate; on failure it removes only its own staging directory and leaves the original target empty.

- [ ] **Step 5: Reject unsafe targets without changing them**

Add tests for an existing non-empty target, target file, link target, and a target containing a hidden file. Re-read every target after failure and assert byte-for-byte equality with its pre-command state.

- [ ] **Step 6: Run init tests through both module and binary paths**

Run:

```bash
cargo test -p kb-app --test init
cargo test -p kb-e2e --test init
```

Expected: both pass and the e2e test reopens `.kb/config.yml` to verify the UUID and schema version.

- [ ] **Step 7: Commit minimum Vault initialization**

Run `git add assets schemas crates/kb-app tests/e2e/tests/init.rs && git commit -m "feat: initialize minimum portable vault"`.

### Task 6: Load layered configuration with field provenance

**Files:**

- Create: `crates/kb-core/src/config.rs`
- Create: `crates/kb-core/src/admission.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Create: `crates/kb-app/src/config/load.rs`
- Create: `crates/kb-app/src/config/mod.rs`
- Create: `crates/kb-app/src/user_dirs.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/tests/config_layers.rs`
- Create: `tests/fixtures/config/layered/.kb/config.yml`
- Create: `tests/fixtures/config/layered/.kb/config.local.yml`
- Create: `tests/fixtures/config/layered/admission.yml`

**Interfaces:**

- Consumes: `SchemaVersion`, `PortableRelativePath`, and Vault discovery.
- Produces: `EffectiveConfig`, `ConfigSource`, `Sourced<T>`, `ConfigOverrides`, `load_effective_config(vault, overrides)`, `AdmissionDocument`, and `AdmissionEntry`.

- [ ] **Step 1: Write a failing precedence test**

```rust
#[test]
fn precedence_is_cli_env_local_vault_user_default() {
    let loaded = fixture()
        .with_user("limits.max_file_bytes", 10)
        .with_vault("limits.max_file_bytes", 20)
        .with_local("limits.max_file_bytes", 30)
        .with_env("KB_LIMITS_MAX_FILE_BYTES", "40")
        .with_cli("limits.max_file_bytes", "50")
        .load()
        .unwrap();
    assert_eq!(loaded.limits.max_file_bytes.value, 50);
    assert_eq!(loaded.limits.max_file_bytes.source, ConfigSource::Cli);
}
```

- [ ] **Step 2: Verify the test fails**

Run `cargo test -p kb-app --test config_layers`.

Expected: configuration types are absent.

- [ ] **Step 3: Implement typed partial layers and merge order**

Use `Option<T>` in `PartialConfig`, never sentinel values. `EffectiveConfig` contains a `Sourced<T>` for every user-visible field so `kb config show --sources` can identify the winning layer. Initial keys are:

```text
search.mode
limits.max_file_bytes
limits.max_files_per_review
limits.max_total_read_bytes
files.include_hidden
operations.plan_retention_hours
```

Built-in defaults are `direct`, 50 MiB, 10,000 files, 500 MiB, `false`, and 168 hours respectively. Environment names use the `KB_` prefix and uppercase underscore-separated keys. An invalid higher-precedence value is an error; it does not silently fall through.

- [ ] **Step 4: Parse and validate admission independently**

`AdmissionDocument` must reject duplicate IDs, duplicate portability keys, disabled entries with invalid paths, links/junctions, reserved `Wiki` and `.kb`, absolute paths and nested paths. Missing `include` defaults to the supported-file patterns at review time; missing `exclude` defaults to framework safety exclusions. These defaults are returned as effective values but are not written into the user's file.

- [ ] **Step 5: Resolve operating-system directories without personal defaults**

Use `directories::ProjectDirs::from("org", "Knowledge-Brain", "Knowledge-Brain")`. Support `KB_CONFIG_DIR`, `KB_STATE_DIR`, and `KB_CACHE_DIR` overrides for deterministic tests and portable automation. Expose:

```rust
pub struct UserPaths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
}
```

- [ ] **Step 6: Run focused configuration tests**

Run `cargo test -p kb-app --test config_layers`.

Expected: precedence, provenance, invalid-value, unknown-field and schema compatibility cases pass.

- [ ] **Step 7: Commit layered configuration loading**

Run `git add crates/kb-core crates/kb-app tests/fixtures/config && git commit -m "feat: load layered vault configuration"`.

### Task 7: Provide lossless config and admission editing

**Files:**

- Create: `crates/kb-app/src/config/edit.rs`
- Create: `crates/kb-app/src/config/commands.rs`
- Modify: `crates/kb-app/src/config/mod.rs`
- Create: `crates/kb-app/tests/config_edit.rs`
- Create: `tests/fixtures/config/comments/config.yml`
- Create: `tests/fixtures/config/comments/admission.yml`
- Create: `tests/e2e/tests/config.rs`

**Interfaces:**

- Consumes: layered config loader, `atomic_replace`, `AdmissionDocument`.
- Produces: `config_show`, `config_get`, `config_set`, `config_unset`, `config_validate`, and `admission_{list,add,enable,disable,remove}` application functions.

- [ ] **Step 1: Write the failing preservation test**

```rust
#[test]
fn setting_one_key_preserves_unrelated_text() {
    let before = "# owner note\nschema_version: \"v1.0\"\ncustom: keep # inline\nsearch:\n  mode: direct\n";
    let after = edit_config(before, "search.mode", Some("bm25")).unwrap();
    assert!(after.starts_with("# owner note\n"));
    assert!(after.contains("custom: keep # inline\n"));
    assert!(after.contains("search:\n  mode: bm25\n"));
}
```

- [ ] **Step 2: Verify the preservation test fails**

Run `cargo test -p kb-app --test config_edit`.

Expected: the editor is absent.

- [ ] **Step 3: Implement parse-validate-edit-validate-write**

Parse semantics with `serde_yaml_ng` and edit the concrete syntax tree with `yaml_edit::Document`. Before writing:

1. reject duplicate target keys or unsupported YAML constructs at the target node;
2. calculate and return a line-oriented preview;
3. parse the edited text into typed configuration;
4. load all layers and validate the resulting effective configuration;
5. atomically replace only the selected file.

If any step cannot preserve unrelated text, return `invalid_config` with a concrete manual edit and leave bytes unchanged.

- [ ] **Step 4: Implement target-layer rules**

`config set/unset` defaults to `.kb/config.yml`; `--local` targets `.kb/config.local.yml`; `--user` targets `<config_dir>/config.yml`. Reject multiple layer flags. `vault_id` and `schema_version` cannot be set or unset through generic key commands. Admission directories are editable only through `kb config admission ...`.

- [ ] **Step 5: Implement admission operations by stable ID**

`add <id> <top-level-directory>` appends one entry with `enabled: true`; enable/disable updates only that entry's scalar; remove deletes only that entry. Every operation previews the diff unless `--yes` is passed. JSON mode returns the proposed diff and requires `--yes` to mutate; no interactive prompt is written to stdout in JSON mode.

- [ ] **Step 6: Verify from the real entry path**

Run:

```bash
cargo test -p kb-app --test config_edit
cargo test -p kb-e2e --test config
```

Expected: both pass. The e2e suite invokes `kb config set`, re-reads the file, invokes `kb config show --sources --json`, and verifies the effective value and source.

- [ ] **Step 7: Commit lossless configuration editing**

Run `git add crates/kb-app tests/fixtures/config tests/e2e/tests/config.rs && git commit -m "feat: edit configuration without rewriting user text"`.

### Task 8: Register, select and relocate Vaults

**Files:**

- Create: `crates/kb-app/src/registry.rs`
- Create: `crates/kb-app/src/vault.rs`
- Modify: `crates/kb-app/src/init.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/tests/registry.rs`
- Create: `tests/e2e/tests/vault.rs`

**Interfaces:**

- Consumes: `UserPaths`, Vault discovery, `atomic_replace`, `vault_id` from shared config.
- Produces: `VaultRegistry`, `VaultRecord { vault_id, path }`, `register_vault`, `resolve_vault`, `rebind_vault`, `list_vaults`, and CLI selectors `--vault <path-or-id>`.

- [ ] **Step 1: Write failing relocation behavior**

```rust
#[test]
fn moved_vault_can_be_rebound_by_stable_id() {
    let old = initialized_vault();
    let id = read_vault_id(&old);
    let new = old.parent().unwrap().join("moved");
    std::fs::rename(&old, &new).unwrap();
    assert!(resolve_vault(&id.to_string()).is_err());
    rebind_vault(id, &new).unwrap();
    assert_eq!(resolve_vault(&id.to_string()).unwrap().root, new);
}
```

- [ ] **Step 2: Verify the test fails**

Run `cargo test -p kb-app --test registry`.

Expected: registry functions are absent.

- [ ] **Step 3: Implement the user-level registry**

Store `<config_dir>/vaults.yml` with `schema_version` and a sequence of `{ vault_id, path }`. Native absolute paths are permitted only in this machine-local file. Registry updates acquire an exclusive lock on `<state_dir>/registry.lock`, verify that the selected path contains the same `vault_id`, then replace the YAML atomically.

After a successful `kb init`, register the new Vault. Registration failure is reported separately and does not remove a valid initialized Vault; the response includes `kb vault register <path>` as the corrective action.

- [ ] **Step 4: Define deterministic Vault selection**

Selection order is explicit `--vault`, `KB_VAULT`, ancestor discovery from current directory, then the sole registered Vault. More than one registered Vault without another selector returns `vault_not_found` with a next action listing `kb vault list`; it never chooses by recency.

- [ ] **Step 5: Add CLI commands**

Implement:

```text
kb vault list
kb vault register <path>
kb vault rebind <vault-id> <path>
kb vault unregister <vault-id>
kb paths [--vault <path-or-id>]
```

Unregister changes only the user registry and never deletes Vault data.

- [ ] **Step 6: Run module and real-entry tests**

Run:

```bash
cargo test -p kb-app --test registry
cargo test -p kb-e2e --test vault
```

Expected: tests use overridden user directories, move the Vault, observe the stale registration, rebind it, and reopen it by UUID.

- [ ] **Step 7: Commit Vault registration and relocation**

Run `git add crates/kb-app tests/e2e/tests/vault.rs && git commit -m "feat: register and relocate vaults"`.

### Task 9: Plan and apply adoption without touching existing content

**Files:**

- Create: `crates/kb-core/src/operation.rs`
- Modify: `crates/kb-core/src/lib.rs`
- Create: `crates/kb-app/src/operation.rs`
- Create: `crates/kb-app/src/adopt.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-app/tests/adopt.rs`
- Create: `tests/e2e/tests/adopt.rs`

**Interfaces:**

- Consumes: minimum template, user state directory, path collision checks, registry and atomic writes.
- Produces: `OperationId`, `AdoptionPlan`, `create_adoption_plan`, `inspect_operation`, and `apply_operation` for the `adopt_vault` operation kind.

- [ ] **Step 1: Write a failing no-mutation planning test**

```rust
#[test]
fn adopt_planning_does_not_change_the_target() {
    let existing = directory_with_file("Notes/keep.md", b"human text\n");
    let before = snapshot_tree(&existing);
    let plan = create_adoption_plan(&existing).unwrap();
    assert_eq!(snapshot_tree(&existing), before);
    assert_eq!(plan.kind, OperationKind::AdoptVault);
    assert!(plan.creates.iter().all(|path| !path.starts_with("Notes/")));
}
```

- [ ] **Step 2: Verify the test fails**

Run `cargo test -p kb-app --test adopt`.

Expected: adoption planning types are absent.

- [ ] **Step 3: Define the adoption plan**

The JSON plan stored under `<state_dir>/operations/<operation_id>/plan.json` contains:

```rust
pub struct AdoptionPlan {
    pub schema_version: SchemaVersion,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub target: PathBuf,
    pub observed_entries: Vec<ObservedEntry>,
    pub creates: Vec<PlannedFile>,
    pub created_at: String,
    pub app_version: String,
}

pub struct ObservedEntry {
    pub relative_path: PortableRelativePath,
    pub kind: ObservedKind,
    pub size: u64,
    pub sha256: Option<String>,
}
```

Hash every existing regular file, record directories, and reject links, junctions, portability collisions, existing `.kb`, existing `admission.yml`, or existing `KB.md` whose ownership cannot be proven. The plan creates only the minimum framework files; it does not list existing top-level directories in `admission.yml`.

- [ ] **Step 4: Implement adoption-only apply**

`kb apply <operation_id>` loads the user-state plan, acquires `<state_dir>/operations.lock`, re-snapshots the target, and rejects `plan_stale` before writing if any observed entry differs. It stages all new files under a private directory inside the target, validates them, moves only planned paths into place, verifies final bytes, writes `.kb/runtime/operations/<id>/result.json`, registers the Vault, and deletes the user-state plan only after success.

If application stops after some created paths appear, the next invocation removes only files whose hashes equal the plan's generated bytes. A changed or user-created file is never removed and produces `vault_needs_recovery` with its exact path.

- [ ] **Step 5: Verify stale input and idempotency**

Add tests that alter an existing note after planning, repeat a successful `operation_id`, and inject a failure after each planned create. Every assertion reopens the target and verifies either the original tree or the complete adopted tree; it does not trust the command's success field alone.

- [ ] **Step 6: Run module and real-entry adoption tests**

Run:

```bash
cargo test -p kb-app --test adopt
cargo test -p kb-e2e --test adopt
```

Expected: planning is read-only, stale plans make no target changes, apply preserves the note byte-for-byte, and repeated apply returns the stored result.

- [ ] **Step 7: Commit planned adoption**

Run `git add crates/kb-core crates/kb-app tests/e2e/tests/adopt.rs && git commit -m "feat: adopt existing directories through plans"`.

### Task 10: Wire the Stage 1 command surface, status and diagnostics

**Files:**

- Create: `crates/kb-app/src/status.rs`
- Create: `crates/kb-app/src/doctor.rs`
- Create: `crates/kb-app/src/capabilities.rs`
- Create: `crates/kb-app/src/lock.rs`
- Modify: `crates/kb-app/src/lib.rs`
- Create: `crates/kb-cli/src/args.rs`
- Create: `crates/kb-cli/src/render.rs`
- Modify: `crates/kb-cli/src/main.rs`
- Create: `tests/e2e/tests/json_contract.rs`
- Create: `tests/e2e/tests/doctor.rs`
- Modify: `tests/e2e/tests/vault_discovery.rs`

**Interfaces:**

- Consumes: all Stage 1 application functions and protocol envelopes.
- Produces: human and JSON forms of `init`, `adopt`, `apply`, `config`, `status`, `doctor`, `vault`, `paths`, `version`, and `capabilities`.

- [ ] **Step 1: Write failing JSON-contract tests**

```rust
#[test]
fn invalid_config_is_stdout_clean_and_machine_readable() {
    let output = kb(&["status", "--vault", fixture("broken"), "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["schema_version"], "v1.0");
    assert_eq!(body["error"]["code"], "invalid_config");
}
```

- [ ] **Step 2: Verify the tests fail**

Run `cargo test -p kb-e2e --test json_contract`.

Expected: commands or response wiring are absent.

- [ ] **Step 3: Implement one application request dispatcher**

```rust
pub enum AppRequest {
    Init(InitRequest),
    Adopt(AdoptRequest),
    Apply(ApplyRequest),
    Config(ConfigRequest),
    Status(StatusRequest),
    Doctor(DoctorRequest),
    Vault(VaultRequest),
    Paths(PathsRequest),
    Version,
    Capabilities,
}

pub fn run(request: AppRequest, context: &AppContext) -> Result<AppResponse, KbError>;
```

CLI parsing maps arguments into this enum and never reads or writes Vault files directly.

- [ ] **Step 4: Define status and doctor distinctions**

`status` reports selected Vault root/id, schema compatibility, configuration validity, admitted enabled/disabled counts, recovery state, and cache state. `doctor` reports separate checks for standard directories, Vault readability, Vault writability, path portability, configuration, lock acquisition, recovery records and optional host integrations. Each check has `pass`, `warn`, `fail`, or `not_checked`; there is no aggregate health score.

For an older migratable schema, only diagnostic/read paths are available and mutation commands return a migration-required error. For a newer minor schema, only `status`, `doctor`, and the future backup command are allowed. A newer major schema is not interpreted beyond diagnostic metadata. Add JSON-contract cases for all three classifications.

`doctor` is read-only except that its lock test may create and remove its own empty lock file under `.kb/runtime/`. It must never edit configuration or Wiki files.

- [ ] **Step 5: Add a cross-platform Vault lock wrapper**

Use `fs2::FileExt` over `.kb/runtime/vault.lock` and an RAII guard. Read commands take shared locks; Stage 1 mutation commands take exclusive locks. A nonblocking collision returns `write_busy` with `.kb/runtime/vault-lock-info.json` used only for command, process ID, optional operation ID and start time display. The OS lock, not the JSON file, decides ownership.

- [ ] **Step 6: Publish explicit capabilities**

Stage 1 capabilities report `direct_search: false`, `bm25: false`, no extractors, `mcp: false`, `http: false`, and the implemented Stage 1 command list. Clients must not infer these values from `0.1.0`.

- [ ] **Step 7: Finish real-entry tests**

Remove the ignore from `vault_discovery.rs`. Run:

```bash
cargo test -p kb-e2e --test vault_discovery
cargo test -p kb-e2e --test json_contract
cargo test -p kb-e2e --test doctor
```

Expected: tests launch the built binary, parse only stdout in JSON mode, re-read filesystem state, and verify nonzero process exit codes for errors.

- [ ] **Step 8: Commit the Stage 1 command surface**

Run `git add crates/kb-app crates/kb-cli tests/e2e && git commit -m "feat: expose vault status and diagnostics"`.

### Task 11: Prove Stage 1 across operating systems and after relocation

**Files:**

- Create: `.github/workflows/phase-1.yml`
- Create: `tests/e2e/tests/phase1_journey.rs`
- Create: `tests/e2e/tests/portable_names.rs`
- Create: `scripts/check-stage-1.sh`
- Create: `scripts/check-stage-1.ps1`

**Interfaces:**

- Consumes: built `kb` and all Stage 1 commands.
- Produces: repeatable local gates plus Windows/macOS/Linux CI evidence.

- [ ] **Step 1: Write the complete user-journey test**

The test must execute the built binary in this order:

```text
kb init <new-vault> --json
kb config admission add notes Notes --vault <new-vault> --yes --json
kb config show --sources --vault <new-vault> --json
kb status --vault <new-vault> --json
move <new-vault> <moved-vault> using the test process filesystem API
kb vault rebind <vault-id> <moved-vault> --json
kb status --vault <vault-id> --json
kb doctor --vault <vault-id> --json
```

Before adding admission, the test creates the `Notes` directory because admission never creates user topic directories. After every mutation it starts a new `kb` process and re-reads files from disk.

- [ ] **Step 2: Add the adoption journey**

Create an existing directory with Markdown and a disabled personal folder, snapshot it, run `kb adopt`, verify the snapshot is unchanged, run `kb apply`, verify existing bytes are unchanged, and reopen through `kb status`.

- [ ] **Step 3: Add platform-specific name cases**

Shared cases cover `/` serialization, reserved Windows names, trailing spaces/dots, case collisions and NFC collisions. Unix creates a symlink fixture; Windows creates a junction fixture. Each fixture is skipped only when the operating system denies creating that fixture, and the skip reason is printed explicitly.

- [ ] **Step 4: Create local gate scripts**

Both scripts run the same logical gate:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release -p kb-cli
run the release binary for version, capabilities, init, move, reopen, adopt-plan and adopt-apply
```

Shell and PowerShell scripts must create private temporary directories, quote all paths, and delete only directories they created.

- [ ] **Step 5: Configure the operating-system matrix**

The workflow uses `ubuntu-latest`, `macos-latest`, and `windows-latest`, executes the native gate script, and uploads the `kb` binary plus JSON journey output on failure. It does not publish a release or label a platform supported.

- [ ] **Step 6: Run the current-machine gate**

Run `bash scripts/check-stage-1.sh`.

Expected on the current macOS host: all checks exit 0. This proves only the macOS execution path; Windows and Linux remain unverified until their native jobs pass.

- [ ] **Step 7: Commit the Stage 1 journey gate**

Run `git add .github scripts tests/e2e && git commit -m "test: cover the Stage 1 vault journey"`. Leave the phase status unaccepted until native Windows and Linux evidence exists.

### Task 12: Publish Stage 1 reference and evaluate the phase gate

**Files:**

- Create: `README.md`
- Create: `docs/architecture/overview.md`
- Create: `docs/reference/configuration.md`
- Create: `docs/reference/commands.md`
- Create: `docs/guides/create-a-vault.md`
- Create: `docs/guides/adopt-an-existing-directory.md`
- Create: `SECURITY.md`
- Modify: `docs/decisions/proposed/architecture/0001-independent-rust-core.md`
- Modify: `docs/decisions/proposed/architecture/0002-files-are-the-knowledge-source-of-truth.md`
- Modify: `docs/decisions/proposed/architecture/0003-admission-list.md`
- Modify: `docs/decisions/proposed/architecture/0008-one-application-layer.md`

**Interfaces:**

- Consumes: shipped Stage 1 behavior and all test evidence.
- Produces: user-facing tutorial/reference split and an honest Stage 1 status.

- [ ] **Step 1: Write documentation from the real command output**

`README.md` contains positioning, supported scope, quick start and links only. `docs/architecture/overview.md` owns the system map and crate responsibilities. `docs/reference/configuration.md` owns precedence, keys, files and failure semantics. `docs/reference/commands.md` owns Stage 1 syntax generated or checked from clap. Guides contain numbered outcome-oriented steps and verification commands; they link to reference pages instead of restating every field.

- [ ] **Step 2: Add security boundaries**

`SECURITY.md` states that local filesystem permissions define CLI authority, source/Wiki text is untrusted data, links and junctions are rejected at managed boundaries, the product has no telemetry or automatic networking in Stage 1, and vulnerability reports must not include private Vault content.

- [ ] **Step 3: Run documentation consistency checks**

Run:

```bash
rg -n '/Users/|/var/folders/|AI-Toolkit|Codex-only|macOS-only' README.md SECURITY.md assets schemas crates docs/architecture docs/reference docs/guides
rg -n 'TO''DO|TB''D|fill'' in|implement'' later' README.md SECURITY.md assets schemas crates docs/architecture docs/reference docs/guides docs/decisions
```

Expected: no matches. The product and templates contain no personal paths, personal topic names or unfinished markers.

- [ ] **Step 4: Re-run the smallest complete Stage 1 gate**

Run `bash scripts/check-stage-1.sh` and save the command output in the task report, not in the repository.

Expected: all macOS checks pass, including the release binary journey.

- [ ] **Step 5: Update ADR lifecycle only for implemented decisions**

Move ADRs 0001, 0002, 0003 and 0008 to `docs/decisions/accepted/architecture/` only when the implementation and required tests demonstrate their acceptance criteria. Change `Status` to `accepted / implemented`, replace future-tense proposal language with present-tense `Decision`, replace acceptance criteria with `Consequences`, and preserve alternatives. Leave ADRs 0004–0007 and 0009 proposed until their later phases ship.

- [ ] **Step 6: Evaluate rather than overstate cross-platform completion**

Stage 1 is accepted only when the same workflow passes on native Windows, macOS and Linux. If only local macOS evidence exists, report “macOS path verified; Windows and Linux pending native execution” and do not call Stage 1 complete, cross-platform verified, or release-ready.

- [ ] **Step 7: Commit Stage 1 documentation**

Run `git add README.md SECURITY.md docs && git commit -m "docs: document the Vault foundation"`.

## Phase Boundary

After Task 12, stop before Stage 2. Stage 2 requires a separate implementation plan for source discovery, content-addressed evidence, extractors, direct Markdown search and the lightweight catalog. It must build on the accepted Stage 1 public interfaces rather than expanding CLI adapters with business logic.
