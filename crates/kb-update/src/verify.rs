use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use minisign::SignatureBox;
use sha2::{Digest, Sha256};

use crate::{AvailableRelease, BuildIdentity, ReleaseTarget, ReleaseTransport, UpdateError};

/// A verified archive staged for executable identity validation and replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArchive {
    pub version: semver::Version,
    pub target: ReleaseTarget,
    pub archive: PathBuf,
    pub executable: PathBuf,
    pub sha256: String,
    pub executable_sha256: String,
}

/// Verifies, downloads, and safely extracts one official target archive.
///
/// # Errors
///
/// Returns [`UpdateError::VerificationFailed`] if the Minisign signature,
/// checksum, archive structure, or extracted content is invalid. Returns
/// [`UpdateError::Transport`] for a download failure.
pub fn verify_release(
    identity: &BuildIdentity,
    release: AvailableRelease,
    transport: &dyn ReleaseTransport,
    stage: &Path,
) -> Result<VerifiedArchive, UpdateError> {
    if identity.target() != Some(release.target) || !identity.can_update() {
        return Err(UpdateError::VerificationFailed(
            "release target does not match this executable".into(),
        ));
    }
    let checksums = transport.get_bytes(&release.checksums_url)?;
    let signature = transport.get_bytes(&release.signature_url)?;
    verify_checksums_signature(identity, &checksums, &signature)?;
    let expected_name = release.target.asset_name(&release.version);
    let expected_digest = checksum_for(&checksums, &expected_name)?;
    let archive = transport.get_bytes(&release.archive_url)?;
    let actual_digest = hex::encode(Sha256::digest(&archive));
    if actual_digest != expected_digest {
        return Err(UpdateError::VerificationFailed(
            "archive checksum does not match SHA256SUMS".into(),
        ));
    }

    let archive_path = prepare_stage(stage, &expected_name, &archive)?;
    let executable = stage
        .join("verified")
        .join(release.target.executable_name());
    match release.target {
        ReleaseTarget::WindowsX64 => extract_zip(&archive, release.target, stage)?,
        ReleaseTarget::LinuxX64 | ReleaseTarget::MacosArm64 => {
            extract_tar_gz(&archive, release.target, stage)?;
            make_executable(&executable)?;
        }
    }
    let executable_sha256 =
        hex::encode(Sha256::digest(fs::read(&executable).map_err(|error| {
            UpdateError::VerificationFailed(format!("read extracted executable: {error}"))
        })?));
    Ok(VerifiedArchive {
        version: release.version,
        target: release.target,
        archive: archive_path,
        executable,
        sha256: actual_digest,
        executable_sha256,
    })
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            UpdateError::VerificationFailed(format!("read executable permissions: {error}"))
        })?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| {
        UpdateError::VerificationFailed(format!("set executable permissions: {error}"))
    })
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), UpdateError> {
    Ok(())
}

fn verify_checksums_signature(
    identity: &BuildIdentity,
    checksums: &[u8],
    signature: &[u8],
) -> Result<(), UpdateError> {
    let signature = std::str::from_utf8(signature).map_err(|error| {
        UpdateError::VerificationFailed(format!("signature is not UTF-8: {error}"))
    })?;
    let signature = SignatureBox::from_string(signature)
        .map_err(|error| UpdateError::VerificationFailed(format!("parse signature: {error}")))?;
    minisign::verify(
        identity
            .public_key()
            .ok_or_else(|| UpdateError::VerificationFailed("missing embedded public key".into()))?,
        &signature,
        Cursor::new(checksums),
        true,
        false,
        false,
    )
    .map_err(|error| UpdateError::VerificationFailed(format!("verify signature: {error}")))
}

fn checksum_for(checksums: &[u8], expected_name: &str) -> Result<String, UpdateError> {
    let content = std::str::from_utf8(checksums).map_err(|error| {
        UpdateError::VerificationFailed(format!("checksums are not UTF-8: {error}"))
    })?;
    let mut matches = Vec::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 2 {
            return Err(UpdateError::VerificationFailed(
                "SHA256SUMS contains an invalid line".into(),
            ));
        }
        if fields[0].len() != 64 || !fields[0].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(UpdateError::VerificationFailed(
                "SHA256SUMS contains an invalid SHA-256 digest".into(),
            ));
        }
        if fields[1] == expected_name {
            matches.push(fields[0].to_ascii_lowercase());
        }
    }
    match matches.as_slice() {
        [digest] => Ok(digest.clone()),
        [] => Err(UpdateError::VerificationFailed(
            "SHA256SUMS does not name the target archive".into(),
        )),
        _ => Err(UpdateError::VerificationFailed(
            "SHA256SUMS names the target archive more than once".into(),
        )),
    }
}

fn prepare_stage(stage: &Path, asset_name: &str, archive: &[u8]) -> Result<PathBuf, UpdateError> {
    fs::create_dir_all(stage).map_err(|error| {
        UpdateError::VerificationFailed(format!("create update stage: {error}"))
    })?;
    let archive_path = stage.join(asset_name);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive_path)
        .map_err(|error| {
            UpdateError::VerificationFailed(format!("create staged archive: {error}"))
        })?;
    output.write_all(archive).map_err(|error| {
        UpdateError::VerificationFailed(format!("write staged archive: {error}"))
    })?;
    Ok(archive_path)
}

fn expected_entries(target: ReleaseTarget) -> BTreeSet<&'static str> {
    [target.executable_name(), "LICENSE", "INSTALL.md"]
        .into_iter()
        .collect()
}

fn extract_zip(archive: &[u8], target: ReleaseTarget, stage: &Path) -> Result<(), UpdateError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|error| UpdateError::VerificationFailed(format!("open ZIP archive: {error}")))?;
    let expected = expected_entries(target);
    let extracted = stage.join("verified");
    fs::create_dir(&extracted).map_err(|error| {
        UpdateError::VerificationFailed(format!("create extraction stage: {error}"))
    })?;
    let mut found = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| UpdateError::VerificationFailed(format!("read ZIP entry: {error}")))?;
        let name = entry.name().to_owned();
        if !expected.contains(name.as_str()) || entry.is_dir() || !found.insert(name.clone()) {
            return Err(UpdateError::VerificationFailed(
                "archive contains an unexpected or duplicate entry".into(),
            ));
        }
        copy_entry(&mut entry, &extracted.join(name))?;
    }
    require_expected_entries(&expected, &found)
}

fn extract_tar_gz(archive: &[u8], target: ReleaseTarget, stage: &Path) -> Result<(), UpdateError> {
    let expected = expected_entries(target);
    let extracted = stage.join("verified");
    fs::create_dir(&extracted).map_err(|error| {
        UpdateError::VerificationFailed(format!("create extraction stage: {error}"))
    })?;
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    let mut found = BTreeSet::new();
    let entries = archive
        .entries()
        .map_err(|error| UpdateError::VerificationFailed(format!("read tar entries: {error}")))?;
    for entry in entries {
        let mut entry = entry
            .map_err(|error| UpdateError::VerificationFailed(format!("read tar entry: {error}")))?;
        let path = entry
            .path()
            .map_err(|error| UpdateError::VerificationFailed(format!("read tar path: {error}")))?;
        let Some(name) = path.to_str() else {
            return Err(UpdateError::VerificationFailed(
                "archive entry path is not UTF-8".into(),
            ));
        };
        let name = name.to_owned();
        if !expected.contains(name.as_str())
            || !entry.header().entry_type().is_file()
            || !found.insert(name.clone())
        {
            return Err(UpdateError::VerificationFailed(
                "archive contains an unexpected or duplicate entry".into(),
            ));
        }
        copy_entry(&mut entry, &extracted.join(name))?;
    }
    require_expected_entries(&expected, &found)
}

fn copy_entry(reader: &mut impl Read, target: &Path) -> Result<(), UpdateError> {
    let mut output = File::create(target).map_err(|error| {
        UpdateError::VerificationFailed(format!("create extracted file: {error}"))
    })?;
    std::io::copy(reader, &mut output)
        .map_err(|error| UpdateError::VerificationFailed(format!("extract file: {error}")))?;
    output
        .flush()
        .map_err(|error| UpdateError::VerificationFailed(format!("flush extracted file: {error}")))
}

fn require_expected_entries(
    expected: &BTreeSet<&str>,
    found: &BTreeSet<String>,
) -> Result<(), UpdateError> {
    if expected.len() != found.len() || !expected.iter().all(|name| found.contains(*name)) {
        return Err(UpdateError::VerificationFailed(
            "archive is missing a required entry".into(),
        ));
    }
    Ok(())
}
