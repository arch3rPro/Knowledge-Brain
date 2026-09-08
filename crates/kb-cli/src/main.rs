use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
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
        args::ParsedCommand::Mcp(command) => match run_mcp(command, context) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => render::error(error, false),
        },
        args::ParsedCommand::Update(command) => {
            let json = command.json;
            match run_update(command) {
                Ok(value) => render::success(&value, json, false, false),
                Err(error) => render::error(error, json),
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

fn run_update(command: args::UpdateCommand) -> Result<Value, KbError> {
    let identity = compiled_identity()?;
    if !identity.can_update() {
        return Err(KbError::new(
            ErrorCode::CapabilityUnavailable,
            "This installation was not installed from an official GitHub Release.",
            false,
            "Use your source or Cargo package manager to update it.",
        ));
    }
    let check =
        kb_update::check_for_update(&identity, &kb_update::UreqTransport).map_err(update_error)?;
    if command.check_only || !check.update_available {
        return Ok(json!({
            "current": check.current.to_string(),
            "latest": check.latest.as_ref().map(|release| release.version.to_string()),
            "update_available": check.update_available,
            "status": "checked",
        }));
    }

    let release = check.latest.ok_or_else(|| {
        KbError::new(
            ErrorCode::CapabilityUnavailable,
            "No newer official release is available.",
            false,
            "Continue using the installed version.",
        )
    })?;
    let version = release.version.clone();
    schedule_update(&identity, release)?;
    Ok(json!({
        "current": check.current.to_string(),
        "latest": version.to_string(),
        "update_available": true,
        "status": "scheduled",
    }))
}

fn schedule_update(
    identity: &kb_update::BuildIdentity,
    release: kb_update::AvailableRelease,
) -> Result<(), KbError> {
    let current = std::env::current_exe().map_err(|error| {
        KbError::io_failure("locate current executable", "process", error.to_string())
    })?;
    let parent = current.parent().ok_or_else(|| {
        KbError::io_failure(
            "locate executable directory",
            current.display().to_string(),
            "missing parent",
        )
    })?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let stage_path = parent.join(format!(".kb-update-{token}"));
    create_private_stage(&stage_path, &token)?;
    let mut stage = UpdateStageGuard::new(stage_path.clone(), token.clone());

    let verified =
        kb_update::verify_release(identity, release, &kb_update::UreqTransport, &stage_path)
            .map_err(update_error)?;
    let output = scrubbed_command(&verified.executable)
        .args(["version", "--json"])
        .output()
        .map_err(|error| {
            update_error(kb_update::UpdateError::VerificationFailed(format!(
                "run staged executable: {error}"
            )))
        })?;
    if !output.status.success() {
        return Err(update_error(kb_update::UpdateError::VerificationFailed(
            "staged executable could not report its identity".into(),
        )));
    }
    kb_update::validate_staged_identity(&output.stdout, &verified.version, verified.target)
        .map_err(update_error)?;

    let helper = stage_path.join(verified.target.executable_name());
    fs::copy(&current, &helper).map_err(|error| {
        KbError::io_failure(
            "copy update helper",
            helper.display().to_string(),
            error.to_string(),
        )
    })?;
    let backup = parent.join(format!(".kb-backup-{}-{token}", verified.version));
    let parent_pid = std::process::id();
    let parent_start_time =
        kb_update::parent_process_start_time(parent_pid).map_err(update_error)?;
    scrubbed_command(&helper)
        .arg("__replace")
        .arg("--parent-pid")
        .arg(parent_pid.to_string())
        .arg("--parent-start-time")
        .arg(parent_start_time.to_string())
        .arg("--from")
        .arg(&verified.executable)
        .arg("--to")
        .arg(&current)
        .arg("--backup")
        .arg(&backup)
        .arg("--expected-sha256")
        .arg(&verified.executable_sha256)
        .arg("--cleanup-dir")
        .arg(&stage_path)
        .arg("--cleanup-token")
        .arg(&token)
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            KbError::io_failure(
                "start update helper",
                helper.display().to_string(),
                error.to_string(),
            )
        })?;
    stage.disarm();
    Ok(())
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

fn create_private_stage(path: &Path, token: &str) -> Result<(), KbError> {
    fs::create_dir(path).map_err(|error| {
        KbError::io_failure(
            "create update stage",
            path.display().to_string(),
            error.to_string(),
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            KbError::io_failure(
                "secure update stage",
                path.display().to_string(),
                error.to_string(),
            )
        })?;
    }
    let marker = path.join(".cleanup-token");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|error| {
            KbError::io_failure(
                "create update marker",
                marker.display().to_string(),
                error.to_string(),
            )
        })?;
    output.write_all(token.as_bytes()).map_err(|error| {
        KbError::io_failure(
            "write update marker",
            marker.display().to_string(),
            error.to_string(),
        )
    })
}

struct UpdateStageGuard {
    path: PathBuf,
    token: String,
    armed: bool,
}

impl UpdateStageGuard {
    fn new(path: PathBuf, token: String) -> Self {
        Self {
            path,
            token,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for UpdateStageGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = kb_update::remove_update_stage(&self.path, &self.token);
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn update_error(error: kb_update::UpdateError) -> KbError {
    let (code, retryable, action) = match error {
        kb_update::UpdateError::VerificationFailed(_) => (
            ErrorCode::UpdateVerificationFailed,
            false,
            "Do not install this release; try again later or report the failed verification.",
        ),
        kb_update::UpdateError::Transport(_) => (
            ErrorCode::IoFailure,
            true,
            "Check your network connection and run the command again.",
        ),
        kb_update::UpdateError::InvalidRelease(_) | kb_update::UpdateError::MissingAsset { .. } => {
            (
                ErrorCode::CapabilityUnavailable,
                false,
                "No compatible official update is available for this installation.",
            )
        }
        kb_update::UpdateError::ReplacementFailed(_) => (
            ErrorCode::IoFailure,
            true,
            "The previous executable was preserved when possible; check permissions and retry.",
        ),
    };
    KbError::new(code, "Cannot check for an update.", retryable, action)
        .with_details(json!({ "reason": error.to_string() }))
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

fn run_mcp(command: args::McpCommand, context: AppContext) -> Result<(), KbError> {
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
    let mut server = kb_mcp::McpServer::new(context, fixed_vault, command.allow_write);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    kb_mcp::serve_frames(stdin.lock(), stdout.lock(), |request| {
        server.handle(&request)
    })
    .map_err(|error| KbError::io_failure("serve MCP stdio", "stdin/stdout", error.to_string()))
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
        let error = update_error(kb_update::UpdateError::Transport(
            "temporary failure".into(),
        ));

        assert!(error.retryable);
    }
}
