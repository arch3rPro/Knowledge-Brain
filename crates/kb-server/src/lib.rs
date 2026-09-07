//! Optional HTTP transport over the shared Knowledge-Brain application layer.

use std::{fs, future::Future, net::SocketAddr, path::Path, str::FromStr, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as RoutePath, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use kb_app::{AppContext, AppRequest, OperationRequest};
use kb_core::{ErrorCode, KbError, KnowledgePlanRequest, OperationId, SearchRequest};
use kb_protocol::{Envelope, ErrorEnvelope};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const MAX_JSON_BODY_BYTES: usize = 1024 * 1024;
const MAX_TOKEN_FILE_BYTES: u64 = 4096;

/// Read a bounded token from a regular, non-link file.
///
/// # Errors
///
/// Returns an IO, unsafe-path or authentication error for an unreadable,
/// link-shaped, oversized or invalid token file.
pub fn read_token_file(path: &Path) -> Result<String, KbError> {
    kb_core::ensure_not_link_or_reparse_point(path)?;
    let metadata = fs::metadata(path).map_err(|error| {
        KbError::io_failure(
            "inspect HTTP token file",
            path.display().to_string(),
            error.to_string(),
        )
    })?;
    if !metadata.is_file() || metadata.len() > MAX_TOKEN_FILE_BYTES {
        return Err(auth_error(
            "HTTP token file must be a regular file no larger than 4096 bytes.",
        ));
    }
    fs::read_to_string(path).map_err(|error| {
        KbError::io_failure(
            "read HTTP token file",
            path.display().to_string(),
            error.to_string(),
        )
    })
}

#[derive(Debug, Clone)]
pub struct ServerPolicy {
    bind: SocketAddr,
    token: Option<Arc<str>>,
    allow_write: bool,
}

impl ServerPolicy {
    /// Validate HTTP bind, authentication and write authority together.
    ///
    /// # Errors
    ///
    /// Rejects empty/header-unsafe tokens, unauthenticated non-loopback binds,
    /// and unauthenticated write-enabled servers.
    pub fn new(
        bind: SocketAddr,
        token: Option<String>,
        allow_write: bool,
    ) -> Result<Self, KbError> {
        let token = match token {
            Some(value) => {
                let value = value.trim().to_owned();
                if value.is_empty()
                    || value.bytes().any(|byte| byte.is_ascii_whitespace())
                    || value.starts_with("Bearer")
                {
                    return Err(auth_error(
                        "HTTP token must be one nonempty value without whitespace or a Bearer prefix.",
                    ));
                }
                Some(value)
            }
            None => None,
        };
        if (!bind.ip().is_loopback() || allow_write) && token.is_none() {
            return Err(auth_error(
                "Non-loopback or write-enabled HTTP serving requires a token file.",
            ));
        }
        Ok(Self {
            bind,
            token: token.map(Arc::from),
            allow_write,
        })
    }

    #[must_use]
    pub const fn bind(&self) -> SocketAddr {
        self.bind
    }

    #[must_use]
    pub const fn allow_write(&self) -> bool {
        self.allow_write
    }

    #[must_use]
    pub fn authentication_required(&self) -> bool {
        self.token.is_some()
    }

    #[must_use]
    pub fn authorize(&self, authorization: Option<&str>) -> bool {
        let Some(expected) = self.token.as_deref() else {
            return true;
        };
        let Some(provided) = authorization.and_then(|value| value.strip_prefix("Bearer ")) else {
            return false;
        };
        constant_time_equal(expected.as_bytes(), provided.as_bytes())
    }
}

fn constant_time_equal(expected: &[u8], provided: &[u8]) -> bool {
    let mut difference = expected.len() ^ provided.len();
    for index in 0..expected.len().max(provided.len()) {
        difference |= usize::from(
            expected.get(index).copied().unwrap_or_default()
                ^ provided.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn auth_error(message: &str) -> KbError {
    KbError::new(
        ErrorCode::AuthDenied,
        message,
        false,
        "Use loopback read-only mode or provide a protected token file.",
    )
}

#[derive(Debug, Clone)]
pub struct ServerState {
    context: AppContext,
    vault: String,
    policy: ServerPolicy,
}

impl ServerState {
    #[must_use]
    pub const fn new(context: AppContext, vault: String, policy: ServerPolicy) -> Self {
        Self {
            context,
            vault,
            policy,
        }
    }

    #[must_use]
    pub const fn policy(&self) -> &ServerPolicy {
        &self.policy
    }
}

/// Serve the fixed-Vault API until shutdown resolves.
///
/// # Errors
///
/// Returns an IO failure if the listener cannot accept or serve connections.
pub async fn serve<F>(listener: TcpListener, state: ServerState, shutdown: F) -> Result<(), KbError>
where
    F: Future<Output = ()> + Send + 'static,
{
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|error| KbError::io_failure("serve HTTP", "listener", error.to_string()))
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/capabilities", get(capabilities))
        .route("/status", get(status))
        .route("/doctor", get(doctor))
        .route("/query", post(query))
        .route("/lint", post(lint))
        .route("/review", post(review))
        .route("/plans", post(create_plan))
        .route("/operations/{operation_id}", get(operation))
        .route("/operations/{operation_id}/apply", post(apply))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES))
        .with_state(state)
}

async fn capabilities(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    if let Err(response) = authenticate(&state, &headers) {
        return response;
    }
    match call(&state, AppRequest::Capabilities).await {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("http".into(), Value::Bool(true));
                object.insert(
                    "http_policy".into(),
                    json!({
                        "allow_write": state.policy.allow_write(),
                        "authentication_required": state.policy.authentication_required(),
                    }),
                );
            }
            success(value)
        }
        Err(error) => failure(error),
    }
}

async fn status(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    run_authenticated(
        &state,
        &headers,
        AppRequest::Status {
            vault: selected(&state),
        },
    )
    .await
}

async fn doctor(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    run_authenticated(
        &state,
        &headers,
        AppRequest::Doctor {
            vault: selected(&state),
        },
    )
    .await
}

async fn query(
    State(state): State<ServerState>,
    headers: HeaderMap,
    request: Result<Json<SearchRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = authenticate(&state, &headers) {
        return response;
    }
    let request = match json_request(request) {
        Ok(request) => request,
        Err(response) => return response,
    };
    match call(
        &state,
        AppRequest::Query {
            vault: selected(&state),
            request,
        },
    )
    .await
    {
        Ok(value) => success(value),
        Err(error) => failure(error),
    }
}

async fn lint(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    run_authenticated(
        &state,
        &headers,
        AppRequest::Lint {
            vault: selected(&state),
        },
    )
    .await
}

async fn review(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    run_authenticated(
        &state,
        &headers,
        AppRequest::Review {
            vault: selected(&state),
        },
    )
    .await
}

async fn create_plan(
    State(state): State<ServerState>,
    headers: HeaderMap,
    request: Result<Json<KnowledgePlanRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = authenticate(&state, &headers) {
        return response;
    }
    let request = match json_request(request) {
        Ok(request) => request,
        Err(response) => return response,
    };
    match call(
        &state,
        AppRequest::PlanCreate {
            vault: selected(&state),
            request,
        },
    )
    .await
    {
        Ok(value) => success(value),
        Err(error) => failure(error),
    }
}

async fn operation(
    State(state): State<ServerState>,
    headers: HeaderMap,
    RoutePath(operation_id): RoutePath<String>,
) -> Response {
    let operation_id = match parse_operation_id(&operation_id) {
        Ok(operation_id) => operation_id,
        Err(response) => return response,
    };
    run_authenticated(
        &state,
        &headers,
        AppRequest::Operation(OperationRequest::ShowForVault {
            vault: state.vault.clone(),
            operation_id,
        }),
    )
    .await
}

async fn apply(
    State(state): State<ServerState>,
    headers: HeaderMap,
    RoutePath(operation_id): RoutePath<String>,
) -> Response {
    if let Err(response) = authenticate(&state, &headers) {
        return response;
    }
    if !state.policy.allow_write() {
        return failure_with_status(
            StatusCode::FORBIDDEN,
            auth_error("HTTP apply is disabled; restart with --allow-write and a token file."),
        );
    }
    let operation_id = match parse_operation_id(&operation_id) {
        Ok(operation_id) => operation_id,
        Err(response) => return response,
    };
    match call(
        &state,
        AppRequest::ApplyForVault {
            vault: state.vault.clone(),
            operation_id,
        },
    )
    .await
    {
        Ok(value) => success(value),
        Err(error) => failure(error),
    }
}

async fn run_authenticated(
    state: &ServerState,
    headers: &HeaderMap,
    request: AppRequest,
) -> Response {
    if let Err(response) = authenticate(state, headers) {
        return response;
    }
    match call(state, request).await {
        Ok(value) => success(value),
        Err(error) => failure(error),
    }
}

async fn call(state: &ServerState, request: AppRequest) -> Result<Value, KbError> {
    let context = state.context.clone();
    tokio::task::spawn_blocking(move || kb_app::run(request, &context))
        .await
        .map_err(|error| {
            KbError::io_failure("join HTTP application task", "runtime", error.to_string())
        })?
}

fn authenticate(state: &ServerState, headers: &HeaderMap) -> Result<(), Response> {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if state.policy.authorize(authorization) {
        Ok(())
    } else {
        Err(failure_with_status(
            StatusCode::UNAUTHORIZED,
            auth_error("A valid Bearer token is required."),
        ))
    }
}

fn json_request<T>(request: Result<Json<T>, JsonRejection>) -> Result<T, Response> {
    request.map(|Json(value)| value).map_err(|error| {
        failure_with_status(
            StatusCode::BAD_REQUEST,
            KbError::invalid_config("HTTP JSON body", error.body_text()),
        )
    })
}

fn parse_operation_id(value: &str) -> Result<OperationId, Response> {
    OperationId::from_str(value).map_err(|error| {
        failure_with_status(
            StatusCode::BAD_REQUEST,
            KbError::invalid_config("operation_id", error.to_string()),
        )
    })
}

fn selected(state: &ServerState) -> Option<String> {
    Some(state.vault.clone())
}

fn success(value: Value) -> Response {
    (StatusCode::OK, Json(Envelope::new(value))).into_response()
}

fn failure(error: KbError) -> Response {
    let status = match error.code {
        ErrorCode::AuthDenied => StatusCode::FORBIDDEN,
        ErrorCode::VaultNotFound | ErrorCode::OperationNotFound => StatusCode::NOT_FOUND,
        ErrorCode::WriteBusy
        | ErrorCode::VaultNeedsRecovery
        | ErrorCode::PlanStale
        | ErrorCode::TargetNotEmpty => StatusCode::CONFLICT,
        ErrorCode::IoFailure => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    failure_with_status(status, error)
}

fn failure_with_status(status: StatusCode, error: KbError) -> Response {
    (status, Json(ErrorEnvelope::from(error))).into_response()
}

async fn not_found() -> Response {
    failure_with_status(
        StatusCode::NOT_FOUND,
        KbError::new(
            ErrorCode::CapabilityUnavailable,
            "HTTP route does not exist.",
            false,
            "Use GET /capabilities to discover available routes.",
        ),
    )
}

async fn method_not_allowed() -> Response {
    failure_with_status(
        StatusCode::METHOD_NOT_ALLOWED,
        KbError::new(
            ErrorCode::CapabilityUnavailable,
            "HTTP method is not available for this route.",
            false,
            "Use GET /capabilities and the documented route method.",
        ),
    )
}
