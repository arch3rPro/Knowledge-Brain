use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use clap::{ArgGroup, Args, Parser, Subcommand};
use kb_app::{
    AdmissionAction, ConfigOverrides, ConfigTarget, InitRequest, UserPaths, admission_change,
    config_get, config_set, config_show, config_unset, config_validate, init_vault, load_admission,
};
use kb_core::KbError;
use kb_protocol::{Envelope, ErrorEnvelope};
use serde::Serialize;
use serde_json::Value;

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
    /// Vault directory. Registry IDs and discovery are added by the Vault task.
    #[arg(long)]
    vault: PathBuf,
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
        Commands::Init { target, .. } => to_value(init_vault(&InitRequest { target })?),
        Commands::Config { command } => dispatch_config(command),
    }
}

fn dispatch_config(command: ConfigCommands) -> Result<Value, KbError> {
    match command {
        ConfigCommands::Show { sources, context } => {
            let (paths, overrides) = runtime_config()?;
            config_show(&context.vault, &paths, &overrides, sources)
        }
        ConfigCommands::Get { key, context } => {
            let (paths, overrides) = runtime_config()?;
            config_get(&context.vault, &paths, &overrides, &key)
        }
        ConfigCommands::Set {
            key,
            value,
            context,
            layer,
            yes,
        } => {
            let (paths, overrides) = runtime_config()?;
            to_value(config_set(
                &context.vault,
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
            to_value(config_unset(
                &context.vault,
                &paths,
                &overrides,
                layer.target(),
                &key,
                yes,
            )?)
        }
        ConfigCommands::Validate { context } => {
            let (paths, overrides) = runtime_config()?;
            to_value(config_validate(&context.vault, &paths, &overrides)?)
        }
        ConfigCommands::Admission { command } => dispatch_admission(command),
    }
}

fn dispatch_admission(command: AdmissionCommands) -> Result<Value, KbError> {
    match command {
        AdmissionCommands::List { context } => to_value(load_admission(&context.vault)?),
        AdmissionCommands::Add {
            id,
            path,
            context,
            yes,
        } => to_value(admission_change(
            &context.vault,
            &AdmissionAction::Add { id, path },
            yes,
        )?),
        AdmissionCommands::Enable { id, context, yes } => to_value(admission_change(
            &context.vault,
            &AdmissionAction::Enable { id },
            yes,
        )?),
        AdmissionCommands::Disable { id, context, yes } => to_value(admission_change(
            &context.vault,
            &AdmissionAction::Disable { id },
            yes,
        )?),
        AdmissionCommands::Remove { id, context, yes } => to_value(admission_change(
            &context.vault,
            &AdmissionAction::Remove { id },
            yes,
        )?),
    }
}

fn runtime_config() -> Result<(UserPaths, ConfigOverrides), KbError> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let paths = UserPaths::resolve(&environment)?;
    Ok((
        paths,
        ConfigOverrides {
            environment,
            cli: BTreeMap::new(),
        },
    ))
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
