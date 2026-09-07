use std::{fs, net::SocketAddr, path::PathBuf, str::FromStr};

use clap::{ArgGroup, Args, Parser, Subcommand};
use kb_app::{
    AdmissionAction, AdmissionRequest, AppRequest, BackupRequest, ConfigRequest, ConfigTarget,
    InitRequest, OperationRequest, SkillRequest, VaultRequest,
};
use kb_core::{KnowledgePlanRequest, OperationId, SkillHost, SkillInstallMode, SkillScope};
use uuid::Uuid;

pub(crate) enum ParsedCommand {
    App {
        request: Result<AppRequest, kb_core::KbError>,
        json: bool,
        fail_on_findings: bool,
    },
    Serve(ServeCommand),
}

pub(crate) struct ServeCommand {
    pub bind: SocketAddr,
    pub token_file: Option<PathBuf>,
    pub allow_write: bool,
    pub vault: Option<String>,
}

pub(crate) fn parse() -> ParsedCommand {
    Cli::parse().into_command()
}

#[derive(Parser)]
#[command(
    name = "kb",
    version,
    about = "Knowledge-Brain portable knowledge vault"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve the selected Vault over an optional HTTP adapter.
    Serve {
        /// Address to listen on; non-loopback addresses require a token file.
        #[arg(long, default_value = "127.0.0.1:9432")]
        bind: SocketAddr,
        /// File containing one Bearer token value.
        #[arg(long)]
        token_file: Option<PathBuf>,
        /// Permit applying reviewed operations through HTTP.
        #[arg(long)]
        allow_write: bool,
        /// Vault path or registered stable ID.
        #[arg(long)]
        vault: Option<String>,
    },
    /// Create, verify, or restore portable ZIP backups.
    Backup {
        #[command(subcommand)]
        command: BackupCommands,
    },
    /// Review changes in enabled admission directories.
    Review {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Search Wiki Markdown or captured sources.
    Query {
        query: String,
        #[arg(long, default_value="wiki", value_parser=["wiki","sources","all"])]
        scope: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Fail instead of falling back when the selected backend is unavailable.
        #[arg(long)]
        strict_backend: bool,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Validate Wiki structure without modifying knowledge or caches.
    Lint {
        /// Exit non-zero when the report contains any finding.
        #[arg(long)]
        strict: bool,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Create a reviewable knowledge save plan from a structured JSON request.
    Plan {
        #[command(subcommand)]
        command: PlanCommands,
    },
    /// Maintain derived caches.
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
    /// Inspect captured source evidence.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Install and inspect the portable Knowledge-Brain Agent Skill.
    Skills {
        #[command(subcommand)]
        command: SkillCommands,
    },
    /// Create a minimum Vault in a nonexistent or empty directory.
    Init {
        target: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Read or edit layered Vault configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Manage machine-local Vault registrations.
    Vault {
        #[command(subcommand)]
        command: VaultCommands,
    },
    /// Show resolved paths for the selected Vault.
    Paths {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Review an existing directory and save an adoption plan.
    Adopt {
        target: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Apply one previously reviewed operation.
    Apply {
        operation_id: OperationId,
        #[arg(long)]
        json: bool,
    },
    /// Inspect stored operation plans and results.
    Operation {
        #[command(subcommand)]
        command: OperationCommands,
    },
    /// Report the selected Vault's factual state.
    Status {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Run independent diagnostics without assigning a health score.
    Doctor {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Show application and schema versions.
    Version {
        #[arg(long)]
        json: bool,
    },
    /// Show implemented and unavailable capabilities explicitly.
    Capabilities {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum BackupCommands {
    /// Create a verified ZIP without overwriting an existing output.
    Create {
        #[arg(long)]
        output: Option<PathBuf>,
        /// Omit immutable source objects and mark the backup incomplete.
        #[arg(long)]
        without_source_objects: bool,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Verify manifest paths, sizes and hashes without extracting.
    Verify {
        archive: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Restore a verified archive into a nonexistent or empty directory.
    Restore {
        archive: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum OperationCommands {
    /// Show a plan or completed result by operation ID.
    Show {
        operation_id: OperationId,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PlanCommands {
    /// Validate a JSON request and save a plan without changing the Vault.
    Create {
        request: KnowledgeRequestArg,
        #[command(flatten)]
        context: VaultContext,
    },
}

#[derive(Debug, Clone)]
enum KnowledgeRequestArg {
    Parsed(KnowledgePlanRequest),
    Invalid(kb_core::KbError),
}

impl FromStr for KnowledgeRequestArg {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let path = PathBuf::from(value);
        let parsed = fs::read(&path)
            .map_err(|error| {
                kb_core::KbError::io_failure(
                    "read knowledge request",
                    path.display().to_string(),
                    error.to_string(),
                )
            })
            .and_then(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| {
                    kb_core::KbError::invalid_config(path.display().to_string(), error.to_string())
                })
            });
        Ok(match parsed {
            Ok(request) => Self::Parsed(request),
            Err(error) => Self::Invalid(error),
        })
    }
}

#[derive(Subcommand)]
enum CacheCommands {
    Rebuild {
        #[command(flatten)]
        context: VaultContext,
    },
}
#[derive(Subcommand)]
enum SourceCommands {
    Verify {
        #[command(flatten)]
        context: VaultContext,
    },
}

#[derive(Subcommand)]
enum SkillCommands {
    /// Detect supported Agent hosts without changing files.
    Detect {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Create a reviewable Skill installation plan.
    Install {
        #[arg(long, default_value = "auto", value_parser = ["auto", "codex", "claude-code", "gemini-cli", "opencode"])]
        host: String,
        #[arg(long, default_value = "vault", value_parser = ["vault", "user"])]
        scope: String,
        #[arg(long, default_value = "copy", value_parser = ["copy", "symlink"])]
        mode: String,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Report whether the installed Skill matches the embedded version.
    Status {
        #[arg(long, default_value = "auto", value_parser = ["auto", "codex", "claude-code", "gemini-cli", "opencode"])]
        host: String,
        #[arg(long, default_value = "vault", value_parser = ["vault", "user"])]
        scope: String,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Create a reviewable plan that removes only unchanged managed files.
    Uninstall {
        #[arg(long, default_value = "auto", value_parser = ["auto", "codex", "claude-code", "gemini-cli", "opencode"])]
        host: String,
        #[arg(long, default_value = "vault", value_parser = ["vault", "user"])]
        scope: String,
        #[command(flatten)]
        context: VaultContext,
    },
}

#[derive(Subcommand)]
enum VaultCommands {
    /// List registered Vaults without opening them.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Register an initialized Vault.
    Register {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Update the path for a registered stable Vault ID.
    Rebind {
        vault_id: Uuid,
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Forget a registration without deleting Vault data.
    Unregister {
        vault_id: Uuid,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show the effective configuration.
    Show {
        #[arg(long)]
        sources: bool,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Read one effective configuration value.
    Get {
        key: String,
        #[command(flatten)]
        context: VaultContext,
    },
    /// Preview or set one configuration value.
    Set {
        key: String,
        value: String,
        #[command(flatten)]
        context: VaultContext,
        #[command(flatten)]
        layer: LayerSelection,
        #[arg(long)]
        yes: bool,
    },
    /// Preview or remove one configuration value from a layer.
    Unset {
        key: String,
        #[command(flatten)]
        context: VaultContext,
        #[command(flatten)]
        layer: LayerSelection,
        #[arg(long)]
        yes: bool,
    },
    /// Validate all configuration layers and admission entries.
    Validate {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Manage the human-readable admission list.
    Admission {
        #[command(subcommand)]
        command: AdmissionCommands,
    },
}

#[derive(Subcommand)]
enum AdmissionCommands {
    /// List every admitted top-level directory.
    List {
        #[command(flatten)]
        context: VaultContext,
    },
    /// Add an enabled top-level directory by stable ID.
    Add {
        id: String,
        path: String,
        #[command(flatten)]
        context: VaultContext,
        #[arg(long)]
        yes: bool,
    },
    /// Enable one admission entry by ID.
    Enable {
        id: String,
        #[command(flatten)]
        context: VaultContext,
        #[arg(long)]
        yes: bool,
    },
    /// Disable one admission entry by ID.
    Disable {
        id: String,
        #[command(flatten)]
        context: VaultContext,
        #[arg(long)]
        yes: bool,
    },
    /// Remove one admission entry without deleting its directory.
    Remove {
        id: String,
        #[command(flatten)]
        context: VaultContext,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Args)]
struct VaultContext {
    /// Vault path or registered stable ID.
    #[arg(long)]
    vault: Option<String>,
    /// Emit one stable JSON response on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Args)]
#[command(group(
    ArgGroup::new("layer")
        .args(["local", "user"])
        .multiple(false)
))]
struct LayerSelection {
    /// Edit the machine-local Vault layer.
    #[arg(long)]
    local: bool,
    /// Edit the operating-system user layer.
    #[arg(long)]
    user: bool,
}

impl LayerSelection {
    const fn target(&self) -> ConfigTarget {
        if self.local {
            ConfigTarget::Local
        } else if self.user {
            ConfigTarget::User
        } else {
            ConfigTarget::Vault
        }
    }
}

impl Cli {
    fn into_command(self) -> ParsedCommand {
        match self.command {
            Commands::Serve {
                bind,
                token_file,
                allow_write,
                vault,
            } => serve_command(bind, token_file, allow_write, vault),
            Commands::Backup { command } => backup_command(command),
            Commands::Review { context } => review_command(context),
            Commands::Query {
                query,
                scope,
                limit,
                strict_backend,
                context,
            } => query_command(query, &scope, limit, strict_backend, context),
            Commands::Lint { strict, context } => lint_command(strict, context),
            Commands::Plan {
                command: PlanCommands::Create { request, context },
            } => plan_command(request, context),
            Commands::Cache {
                command: CacheCommands::Rebuild { context },
            } => ParsedCommand::App {
                request: Ok(AppRequest::CacheRebuild {
                    vault: context.vault,
                }),
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Source {
                command: SourceCommands::Verify { context },
            } => ParsedCommand::App {
                request: Ok(AppRequest::SourceVerify {
                    vault: context.vault,
                }),
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Skills { command } => skill_command(command),
            Commands::Init { target, json } => ParsedCommand::App {
                request: Ok(AppRequest::Init(InitRequest { target })),
                json,
                fail_on_findings: false,
            },
            Commands::Adopt { target, json } => ParsedCommand::App {
                request: Ok(AppRequest::Adopt { target }),
                json,
                fail_on_findings: false,
            },
            Commands::Apply { operation_id, json } => ParsedCommand::App {
                request: Ok(AppRequest::Apply { operation_id }),
                json,
                fail_on_findings: false,
            },
            Commands::Operation { command } => match command {
                OperationCommands::Show { operation_id, json } => ParsedCommand::App {
                    request: Ok(AppRequest::Operation(OperationRequest::Show {
                        operation_id,
                    })),
                    json,
                    fail_on_findings: false,
                },
            },
            Commands::Config { command } => config_command(command),
            Commands::Vault { command } => vault_command(command),
            Commands::Paths { context } => ParsedCommand::App {
                request: Ok(AppRequest::Paths {
                    vault: context.vault,
                }),
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Status { context } => ParsedCommand::App {
                request: Ok(AppRequest::Status {
                    vault: context.vault,
                }),
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Doctor { context } => ParsedCommand::App {
                request: Ok(AppRequest::Doctor {
                    vault: context.vault,
                }),
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Version { json } => ParsedCommand::App {
                request: Ok(AppRequest::Version),
                json,
                fail_on_findings: false,
            },
            Commands::Capabilities { json } => ParsedCommand::App {
                request: Ok(AppRequest::Capabilities),
                json,
                fail_on_findings: false,
            },
        }
    }
}

fn skill_command(command: SkillCommands) -> ParsedCommand {
    let (request, json) = match command {
        SkillCommands::Detect { context } => (
            Ok(AppRequest::Skills(SkillRequest::Detect {
                vault: context.vault,
            })),
            context.json,
        ),
        SkillCommands::Install {
            host,
            scope,
            mode,
            context,
        } => (
            parse_skill_host(&host).map(|host| {
                AppRequest::Skills(SkillRequest::Install {
                    vault: context.vault,
                    host,
                    scope: parse_skill_scope(&scope),
                    mode: parse_skill_mode(&mode),
                })
            }),
            context.json,
        ),
        SkillCommands::Status {
            host,
            scope,
            context,
        } => (
            parse_skill_host(&host).map(|host| {
                AppRequest::Skills(SkillRequest::Status {
                    vault: context.vault,
                    host,
                    scope: parse_skill_scope(&scope),
                })
            }),
            context.json,
        ),
        SkillCommands::Uninstall {
            host,
            scope,
            context,
        } => (
            parse_skill_host(&host).map(|host| {
                AppRequest::Skills(SkillRequest::Uninstall {
                    vault: context.vault,
                    host,
                    scope: parse_skill_scope(&scope),
                })
            }),
            context.json,
        ),
    };
    ParsedCommand::App {
        request,
        json,
        fail_on_findings: false,
    }
}

fn parse_skill_host(value: &str) -> Result<Option<SkillHost>, kb_core::KbError> {
    Ok(match value {
        "auto" => None,
        "codex" => Some(SkillHost::Codex),
        "claude-code" => Some(SkillHost::ClaudeCode),
        "gemini-cli" => Some(SkillHost::GeminiCli),
        "opencode" => Some(SkillHost::OpenCode),
        _ => {
            return Err(kb_core::KbError::invalid_config(
                "Skill host",
                "unsupported host",
            ));
        }
    })
}

fn parse_skill_scope(value: &str) -> SkillScope {
    if value == "user" {
        SkillScope::User
    } else {
        SkillScope::Vault
    }
}

fn parse_skill_mode(value: &str) -> SkillInstallMode {
    if value == "symlink" {
        SkillInstallMode::Symlink
    } else {
        SkillInstallMode::Copy
    }
}

fn serve_command(
    bind: SocketAddr,
    token_file: Option<PathBuf>,
    allow_write: bool,
    vault: Option<String>,
) -> ParsedCommand {
    ParsedCommand::Serve(ServeCommand {
        bind,
        token_file,
        allow_write,
        vault,
    })
}

fn plan_command(request: KnowledgeRequestArg, context: VaultContext) -> ParsedCommand {
    ParsedCommand::App {
        request: match request {
            KnowledgeRequestArg::Parsed(request) => Ok(AppRequest::PlanCreate {
                vault: context.vault,
                request,
            }),
            KnowledgeRequestArg::Invalid(error) => Err(error),
        },
        json: context.json,
        fail_on_findings: false,
    }
}

fn backup_command(command: BackupCommands) -> ParsedCommand {
    let (request, json) = match command {
        BackupCommands::Create {
            output,
            without_source_objects,
            context,
        } => (
            BackupRequest::Create {
                vault: context.vault,
                output,
                without_source_objects,
            },
            context.json,
        ),
        BackupCommands::Verify { archive, json } => (BackupRequest::Verify { archive }, json),
        BackupCommands::Restore {
            archive,
            target,
            json,
        } => (BackupRequest::Restore { archive, target }, json),
    };
    ParsedCommand::App {
        request: Ok(AppRequest::Backup(request)),
        json,
        fail_on_findings: false,
    }
}

fn review_command(context: VaultContext) -> ParsedCommand {
    ParsedCommand::App {
        request: Ok(AppRequest::Review {
            vault: context.vault,
        }),
        json: context.json,
        fail_on_findings: false,
    }
}

fn query_command(
    query: String,
    scope: &str,
    limit: usize,
    strict_backend: bool,
    context: VaultContext,
) -> ParsedCommand {
    ParsedCommand::App {
        request: Ok(AppRequest::Query {
            vault: context.vault,
            request: kb_core::SearchRequest {
                query,
                limit,
                strict_backend,
                scope: match scope {
                    "sources" => kb_core::SearchScope::Sources,
                    "all" => kb_core::SearchScope::All,
                    _ => kb_core::SearchScope::Wiki,
                },
            },
        }),
        json: context.json,
        fail_on_findings: false,
    }
}

fn lint_command(strict: bool, context: VaultContext) -> ParsedCommand {
    ParsedCommand::App {
        request: Ok(AppRequest::Lint {
            vault: context.vault,
        }),
        json: context.json,
        fail_on_findings: strict,
    }
}

fn config_command(command: ConfigCommands) -> ParsedCommand {
    let (request, json) = match command {
        ConfigCommands::Show { sources, context } => (
            ConfigRequest::Show {
                vault: context.vault,
                sources,
            },
            context.json,
        ),
        ConfigCommands::Get { key, context } => (
            ConfigRequest::Get {
                vault: context.vault,
                key,
            },
            context.json,
        ),
        ConfigCommands::Set {
            key,
            value,
            context,
            layer,
            yes,
        } => (
            ConfigRequest::Set {
                vault: context.vault,
                target: layer.target(),
                key,
                value,
                write: yes,
            },
            context.json,
        ),
        ConfigCommands::Unset {
            key,
            context,
            layer,
            yes,
        } => (
            ConfigRequest::Unset {
                vault: context.vault,
                target: layer.target(),
                key,
                write: yes,
            },
            context.json,
        ),
        ConfigCommands::Validate { context } => (
            ConfigRequest::Validate {
                vault: context.vault,
            },
            context.json,
        ),
        ConfigCommands::Admission { command } => return admission_command(command),
    };
    ParsedCommand::App {
        request: Ok(AppRequest::Config(request)),
        json,
        fail_on_findings: false,
    }
}

fn admission_command(command: AdmissionCommands) -> ParsedCommand {
    let (vault, request, json) = match command {
        AdmissionCommands::List { context } => {
            (context.vault, AdmissionRequest::List, context.json)
        }
        AdmissionCommands::Add {
            id,
            path,
            context,
            yes,
        } => (
            context.vault,
            AdmissionRequest::Change {
                action: AdmissionAction::Add { id, path },
                write: yes,
            },
            context.json,
        ),
        AdmissionCommands::Enable { id, context, yes } => (
            context.vault,
            AdmissionRequest::Change {
                action: AdmissionAction::Enable { id },
                write: yes,
            },
            context.json,
        ),
        AdmissionCommands::Disable { id, context, yes } => (
            context.vault,
            AdmissionRequest::Change {
                action: AdmissionAction::Disable { id },
                write: yes,
            },
            context.json,
        ),
        AdmissionCommands::Remove { id, context, yes } => (
            context.vault,
            AdmissionRequest::Change {
                action: AdmissionAction::Remove { id },
                write: yes,
            },
            context.json,
        ),
    };
    ParsedCommand::App {
        request: Ok(AppRequest::Config(ConfigRequest::Admission {
            vault,
            request,
        })),
        json,
        fail_on_findings: false,
    }
}

fn vault_command(command: VaultCommands) -> ParsedCommand {
    let (request, json) = match command {
        VaultCommands::List { json } => (VaultRequest::List, json),
        VaultCommands::Register { path, json } => (VaultRequest::Register { path }, json),
        VaultCommands::Rebind {
            vault_id,
            path,
            json,
        } => (VaultRequest::Rebind { vault_id, path }, json),
        VaultCommands::Unregister { vault_id, json } => {
            (VaultRequest::Unregister { vault_id }, json)
        }
    };
    ParsedCommand::App {
        request: Ok(AppRequest::Vault(request)),
        json,
        fail_on_findings: false,
    }
}
