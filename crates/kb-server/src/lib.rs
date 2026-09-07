//! Optional HTTP transport over the shared Knowledge-Brain application layer.

use std::{net::SocketAddr, sync::Arc};

use kb_core::{ErrorCode, KbError};

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
