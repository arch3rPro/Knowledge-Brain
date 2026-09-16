use crate::{ErrorCode, KbError, PortableRelativePath, SourceId, SourceVersion};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

pub const DEFAULT_RESOURCE_PAGE_CHARS: usize = 16_000;
pub const MAX_RESOURCE_PAGE_CHARS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeResourceUri {
    Vault {
        vault_id: Uuid,
        path: PortableRelativePath,
    },
    Source(SourceVersion),
}

impl KnowledgeResourceUri {
    #[must_use]
    pub const fn vault_id(&self) -> Option<Uuid> {
        match self {
            Self::Vault { vault_id, .. } => Some(*vault_id),
            Self::Source(_) => None,
        }
    }

    #[must_use]
    pub const fn vault_path(&self) -> Option<&PortableRelativePath> {
        match self {
            Self::Vault { path, .. } => Some(path),
            Self::Source(_) => None,
        }
    }
}

impl FromStr for KnowledgeResourceUri {
    type Err = KbError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = value.strip_prefix("kb-vault://") {
            return parse_vault(rest);
        }
        if let Some(rest) = value.strip_prefix("kb-source://") {
            return parse_source(rest);
        }
        Err(invalid_uri(value, "unsupported resource URI scheme"))
    }
}

impl fmt::Display for KnowledgeResourceUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vault { vault_id, path } => {
                write!(formatter, "kb-vault://{vault_id}/{}", path.as_str())
            }
            Self::Source(version) => formatter.write_str(&version.exact_uri()),
        }
    }
}

fn parse_vault(value: &str) -> Result<KnowledgeResourceUri, KbError> {
    let (identity, path) = value
        .split_once('/')
        .ok_or_else(|| invalid_uri(value, "Vault resource path is required"))?;
    let vault_id = Uuid::parse_str(identity)
        .map_err(|_| invalid_uri(value, "Vault identity must be a UUID"))?;
    let path = PortableRelativePath::parse(path)
        .map_err(|_| invalid_uri(value, "Vault resource path is invalid"))?;
    let readable_wiki = path.as_str() == "Wiki/index.md"
        || path.as_str().starts_with("Wiki/research/")
        || path.as_str().starts_with("Wiki/articles/");
    let readable = path.as_str() == "KB.md"
        || (readable_wiki && path.as_str().to_ascii_lowercase().ends_with(".md"));
    if !readable {
        return Err(invalid_uri(
            value,
            "Vault resource is outside the readable scope",
        ));
    }
    Ok(KnowledgeResourceUri::Vault { vault_id, path })
}

fn parse_source(value: &str) -> Result<KnowledgeResourceUri, KbError> {
    let (address, query) = value
        .split_once('?')
        .ok_or_else(|| invalid_uri(value, "exact source sha256 is required"))?;
    let sha = query
        .strip_prefix("sha256=")
        .filter(|_| !query.contains('&'))
        .ok_or_else(|| invalid_uri(value, "exact source sha256 is required"))?;
    let (admission, relative) = address
        .split_once('/')
        .ok_or_else(|| invalid_uri(value, "source path is required"))?;
    let admission = decode_component(admission, value)?;
    let relative = relative
        .split('/')
        .map(|segment| decode_component(segment, value))
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    let source = SourceId::new(
        admission,
        PortableRelativePath::parse(&relative)
            .map_err(|_| invalid_uri(value, "source path is invalid"))?,
    )
    .map_err(|_| invalid_uri(value, "source identity is invalid"))?;
    let version = SourceVersion::new(source, sha)
        .map_err(|_| invalid_uri(value, "source sha256 is invalid"))?;
    Ok(KnowledgeResourceUri::Source(version))
}

fn decode_component(value: &str, full_uri: &str) -> Result<String, KbError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(invalid_uri(full_uri, "invalid percent encoding"));
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| invalid_uri(full_uri, "invalid percent encoding"))?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| invalid_uri(full_uri, "invalid percent encoding"))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded)
        .map_err(|_| invalid_uri(full_uri, "resource URI is not valid UTF-8"))?;
    if decoded.is_empty() || decoded.contains(['/', '\0', '\\']) {
        return Err(invalid_uri(full_uri, "resource URI component is invalid"));
    }
    Ok(decoded)
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid_uri(uri: &str, reason: &str) -> KbError {
    KbError::new(
        ErrorCode::InvalidResourceUri,
        format!("Invalid knowledge resource URI: {reason}."),
        false,
        "Use a resource_uri returned by Knowledge-Brain.",
    )
    .with_details(serde_json::json!({ "resource_uri": uri, "reason": reason }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReadRequest {
    pub resource_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default = "default_page_chars")]
    pub max_chars: usize,
}

const fn default_page_chars() -> usize {
    DEFAULT_RESOURCE_PAGE_CHARS
}

impl ResourceReadRequest {
    /// Validate the resource URI and requested character page size.
    ///
    /// # Errors
    /// Returns a stable caller-facing error for an invalid URI or page size.
    pub fn validate(&self) -> Result<KnowledgeResourceUri, KbError> {
        if !(1..=MAX_RESOURCE_PAGE_CHARS).contains(&self.max_chars) {
            return Err(KbError::new(
                ErrorCode::LimitExceeded,
                format!("Resource max_chars must be between 1 and {MAX_RESOURCE_PAGE_CHARS}."),
                false,
                "Choose a bounded page size and retry the read.",
            ));
        }
        self.resource_uri.parse()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Rules,
    Wiki,
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePathScope {
    ServerVault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLink {
    pub label: String,
    pub target_uri: String,
    pub exists: Option<bool>,
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReadResponse {
    pub resource_uri: String,
    pub kind: ResourceKind,
    pub content_type: String,
    pub content: Option<String>,
    pub sha256: String,
    pub text_available: bool,
    pub complete: bool,
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub links: Vec<ResourceLink>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
