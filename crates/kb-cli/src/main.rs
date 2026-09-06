use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use clap::{ArgGroup, Args, Parser, Subcommand};
use kb_app::{
    AdmissionAction, ConfigOverrides, ConfigTarget, InitRequest, UserPaths, VaultSelection,
    admission_change, config_get, config_set, config_show, config_unset, config_validate,
    init_and_register_vault, init_vault, list_vaults, load_admission, rebind_vault, register_vault,
    resolve_vault, unregister_vault,
};
use kb_core::KbError;
use kb_protocol::{Envelope, ErrorEnvelope};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

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
    /// Create a minimum Vault in a nonexistent or empty directory.
    Init {
        target: PathBuf,
        /// Emit one stable JSON response on stdout.
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
        /// Include the winning source for every value.
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
        /// Save the proposed change. Without this flag, only show a preview.
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
        /// Save the proposed change. Without this flag, only show a preview.
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
        /// Save the proposed change. Without this flag, only show a preview.
        #[arg(long)]
        yes: bool,
    },
    /// Enable one admission entry by ID.
    Enable {
        id: String,
        #[command(flatten)]
        context: VaultContext,
        /// Save the proposed change. Without this flag, only show a preview.
        #[arg(long)]
        yes: bool,
    },
    /// Disable one admission entry by ID.
    Disable {
        id: String,
        #[command(flatten)]
        context: VaultContext,
        /// Save the proposed change. Without this flag, only show a preview.
        #[arg(long)]
        yes: bool,
    },
    /// Remove one admission entry without deleting its directory.
    Remove {
        id: String,
        #[command(flatten)]
        context: VaultContext,
        /// Save the proposed change. Without this flag, only show a preview.
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json_output = command_wants_json(&cli.command);
    match dispatch(cli.command) {
        Ok(value) => render_success(&value, json_output),
        Err(error) => render_error(error, json_output),
    }
}

fn dispatch(command: Commands) -> Result<Value, KbError> {
    match command {
        Commands::Init { target, .. } => {
            let request = InitRequest { target };
            let environment = environment();
            let mut report = match UserPaths::resolve(&environment) {
                Ok(paths) => init_and_register_vault(&request, &paths)?,
                Err(error) => {
                    let mut report = init_vault(&request)?;
                    report.warnings.push(format!(
                        "Vault initialized but not registered: {error} Run kb vault register {}.",
                        report.root.display()
                    ));
                    report
                }
            };
            report.warnings.sort();
            to_value(report)
        }
        Commands::Config { command } => dispatch_config(command),
        Commands::Vault { command } => dispatch_vault(command),
        Commands::Paths { context } => {
            let (_, resolved) = resolve_context(&context)?;
            Ok(serde_json::json!({
                "vault_id": resolved.vault_id,
                "root": resolved.root,
                "config": resolved.root.join(".kb/config.yml"),
                "local_config": resolved.root.join(".kb/config.local.yml"),
                "admission": resolved.root.join("admission.yml"),
                "wiki": resolved.root.join("Wiki"),
                "runtime": resolved.root.join(".kb/runtime"),
                "cache": resolved.root.join(".kb/cache"),
            }))
        }
    }
}

fn dispatch_config(command: ConfigCommands) -> Result<Value, KbError> {
    match command {
        ConfigCommands::Show { sources, context } => {
            let (paths, overrides) = runtime_config()?;
            let vault = resolve_with_paths(&context, &paths, &overrides.environment)?;
            config_show(&vault.root, &paths, &overrides, sources)
        }
        ConfigCommands::Get { key, context } => {
            let (paths, overrides) = runtime_config()?;
            let vault = resolve_with_paths(&context, &paths, &overrides.environment)?;
            config_get(&vault.root, &paths, &overrides, &key)
        }
        ConfigCommands::Set {
            key,
            value,
            context,
            layer,
            yes,
        } => {
            let (paths, overrides) = runtime_config()?;
            let vault = resolve_with_paths(&context, &paths, &overrides.environment)?;
            to_value(config_set(
                &vault.root,
                &paths,
                &overrides,
                layer.target(),
                &key,
                &value,
                yes,
            )?)
        }
        ConfigCommands::Unset {
            key,
            context,
            layer,
            yes,
        } => {
            let (paths, overrides) = runtime_config()?;
            let vault = resolve_with_paths(&context, &paths, &overrides.environment)?;
            to_value(config_unset(
                &vault.root,
                &paths,
                &overrides,
                layer.target(),
                &key,
                yes,
            )?)
        }
        ConfigCommands::Validate { context } => {
            let (paths, overrides) = runtime_config()?;
            let vault = resolve_with_paths(&context, &paths, &overrides.environment)?;
            to_value(config_validate(&vault.root, &paths, &overrides)?)
        }
        ConfigCommands::Admission { command } => dispatch_admission(command),
    }
}

fn dispatch_admission(command: AdmissionCommands) -> Result<Value, KbError> {
    match command {
        AdmissionCommands::List { context } => {
            let (_, vault) = resolve_context(&context)?;
            to_value(load_admission(&vault.root)?)
        }
        AdmissionCommands::Add {
            id,
            path,
            context,
            yes,
        } => {
            let (_, vault) = resolve_context(&context)?;
            to_value(admission_change(
                &vault.root,
                &AdmissionAction::Add { id, path },
                yes,
            )?)
        }
        AdmissionCommands::Enable { id, context, yes } => {
            let (_, vault) = resolve_context(&context)?;
            to_value(admission_change(
                &vault.root,
                &AdmissionAction::Enable { id },
                yes,
            )?)
        }
        AdmissionCommands::Disable { id, context, yes } => {
            let (_, vault) = resolve_context(&context)?;
            to_value(admission_change(
                &vault.root,
                &AdmissionAction::Disable { id },
                yes,
            )?)
        }
        AdmissionCommands::Remove { id, context, yes } => {
            let (_, vault) = resolve_context(&context)?;
            to_value(admission_change(
                &vault.root,
                &AdmissionAction::Remove { id },
                yes,
            )?)
        }
    }
}

fn dispatch_vault(command: VaultCommands) -> Result<Value, KbError> {
    let paths = UserPaths::resolve(&environment())?;
    match command {
        VaultCommands::List { .. } => Ok(serde_json::json!({
            "vaults": list_vaults(&paths)?,
        })),
        VaultCommands::Register { path, .. } => to_value(register_vault(&paths, &path)?),
        VaultCommands::Rebind { vault_id, path, .. } => {
            to_value(rebind_vault(&paths, vault_id, &path)?)
        }
        VaultCommands::Unregister { vault_id, .. } => {
            unregister_vault(&paths, vault_id)?;
            Ok(serde_json::json!({
                "vault_id": vault_id,
                "unregistered": true,
            }))
        }
    }
}

fn runtime_config() -> Result<(UserPaths, ConfigOverrides), KbError> {
    let environment = environment();
    let paths = UserPaths::resolve(&environment)?;
    Ok((
        paths,
        ConfigOverrides {
            environment,
            cli: BTreeMap::new(),
        },
    ))
}

fn resolve_context(context: &VaultContext) -> Result<(UserPaths, kb_app::ResolvedVault), KbError> {
    let environment = environment();
    let paths = UserPaths::resolve(&environment)?;
    let vault = resolve_with_paths(context, &paths, &environment)?;
    Ok((paths, vault))
}

fn resolve_with_paths(
    context: &VaultContext,
    paths: &UserPaths,
    environment: &BTreeMap<String, String>,
) -> Result<kb_app::ResolvedVault, KbError> {
    let current_dir = std::env::current_dir()
        .map_err(|error| KbError::io_failure("read current directory", ".", error.to_string()))?;
    resolve_vault(
        paths,
        &VaultSelection {
            explicit: context.vault.clone(),
            environment: environment.clone(),
            current_dir,
        },
    )
}

fn environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

fn to_value(value: impl Serialize) -> Result<Value, KbError> {
    serde_json::to_value(value)
        .map_err(|error| KbError::invalid_config("command response", error.to_string()))
}

fn render_success(value: &Value, json_output: bool) -> ExitCode {
    let rendered = if json_output {
        serde_json::to_string(&Envelope::new(value))
    } else if let Some(diff) = value.get("diff").and_then(Value::as_str) {
        Ok(diff.to_owned())
    } else {
        serde_json::to_string_pretty(value)
    };
    match rendered {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => render_error(
            KbError::invalid_config("command response", error.to_string()),
            json_output,
        ),
    }
}

fn render_error(error: KbError, json_output: bool) -> ExitCode {
    if json_output {
        match serde_json::to_string(&ErrorEnvelope::from(error)) {
            Ok(text) => println!("{text}"),
            Err(serialization_error) => eprintln!("Error: {serialization_error}"),
        }
    } else {
        eprintln!("Error: {error}");
    }
    ExitCode::FAILURE
}

fn command_wants_json(command: &Commands) -> bool {
    match command {
        Commands::Init { json, .. } => *json,
        Commands::Vault { command } => match command {
            VaultCommands::List { json }
            | VaultCommands::Register { json, .. }
            | VaultCommands::Rebind { json, .. }
            | VaultCommands::Unregister { json, .. } => *json,
        },
        Commands::Paths { context } => context.json,
        Commands::Config { command } => match command {
            ConfigCommands::Show { context, .. }
            | ConfigCommands::Get { context, .. }
            | ConfigCommands::Set { context, .. }
            | ConfigCommands::Unset { context, .. }
            | ConfigCommands::Validate { context } => context.json,
            ConfigCommands::Admission { command } => match command {
                AdmissionCommands::List { context }
                | AdmissionCommands::Add { context, .. }
                | AdmissionCommands::Enable { context, .. }
                | AdmissionCommands::Disable { context, .. }
                | AdmissionCommands::Remove { context, .. } => context.json,
            },
        },
    }
}
