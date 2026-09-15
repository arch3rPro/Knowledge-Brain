use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, ExitCode, Stdio},
    sync::{Arc, Mutex},
};

use kb_app::{AppContext, AppRequest};
use kb_core::{ErrorCode, KbError};
use serde_json::{Value, json};

mod args;
mod render;

#[tokio::main]
async fn main() -> ExitCode {
    let parsed = args::parse();
    let current_dir = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            return render::error(
                KbError::io_failure("read current directory", ".", error.to_string()),
                false,
            );
        }
    };
    let context = AppContext::new(std::env::vars().collect::<BTreeMap<_, _>>(), current_dir);
    match parsed {
        args::ParsedCommand::App {
            request,
            json,
            fail_on_findings,
            full_hashes,
        } => match request.and_then(|request| run_app_request(request, &context)) {
            Ok(value) => render::success(&value, json, fail_on_findings, full_hashes),
            Err(error) => render::error(error, json),
        },
        args::ParsedCommand::Serve(command) => match run_server(command, context).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => render::error(error, false),
        },
        args::ParsedCommand::Mcp(command) => match run_mcp(command, context).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => render::error(error, false),
        },
        args::ParsedCommand::Update(command) => {
            let json = command.json;
            match run_update(command, &context) {
                Ok(value) => render::success(&value, json, false, false),
                Err(error) => render::error(error, json),
            }
        }
        args::ParsedCommand::UpdatePlan(command) => match run_target_plan(&command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        args::ParsedCommand::ReplaceUpdate(command) => match run_replace_update(&command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        args::ParsedCommand::ResumeUpdate(command) => {
            match kb_app::run(
                AppRequest::Update(Box::new(kb_app::UpdateRequest::Resume {
                    operation_id: command.operation_id,
                })),
                &context,
            ) {
                Ok(value) => render::success(&value, command.json, false, false),
                Err(error) => render::error(error, command.json),
            }
        }
        args::ParsedCommand::Replace(command) => {
            let json = command.json;
            let request = kb_update::ReplaceRequest::verified(
                command.from,
                &command.to,
                command.backup,
                command.expected_sha256,
            );
            match kb_update::wait_for_parent_exit(command.parent_pid, command.parent_start_time)
                .and_then(|()| kb_update::replace_with_backup(&request))
                .and_then(|()| {
                    schedule_cleanup(&command.to, &command.cleanup_dir, &command.cleanup_token)
                })
                .map_err(update_error)
            {
                Ok(()) => render::success(&json!({ "replaced": true }), json, false, false),
                Err(error) => render::error(error, json),
            }
        }
        args::ParsedCommand::Cleanup(command) => {
            match kb_update::wait_for_parent_exit(command.parent_pid, command.parent_start_time)
                .and_then(|()| kb_update::remove_update_stage(&command.directory, &command.token))
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::FAILURE,
            }
        }
    }
}

fn run_target_plan(command: &args::TargetPlanCommand) -> Result<(), KbError> {
    let request_bytes = fs::read(&command.request).map_err(|error| {
        KbError::io_failure(
            "read target update request",
            command.request.display().to_string(),
            error.to_string(),
        )
    })?;
    let request: kb_app::TargetPlanRequest =
        serde_json::from_slice(&request_bytes).map_err(|error| {
            KbError::invalid_config(command.request.display().to_string(), error.to_string())
        })?;
    let plan = kb_app::create_target_update_plan(&request)?;
    let output = serde_json::to_vec_pretty(&plan).map_err(|error| {
        KbError::invalid_config(command.output.display().to_string(), error.to_string())
    })?;
    kb_app::atomic_replace(&command.output, &output)
}

fn run_replace_update(command: &args::ReplaceUpdateCommand) -> Result<(), KbError> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let paths = kb_app::UserPaths::resolve(&environment)?;
    let store = kb_app::UpdateStore::new(&paths);
    store.transition(
        command.operation_id,
        kb_core::UpdateExecutionState::ReplacingCli,
    )?;
    let stage: kb_app::StoredUpdateStage = store.load_stage(command.operation_id)?;
    let operation = store.load(command.operation_id)?;
    let executable = operation
        .components
        .iter()
        .find(|component| {
            component.kind == kb_core::UpdateComponentKind::Executable
                && component.state == kb_core::UpdateComponentState::Pending
        })
        .and_then(|component| component.changes.first())
        .ok_or_else(|| KbError::invalid_config("update operation", "missing executable change"))?;
    let operation_dir = paths
        .state_dir
        .join("updates")
        .join(command.operation_id.to_string());
    let staged = operation_dir.join(&stage.executable_relative);
    let backup = operation_dir.join("cli.backup");
    let replacement = kb_update::ReplaceRequest::verified(
        staged,
        &executable.path,
        backup,
        stage.executable_sha256,
    );
    let replaced = kb_update::wait_for_parent_exit(command.parent_pid, command.parent_start_time)
        .and_then(|()| kb_update::replace_with_backup(&replacement));
    if let Err(error) = replaced {
        let _ = store.transition(command.operation_id, kb_core::UpdateExecutionState::Failed);
        return Err(update_error(error));
    }
    store.transition(
        command.operation_id,
        kb_core::UpdateExecutionState::CliReplaced,
    )?;
    scrubbed_command(&executable.path)
        .arg("__resume-update")
        .arg("--operation")
        .arg(command.operation_id.to_string())
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            KbError::io_failure(
                "start updated executable",
                executable.path.display().to_string(),
                error.to_string(),
            )
        })?;
    Ok(())
}

fn run_update(command: args::UpdateCommand, context: &AppContext) -> Result<Value, KbError> {
    let identity = compiled_identity()?;
    let executable = std::env::current_exe().map_err(|error| {
        KbError::io_failure("locate current executable", "process", error.to_string())
    })?;
    let context = context.clone().with_update_runtime(kb_app::UpdateRuntime {
        identity,
        executable,
    });
    let selection = kb_app::UpdateSelection {
        vault: command.vault,
        excluded_vaults: command.excluded_vaults,
        persist: !command.check_only,
    };
    kb_app::run(
        AppRequest::Update(Box::new(if command.check_only {
            kb_app::UpdateRequest::Check(selection)
        } else {
            kb_app::UpdateRequest::Prepare(selection)
        })),
        &context,
    )
}

fn schedule_cleanup(
    executable: &Path,
    directory: &Path,
    token: &str,
) -> Result<(), kb_update::UpdateError> {
    let parent_pid = std::process::id();
    let parent_start_time = kb_update::parent_process_start_time(parent_pid)?;
    scrubbed_command(executable)
        .arg("__cleanup")
        .arg("--parent-pid")
        .arg(parent_pid.to_string())
        .arg("--parent-start-time")
        .arg(parent_start_time.to_string())
        .arg("--directory")
        .arg(directory)
        .arg("--token")
        .arg(token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            kb_update::UpdateError::ReplacementFailed(format!("start cleanup helper: {error}"))
        })
}

fn scrubbed_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let environment = std::env::vars_os().filter(|(name, _)| {
        let name = name.to_string_lossy().to_ascii_uppercase();
        !["KEY", "SECRET", "TOKEN", "PASSWORD"]
            .iter()
            .any(|sensitive| name.contains(sensitive))
    });
    let mut command = Command::new(program);
    command.env_clear().envs(environment);
    command
}

#[allow(clippy::needless_pass_by_value)]
fn update_error(error: kb_update::UpdateError) -> KbError {
    let reason = error.to_string();
    let (code, retryable, action) = match &error {
        kb_update::UpdateError::VerificationFailed(_) => (
            ErrorCode::UpdateVerificationFailed,
            false,
            "Do not install this release; try again later or report the failed verification."
                .to_owned(),
        ),
        kb_update::UpdateError::Transport(failure) => {
            let action = if failure.retryable() {
                format!(
                    "The release service could not be reached reliably during {} for {} after {} attempt(s); try again later.",
                    failure.stage(),
                    failure.host(),
                    failure.attempts()
                )
            } else {
                format!(
                    "The release request failed during {} for {} after {} attempt(s); review TLS or proxy settings, then retry.",
                    failure.stage(),
                    failure.host(),
                    failure.attempts()
                )
            };
            (ErrorCode::IoFailure, failure.retryable(), action)
        }
        kb_update::UpdateError::InvalidRelease(_) | kb_update::UpdateError::MissingAsset { .. } => {
            (
                ErrorCode::CapabilityUnavailable,
                false,
                "No compatible official update is available for this installation.".to_owned(),
            )
        }
        kb_update::UpdateError::ReplacementFailed(_) => (
            ErrorCode::IoFailure,
            true,
            "The previous executable was preserved when possible; check permissions and retry."
                .to_owned(),
        ),
    };
    KbError::new(code, "Cannot check for an update.", retryable, action)
        .with_details(json!({ "reason": reason }))
}

fn run_app_request(request: AppRequest, context: &AppContext) -> Result<Value, KbError> {
    if !matches!(request, AppRequest::Version) {
        return kb_app::run(request, context);
    }

    let mut response = kb_app::run(request, context)?;
    let identity = compiled_identity()?;
    let object = response
        .as_object_mut()
        .ok_or_else(|| KbError::invalid_config("version response", "expected an object"))?;
    object.insert(
        "distribution".to_owned(),
        json!({
            "official_release": identity.can_update(),
            "target": identity.target().map(kb_update::ReleaseTarget::triple),
        }),
    );
    Ok(response)
}

fn compiled_identity() -> Result<kb_update::BuildIdentity, KbError> {
    let identity = match (
        option_env!("KB_RELEASE_TARGET"),
        option_env!("KB_RELEASE_PUBLIC_KEY"),
    ) {
        (None, None) => kb_update::BuildIdentity::development(env!("CARGO_PKG_VERSION")),
        (Some(target), Some(public_key)) => {
            kb_update::BuildIdentity::official(env!("CARGO_PKG_VERSION"), target, public_key)
        }
        _ => {
            return Err(KbError::new(
                ErrorCode::InvalidConfig,
                "The executable has incomplete official release metadata.",
                false,
                "Reinstall an official release binary.",
            ));
        }
    };
    identity.map_err(|error| {
        KbError::new(
            ErrorCode::InvalidConfig,
            "The executable has invalid official release metadata.",
            false,
            "Reinstall an official release binary.",
        )
        .with_details(json!({ "reason": error.to_string() }))
    })
}

async fn run_mcp(command: args::McpCommand, context: AppContext) -> Result<(), KbError> {
    if command.transport == args::McpTransport::Stdio
        && (command.token_file.is_some() || !command.allow_origins.is_empty())
    {
        return Err(KbError::invalid_config(
            "MCP transport options",
            "--token-file and --allow-origin require --transport streamable-http",
        ));
    }
    let selected = kb_app::run(
        kb_app::AppRequest::Paths {
            vault: command.vault,
        },
        &context,
    )?;
    let fixed_vault = selected["root"]
        .as_str()
        .ok_or_else(|| KbError::invalid_config("selected Vault", "missing root"))?
        .to_owned();
    let server = kb_mcp::McpServer::new(context, fixed_vault, command.allow_write);
    if command.transport == args::McpTransport::Stdio {
        let mut server = server;
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        return kb_mcp::serve_frames(stdin.lock(), stdout.lock(), |request| {
            server.handle(&request)
        })
        .map_err(|error| {
            KbError::io_failure("serve MCP stdio", "stdin/stdout", error.to_string())
        });
    }

    let token = command
        .token_file
        .as_deref()
        .map(kb_server::read_token_file)
        .transpose()?;
    let _ = kb_server::ServerPolicy::new(command.bind, token.clone(), command.allow_write)?;
    let listener = tokio::net::TcpListener::bind(command.bind)
        .await
        .map_err(|error| {
            KbError::io_failure(
                "bind MCP HTTP listener",
                command.bind.to_string(),
                error.to_string(),
            )
        })?;
    let address = listener.local_addr().map_err(|error| {
        KbError::io_failure(
            "read MCP HTTP listener address",
            command.bind.to_string(),
            error.to_string(),
        )
    })?;
    let policy = kb_server::ServerPolicy::new(address, token, command.allow_write)?;
    let state = kb_server::McpHttpState::new(server, policy.clone(), command.allow_origins);
    render::startup(&json!({
        "transport": "streamable-http",
        "protocol_version": kb_mcp::MODERN_PROTOCOL_VERSION,
        "endpoint": format!("http://{address}/mcp"),
        "root": selected["root"],
        "allow_write": policy.allow_write(),
        "authentication_required": policy.authentication_required(),
    }))?;
    kb_server::serve_mcp(listener, state, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

async fn run_server(command: args::ServeCommand, context: AppContext) -> Result<(), KbError> {
    let token = command
        .token_file
        .as_deref()
        .map(kb_server::read_token_file)
        .transpose()?;
    let _ = kb_server::ServerPolicy::new(command.bind, token.clone(), command.allow_write)?;
    let selected = kb_app::run(
        kb_app::AppRequest::Paths {
            vault: command.vault,
        },
        &context,
    )?;
    let vault_id = selected["vault_id"]
        .as_str()
        .ok_or_else(|| KbError::invalid_config("selected Vault", "missing vault_id"))?
        .to_owned();
    let listener = tokio::net::TcpListener::bind(command.bind)
        .await
        .map_err(|error| {
            KbError::io_failure(
                "bind HTTP listener",
                command.bind.to_string(),
                error.to_string(),
            )
        })?;
    let address = listener.local_addr().map_err(|error| {
        KbError::io_failure(
            "read HTTP listener address",
            command.bind.to_string(),
            error.to_string(),
        )
    })?;
    let policy = kb_server::ServerPolicy::new(address, token, command.allow_write)?;
    let state = kb_server::ServerState::new(context, vault_id.clone(), policy.clone());
    render::startup(&serde_json::json!({
        "bind": address,
        "vault_id": vault_id,
        "root": selected["root"],
        "allow_write": policy.allow_write(),
        "authentication_required": policy.authentication_required(),
    }))?;

    let signal_error = Arc::new(Mutex::new(None));
    let shutdown_error = Arc::clone(&signal_error);
    kb_server::serve(listener, state, async move {
        if let Err(error) = tokio::signal::ctrl_c().await {
            *shutdown_error.lock().expect("signal error lock poisoned") = Some(error.to_string());
        }
    })
    .await?;
    if let Some(error) = signal_error
        .lock()
        .expect("signal error lock poisoned")
        .take()
    {
        return Err(KbError::io_failure(
            "listen for Ctrl-C",
            "process signal",
            error,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::update_error;

    #[test]
    fn update_transport_errors_are_retryable() {
        let failure = kb_update::TransportFailure::for_url(
            kb_update::TransportStage::Resolve,
            "https://github.com/releases/latest?token=secret",
            3,
            true,
            "temporary failure",
        );
        let error = update_error(kb_update::UpdateError::Transport(failure));

        assert!(error.retryable);
        assert!(error.next_action.contains("resolve"));
        assert!(error.next_action.contains("3 attempt(s)"));
        assert!(!error.next_action.contains("network connection"));
        assert!(!error.next_action.contains("secret"));
    }
}
