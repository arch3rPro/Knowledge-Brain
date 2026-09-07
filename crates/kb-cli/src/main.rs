use std::{
    collections::BTreeMap,
    process::ExitCode,
    sync::{Arc, Mutex},
};

use kb_app::AppContext;
use kb_core::KbError;

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
        } => match request.and_then(|request| kb_app::run(request, &context)) {
            Ok(value) => render::success(&value, json, fail_on_findings),
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
    }
}

fn run_mcp(command: args::McpCommand, context: AppContext) -> Result<(), KbError> {
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
    let mut server = kb_mcp::McpServer::new(context, vault_id, command.allow_write);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    kb_mcp::serve_frames(stdin.lock(), stdout.lock(), |request| {
        server.handle(request)
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
