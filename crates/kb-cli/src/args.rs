use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};
use kb_app::{
    AdmissionAction, AdmissionRequest, AppRequest, ConfigRequest, ConfigTarget, InitRequest,
    OperationRequest, VaultRequest,
};
use kb_core::OperationId;
use uuid::Uuid;

pub(crate) struct ParsedCommand {
    pub request: AppRequest,
    pub json: bool,
    pub fail_on_findings: bool,
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
enum OperationCommands {
    /// Show a plan or completed result by operation ID.
    Show {
        operation_id: OperationId,
        #[arg(long)]
        json: bool,
    },
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
            Commands::Review { context } => ParsedCommand {
                request: AppRequest::Review {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Query {
                query,
                scope,
                limit,
                context,
            } => ParsedCommand {
                request: AppRequest::Query {
                    vault: context.vault,
                    request: kb_core::SearchRequest {
                        query,
                        limit,
                        scope: match scope.as_str() {
                            "sources" => kb_core::SearchScope::Sources,
                            "all" => kb_core::SearchScope::All,
                            _ => kb_core::SearchScope::Wiki,
                        },
                    },
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Lint { strict, context } => ParsedCommand {
                request: AppRequest::Lint {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: strict,
            },
            Commands::Cache {
                command: CacheCommands::Rebuild { context },
            } => ParsedCommand {
                request: AppRequest::CacheRebuild {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Source {
                command: SourceCommands::Verify { context },
            } => ParsedCommand {
                request: AppRequest::SourceVerify {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Init { target, json } => ParsedCommand {
                request: AppRequest::Init(InitRequest { target }),
                json,
                fail_on_findings: false,
            },
            Commands::Adopt { target, json } => ParsedCommand {
                request: AppRequest::Adopt { target },
                json,
                fail_on_findings: false,
            },
            Commands::Apply { operation_id, json } => ParsedCommand {
                request: AppRequest::Apply { operation_id },
                json,
                fail_on_findings: false,
            },
            Commands::Operation { command } => match command {
                OperationCommands::Show { operation_id, json } => ParsedCommand {
                    request: AppRequest::Operation(OperationRequest::Show { operation_id }),
                    json,
                    fail_on_findings: false,
                },
            },
            Commands::Config { command } => config_command(command),
            Commands::Vault { command } => vault_command(command),
            Commands::Paths { context } => ParsedCommand {
                request: AppRequest::Paths {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Status { context } => ParsedCommand {
                request: AppRequest::Status {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Doctor { context } => ParsedCommand {
                request: AppRequest::Doctor {
                    vault: context.vault,
                },
                json: context.json,
                fail_on_findings: false,
            },
            Commands::Version { json } => ParsedCommand {
                request: AppRequest::Version,
                json,
                fail_on_findings: false,
            },
            Commands::Capabilities { json } => ParsedCommand {
                request: AppRequest::Capabilities,
                json,
                fail_on_findings: false,
            },
        }
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
    ParsedCommand {
        request: AppRequest::Config(request),
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
    ParsedCommand {
        request: AppRequest::Config(ConfigRequest::Admission { vault, request }),
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
    ParsedCommand {
        request: AppRequest::Vault(request),
        json,
        fail_on_findings: false,
    }
}
