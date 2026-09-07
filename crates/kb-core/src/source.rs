use crate::{KbError, PortableRelativePath};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "RawSourceId")]
pub struct SourceId {
    admission_id: String,
    relative_path: PortableRelativePath,
}
#[derive(Deserialize)]
struct RawSourceId {
    admission_id: String,
    relative_path: PortableRelativePath,
}
impl TryFrom<RawSourceId> for SourceId {
    type Error = KbError;
    fn try_from(raw: RawSourceId) -> Result<Self, KbError> {
        Self::new(raw.admission_id, raw.relative_path)
    }
}
impl SourceId {
    /// Construct a logical source address.
    /// # Errors
    /// Rejects an empty admission identity.
    pub fn new(id: impl Into<String>, path: PortableRelativePath) -> Result<Self, KbError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(KbError::invalid_config(
                "source identity",
                "empty admission ID",
            ));
        }
        Ok(Self {
            admission_id: id,
            relative_path: path,
        })
    }
    #[must_use]
    pub fn admission_id(&self) -> &str {
        &self.admission_id
    }
    #[must_use]
    pub fn relative_path(&self) -> &PortableRelativePath {
        &self.relative_path
    }
    #[must_use]
    pub fn logical_uri(&self) -> String {
        format!(
            "kb-source://{}/{}",
            encode(&self.admission_id),
            self.relative_path
                .as_str()
                .split('/')
                .map(encode)
                .collect::<Vec<_>>()
                .join("/")
        )
    }
}
fn encode(value: &str) -> String {
    let mut s = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            s.push(char::from(b));
        } else {
            write!(s, "%{b:02X}").expect("writing to a String cannot fail");
        }
    }
    s
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawVersion")]
pub struct SourceVersion {
    pub source: SourceId,
    sha256: String,
}
#[derive(Deserialize)]
struct RawVersion {
    source: SourceId,
    sha256: String,
}
impl TryFrom<RawVersion> for SourceVersion {
    type Error = KbError;
    fn try_from(raw: RawVersion) -> Result<Self, KbError> {
        Self::new(raw.source, raw.sha256)
    }
}
impl SourceVersion {
    /// Bind an identity to exact source bytes.
    /// # Errors
    /// Rejects any digest other than 64 lowercase hexadecimal characters.
    pub fn new(source: SourceId, sha: impl Into<String>) -> Result<Self, KbError> {
        let sha = sha.into();
        if sha.len() != 64
            || !sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(KbError::invalid_config(
                "source sha256",
                "expected 64 lowercase hexadecimal characters",
            ));
        }
        Ok(Self {
            source,
            sha256: sha,
        })
    }
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    #[must_use]
    pub fn exact_uri(&self) -> String {
        format!("{}?sha256={}", self.source.logical_uri(), self.sha256)
    }
    #[must_use]
    pub fn object_path(&self) -> String {
        format!(
            "Wiki/external-sources/.objects/sha256/{}/{}",
            &self.sha256[..2],
            self.sha256
        )
    }
}
