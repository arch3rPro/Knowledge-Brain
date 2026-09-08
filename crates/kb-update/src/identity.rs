use minisign::PublicKey;
use semver::Version;
use thiserror::Error;

/// The release target a signed Knowledge-Brain archive can serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseTarget {
    LinuxX64,
    MacosArm64,
    WindowsX64,
}

impl ReleaseTarget {
    /// Parses one supported Rust target triple.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateIdentityError::UnsupportedTarget`] for an unsupported triple.
    pub fn from_triple(value: &str) -> Result<Self, UpdateIdentityError> {
        match value {
            "x86_64-unknown-linux-gnu" => Ok(Self::LinuxX64),
            "aarch64-apple-darwin" => Ok(Self::MacosArm64),
            "x86_64-pc-windows-msvc" => Ok(Self::WindowsX64),
            _ => Err(UpdateIdentityError::UnsupportedTarget(value.to_owned())),
        }
    }

    /// Returns the canonical Rust target triple.
    #[must_use]
    pub const fn triple(self) -> &'static str {
        match self {
            Self::LinuxX64 => "x86_64-unknown-linux-gnu",
            Self::MacosArm64 => "aarch64-apple-darwin",
            Self::WindowsX64 => "x86_64-pc-windows-msvc",
        }
    }

    /// Returns the executable name inside this target's release archive.
    #[must_use]
    pub const fn executable_name(self) -> &'static str {
        match self {
            Self::WindowsX64 => "kb.exe",
            Self::LinuxX64 | Self::MacosArm64 => "kb",
        }
    }

    /// Returns the canonical official archive name for `version`.
    #[must_use]
    pub fn asset_name(self, version: &Version) -> String {
        let extension = match self {
            Self::WindowsX64 => "zip",
            Self::LinuxX64 | Self::MacosArm64 => "tar.gz",
        };
        format!("knowledge-brain-v{version}-{}.{}", self.triple(), extension)
    }
}

/// The updater identity compiled into one Knowledge-Brain executable.
#[derive(Debug, Clone)]
pub struct BuildIdentity {
    version: Version,
    target: Option<ReleaseTarget>,
    public_key: Option<PublicKey>,
}

impl BuildIdentity {
    /// Creates the identity used by a development, source, or Cargo build.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateIdentityError::InvalidVersion`] when `version` is not `SemVer`.
    pub fn development(version: &str) -> Result<Self, UpdateIdentityError> {
        Ok(Self {
            version: parse_version(version)?,
            target: None,
            public_key: None,
        })
    }

    /// Creates the identity used by an official signed release binary.
    ///
    /// # Errors
    ///
    /// Returns an error when the version, target, or Minisign public key is invalid.
    pub fn official(
        version: &str,
        target: &str,
        public_key: &str,
    ) -> Result<Self, UpdateIdentityError> {
        let public_key = PublicKey::from_base64(public_key)
            .map_err(|error| UpdateIdentityError::InvalidPublicKey(error.to_string()))?;
        Ok(Self {
            version: parse_version(version)?,
            target: Some(ReleaseTarget::from_triple(target)?),
            public_key: Some(public_key),
        })
    }

    /// Returns whether this executable is an eligible official update target.
    #[must_use]
    pub const fn can_update(&self) -> bool {
        self.target.is_some() && self.public_key.is_some()
    }

    /// Returns the compiled application version.
    #[must_use]
    pub const fn version(&self) -> &Version {
        &self.version
    }

    /// Returns the compiled release target, if this is an official release binary.
    #[must_use]
    pub const fn target(&self) -> Option<ReleaseTarget> {
        self.target
    }

    /// Returns the embedded verification key, if this is an official release binary.
    #[must_use]
    pub const fn public_key(&self) -> Option<&PublicKey> {
        self.public_key.as_ref()
    }
}

fn parse_version(value: &str) -> Result<Version, UpdateIdentityError> {
    Version::parse(value).map_err(|error| UpdateIdentityError::InvalidVersion(error.to_string()))
}

/// An invalid value used to construct a build identity.
#[derive(Debug, Error)]
pub enum UpdateIdentityError {
    #[error("invalid release version: {0}")]
    InvalidVersion(String),
    #[error("unsupported release target: {0}")]
    UnsupportedTarget(String),
    #[error("invalid Minisign public key: {0}")]
    InvalidPublicKey(String),
}
