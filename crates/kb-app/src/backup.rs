use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use kb_core::{
    AdmissionDocument, BackupCreateReport, BackupFile, BackupManifest, BackupRestoreReport,
    BackupVerifyReport, CURRENT_SCHEMA_VERSION, ErrorCode, KbError, PortableRelativePath,
    ensure_not_link_or_reparse_point, portability_key, validate_admission_directory,
};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{source_io::io, vault::read_vault_identity};

const MANIFEST_NAME: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BackupCreateRequest {
    pub vault: PathBuf,
    pub output: PathBuf,
    pub include_source_objects: bool,
    pub created_at: OffsetDateTime,
}

/// Create a complete or compact verified ZIP for one selected Vault.
///
/// # Errors
///
/// Returns an error for an unsafe Vault tree, an existing/inside-Vault output,
/// an active incomplete save, changed input bytes, or failed archive IO.
pub fn create_backup(request: &BackupCreateRequest) -> Result<BackupCreateReport, KbError> {
    ensure_not_link_or_reparse_point(&request.vault)?;
    if !request.vault.is_dir() {
        return Err(KbError::new(
            ErrorCode::VaultNotFound,
            format!("Vault does not exist: {}", request.vault.display()),
            false,
            "Choose an existing Vault.",
        ));
    }
    if request.output.starts_with(&request.vault) {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            "A backup archive cannot be created inside the Vault being backed up.",
            false,
            "Choose an output path outside the Vault.",
        ));
    }
    if request.output.exists() {
        return Err(target_exists(&request.output));
    }
    let output_parent = request.output.parent().ok_or_else(|| {
        KbError::new(
            ErrorCode::UnsafePath,
            "Backup output has no parent directory.",
            false,
            "Choose an output below an existing directory.",
        )
    })?;
    ensure_not_link_or_reparse_point(output_parent)?;
    if !output_parent.is_dir() {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            format!(
                "Backup output parent does not exist: {}",
                output_parent.display()
            ),
            false,
            "Create the parent directory and run backup again.",
        ));
    }
    let canonical_vault = fs::canonicalize(&request.vault)
        .map_err(|error| io("resolve Vault", &request.vault, error))?;
    let canonical_output_parent = fs::canonicalize(output_parent)
        .map_err(|error| io("resolve backup output parent", output_parent, error))?;
    if canonical_output_parent.starts_with(canonical_vault) {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            "A backup archive cannot be created inside the Vault being backed up.",
            false,
            "Choose an output path outside the Vault.",
        ));
    }

    let identity = read_vault_identity(&request.vault)?;
    let (directories, files) = collect(&request.vault, request.include_source_objects)?;
    let manifest = BackupManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        vault_schema_version: identity.schema_version,
        vault_id: identity.vault_id,
        created_at: request
            .created_at
            .format(&Rfc3339)
            .map_err(|error| KbError::invalid_config("backup time", error.to_string()))?,
        app_version: env!("CARGO_PKG_VERSION").into(),
        complete_source_evidence: request.include_source_objects,
        directories,
        files,
    };
    manifest.validate()?;
    write_archive(&request.vault, &request.output, &manifest)?;
    let total_bytes = manifest.total_bytes();
    Ok(BackupCreateReport {
        archive: request.output.clone(),
        vault_id: manifest.vault_id,
        complete_source_evidence: manifest.complete_source_evidence,
        file_count: manifest.files.len() as u64,
        total_bytes,
        warnings: Vec::new(),
    })
}

/// Verify every manifest and ZIP entry without extracting it.
///
/// # Errors
///
/// Returns an error for unreadable, malformed, unsafe, incomplete, extra, or
/// byte-inconsistent archive entries.
pub fn verify_backup(archive_path: &Path) -> Result<BackupVerifyReport, KbError> {
    let manifest = verify_manifest_and_entries(archive_path)?;
    Ok(verify_report(archive_path, &manifest))
}

/// Restore a fully verified archive into a nonexistent or empty target.
///
/// # Errors
///
/// Returns an error when verification fails, the target is unsafe/nonempty,
/// staged extraction fails, or the verified stage cannot be published.
pub fn restore_backup(archive_path: &Path, target: &Path) -> Result<BackupRestoreReport, KbError> {
    let manifest = verify_manifest_and_entries(archive_path)?;
    let existed = validate_restore_target(target)?;
    let parent = target.parent().ok_or_else(|| {
        KbError::new(
            ErrorCode::RestoreFailed,
            "Restore target has no parent directory.",
            false,
            "Choose a restore target below an existing directory.",
        )
    })?;
    let stage = tempfile::Builder::new()
        .prefix(".kb-restore-")
        .tempdir_in(parent)
        .map_err(|error| io("create restore staging directory", parent, error))?;
    extract_verified(archive_path, stage.path(), &manifest)?;

    if existed {
        fs::remove_dir(target).map_err(|error| io("remove empty restore target", target, error))?;
    }
    if let Err(error) = fs::rename(stage.path(), target) {
        let mut reason = error.to_string();
        if existed {
            if let Err(recreate) = fs::create_dir(target) {
                reason.push_str("; could not recreate the prior empty target: ");
                reason.push_str(&recreate.to_string());
            }
        }
        return Err(KbError::new(
            ErrorCode::RestoreFailed,
            format!(
                "Could not publish the verified restore at {}: {reason}",
                target.display()
            ),
            false,
            "Check target permissions and available storage, then run restore again.",
        ));
    }

    let verified = verify_report(archive_path, &manifest);
    Ok(BackupRestoreReport {
        archive: archive_path.to_path_buf(),
        target: target.to_path_buf(),
        vault_schema_version: verified.vault_schema_version,
        vault_id: verified.vault_id,
        complete_source_evidence: verified.complete_source_evidence,
        file_count: verified.file_count,
        total_bytes: verified.total_bytes,
        warnings: Vec::new(),
    })
}

fn collect(
    root: &Path,
    include_source_objects: bool,
) -> Result<(Vec<PortableRelativePath>, Vec<BackupFile>), KbError> {
    let admission_path = root.join("admission.yml");
    let admission: AdmissionDocument = serde_yaml_ng::from_slice(
        &fs::read(&admission_path).map_err(|error| io("read", &admission_path, error))?,
    )
    .map_err(|error| KbError::invalid_config("admission.yml", error.to_string()))?;

    let mut roots = vec![
        PortableRelativePath::parse("Wiki")?,
        PortableRelativePath::parse(".kb/schemas")?,
    ];
    roots.extend(admission_roots(&admission)?);
    let mut directories = vec![PortableRelativePath::parse(".kb")?];
    let mut paths = Vec::new();
    for portable in roots {
        let path = root.join(portable.to_native_path());
        ensure_not_link_or_reparse_point(&path)?;
        if !path.is_dir() {
            return Err(KbError::new(
                ErrorCode::PathNotAdmitted,
                format!("Backup directory is missing: {}", path.display()),
                false,
                "Restore the required directory or correct admission.yml.",
            ));
        }
        walk(
            root,
            &path,
            include_source_objects,
            &mut directories,
            &mut paths,
        )?;
    }
    for relative in ["admission.yml", "KB.md", ".kb/config.yml"] {
        paths.push(PortableRelativePath::parse(relative)?);
    }
    directories.sort();
    directories.dedup();
    paths.sort();
    paths.dedup();

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let native = root.join(path.to_native_path());
        let (size, sha256) = hash_file(&native)?;
        files.push(BackupFile { path, size, sha256 });
    }
    let manifest = BackupManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        vault_schema_version: CURRENT_SCHEMA_VERSION,
        vault_id: uuid::Uuid::nil(),
        created_at: "1970-01-01T00:00:00Z".into(),
        app_version: "validation".into(),
        complete_source_evidence: include_source_objects,
        directories: directories.clone(),
        files: files.clone(),
    };
    manifest.validate()?;
    Ok((directories, files))
}

fn walk(
    root: &Path,
    directory: &Path,
    include_source_objects: bool,
    directories: &mut Vec<PortableRelativePath>,
    files: &mut Vec<PortableRelativePath>,
) -> Result<(), KbError> {
    let relative = PortableRelativePath::from_path(
        directory
            .strip_prefix(root)
            .map_err(|error| io("resolve", directory, error))?,
    )?;
    if !include_source_objects
        && (relative.as_str() == "Wiki/external-sources/.objects"
            || relative
                .as_str()
                .starts_with("Wiki/external-sources/.objects/"))
    {
        return Ok(());
    }
    directories.push(relative);
    let mut entries = fs::read_dir(directory)
        .map_err(|error| io("read directory", directory, error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io("read directory entry", directory, error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        ensure_not_link_or_reparse_point(&path)?;
        let kind = entry
            .file_type()
            .map_err(|error| io("inspect", &path, error))?;
        if kind.is_dir() {
            walk(root, &path, include_source_objects, directories, files)?;
        } else if kind.is_file() {
            files.push(PortableRelativePath::from_path(
                path.strip_prefix(root)
                    .map_err(|error| io("resolve", &path, error))?,
            )?);
        } else {
            return Err(KbError::new(
                ErrorCode::UnsafePath,
                format!(
                    "Backup entry is not a regular file or directory: {}",
                    path.display()
                ),
                false,
                "Replace or remove the special filesystem entry.",
            ));
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<(u64, String), KbError> {
    ensure_not_link_or_reparse_point(path)?;
    let mut file = fs::File::open(path).map_err(|error| io("open", path, error))?;
    if !file
        .metadata()
        .map_err(|error| io("inspect", path, error))?
        .is_file()
    {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            format!("Backup entry is not a regular file: {}", path.display()),
            false,
            "Replace the entry with a regular file.",
        ));
    }
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io("read", path, error))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| invalid_archive("backup size overflow"))?;
    }
    Ok((size, hex::encode(digest.finalize())))
}

fn write_archive(root: &Path, output: &Path, manifest: &BackupManifest) -> Result<(), KbError> {
    let parent = output.parent().expect("validated output parent");
    let temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| io("create temporary backup", output, error))?;
    let writer_file = temporary
        .reopen()
        .map_err(|error| io("open temporary backup", output, error))?;
    let mut writer = ZipWriter::new(writer_file);
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let directory_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o755);

    writer
        .start_file(MANIFEST_NAME, file_options)
        .map_err(|error| archive_io("write manifest entry", output, error))?;
    writer
        .write_all(
            &serde_json::to_vec_pretty(manifest)
                .map_err(|error| KbError::invalid_config(MANIFEST_NAME, error.to_string()))?,
        )
        .map_err(|error| io("write manifest", output, error))?;
    for directory in &manifest.directories {
        writer
            .add_directory(format!("{}/", directory.as_str()), directory_options)
            .map_err(|error| archive_io("write directory entry", output, error))?;
    }
    for expected in &manifest.files {
        writer
            .start_file(expected.path.as_str(), file_options)
            .map_err(|error| archive_io("write file entry", output, error))?;
        let native = root.join(expected.path.to_native_path());
        let mut input = fs::File::open(&native).map_err(|error| io("open", &native, error))?;
        let (size, sha256) = copy_with_hash(&mut input, &mut writer, &native)?;
        if size != expected.size || sha256 != expected.sha256 {
            return Err(KbError::new(
                ErrorCode::PlanStale,
                format!(
                    "A Vault file changed while the backup was being created: {}",
                    native.display()
                ),
                true,
                "Run backup create again after active edits finish.",
            ));
        }
    }
    let finished = writer
        .finish()
        .map_err(|error| archive_io("finish backup", output, error))?;
    finished
        .sync_all()
        .map_err(|error| io("synchronize backup", output, error))?;
    temporary
        .persist_noclobber(output)
        .map_err(|error| io("publish backup without replacing", output, error.error))?;
    Ok(())
}

fn verify_manifest_and_entries(path: &Path) -> Result<BackupManifest, KbError> {
    ensure_not_link_or_reparse_point(path)?;
    let file = fs::File::open(path).map_err(|error| io("open backup", path, error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| archive_io("read backup", path, error))?;
    let mut entries = BTreeMap::new();
    let mut manifest_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| archive_io("read backup entry", path, error))?;
        let name = entry.name().to_owned();
        if entries.insert(name.clone(), index).is_some() {
            return Err(invalid_archive(&format!("duplicate ZIP entry: {name}")));
        }
        if entry.is_symlink() || (!entry.is_file() && !entry.is_dir()) {
            return Err(invalid_archive(&format!(
                "special ZIP entry is forbidden: {name}"
            )));
        }
        if name == MANIFEST_NAME {
            if !entry.is_file() || entry.size() > MAX_MANIFEST_BYTES {
                return Err(invalid_archive(
                    "manifest.json is not a bounded regular file",
                ));
            }
            manifest_index = Some(index);
        } else {
            parse_archive_name(&name, entry.is_dir())?;
        }
    }
    let manifest_index =
        manifest_index.ok_or_else(|| invalid_archive("manifest.json is missing"))?;
    let manifest: BackupManifest = {
        let entry = archive
            .by_index(manifest_index)
            .map_err(|error| archive_io("read manifest", path, error))?;
        serde_json::from_reader(entry)
            .map_err(|error| invalid_archive(&format!("invalid manifest: {error}")))?
    };
    manifest
        .validate()
        .map_err(|error| invalid_archive(&error.message))?;

    let expected_directories = manifest
        .directories
        .iter()
        .map(|directory| format!("{}/", directory.as_str()))
        .collect::<BTreeSet<_>>();
    let expected_files = manifest
        .files
        .iter()
        .map(|file| file.path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let actual = entries
        .keys()
        .filter(|name| name.as_str() != MANIFEST_NAME)
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected = expected_directories
        .iter()
        .chain(expected_files.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(invalid_archive(
            "ZIP entries do not exactly match manifest.json",
        ));
    }

    for expected in &manifest.files {
        let index = entries[expected.path.as_str()];
        let mut entry = archive
            .by_index(index)
            .map_err(|error| archive_io("read backup file", path, error))?;
        if entry.size() != expected.size {
            return Err(invalid_archive(&format!(
                "size mismatch for {}",
                expected.path.as_str()
            )));
        }
        let (size, sha256) = read_hash(&mut entry, path)?;
        if size != expected.size || sha256 != expected.sha256 {
            return Err(invalid_archive(&format!(
                "hash mismatch for {}",
                expected.path.as_str()
            )));
        }
    }
    validate_vault_contract(&mut archive, &entries, &manifest)?;
    Ok(manifest)
}

fn validate_vault_contract(
    archive: &mut ZipArchive<fs::File>,
    entries: &BTreeMap<String, usize>,
    manifest: &BackupManifest,
) -> Result<(), KbError> {
    let identity: crate::vault::VaultIdentity =
        serde_yaml_ng::from_reader(archive.by_index(entries[".kb/config.yml"]).map_err(
            |error| archive_io("read Vault identity", Path::new(".kb/config.yml"), error),
        )?)
        .map_err(|error| invalid_archive(&format!("invalid .kb/config.yml: {error}")))?;
    if identity.vault_id != manifest.vault_id
        || identity.schema_version != manifest.vault_schema_version
    {
        return Err(invalid_archive(
            "manifest Vault identity does not match .kb/config.yml",
        ));
    }
    let admission: AdmissionDocument = serde_yaml_ng::from_reader(
        archive
            .by_index(entries["admission.yml"])
            .map_err(|error| archive_io("read admission", Path::new("admission.yml"), error))?,
    )
    .map_err(|error| invalid_archive(&format!("invalid admission.yml: {error}")))?;
    let admission_roots = admission_roots(&admission)
        .map_err(|error| invalid_archive(&format!("invalid admission.yml: {}", error.message)))?;
    for root in &admission_roots {
        if !manifest.directories.contains(root) {
            return Err(invalid_archive(&format!(
                "admission directory is missing: {}",
                root.as_str()
            )));
        }
    }
    for directory in &manifest.directories {
        validate_backup_scope(
            directory,
            true,
            &admission_roots,
            manifest.complete_source_evidence,
        )?;
    }
    for file in &manifest.files {
        validate_backup_scope(
            &file.path,
            false,
            &admission_roots,
            manifest.complete_source_evidence,
        )?;
    }
    Ok(())
}

fn admission_roots(admission: &AdmissionDocument) -> Result<Vec<PortableRelativePath>, KbError> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut roots = Vec::with_capacity(admission.directories.len());
    for entry in &admission.directories {
        if entry.id.trim().is_empty() || !ids.insert(entry.id.clone()) {
            return Err(KbError::invalid_config(
                "admission.yml",
                format!("admission id is empty or duplicated: {}", entry.id),
            ));
        }
        let path = validate_admission_directory(Path::new(&entry.path))?;
        if !paths.insert(portability_key(&path)) {
            return Err(KbError::invalid_config(
                "admission.yml",
                format!(
                    "admission path is duplicated across platforms: {}",
                    entry.path
                ),
            ));
        }
        roots.push(path);
    }
    roots.sort();
    Ok(roots)
}

fn validate_backup_scope(
    path: &PortableRelativePath,
    directory: bool,
    admission_roots: &[PortableRelativePath],
    complete_source_evidence: bool,
) -> Result<(), KbError> {
    let value = path.as_str();
    if value.split('/').any(|component| component == ".git") {
        return Err(invalid_archive("Git internals are forbidden in backups"));
    }
    if !complete_source_evidence
        && (value == "Wiki/external-sources/.objects"
            || value.starts_with("Wiki/external-sources/.objects/"))
    {
        return Err(invalid_archive(
            "compact backup contains excluded source objects",
        ));
    }
    let in_tree = |root: &str| value == root || value.starts_with(&format!("{root}/"));
    let allowed = if directory {
        value == ".kb"
            || in_tree(".kb/schemas")
            || in_tree("Wiki")
            || admission_roots.iter().any(|root| in_tree(root.as_str()))
    } else {
        matches!(value, "admission.yml" | "KB.md" | ".kb/config.yml")
            || value.starts_with(".kb/schemas/")
            || value.starts_with("Wiki/")
            || admission_roots
                .iter()
                .any(|root| value.starts_with(&format!("{}/", root.as_str())))
    };
    if !allowed {
        return Err(invalid_archive(&format!(
            "entry is outside the standard backup scope: {value}"
        )));
    }
    Ok(())
}

fn extract_verified(path: &Path, stage: &Path, manifest: &BackupManifest) -> Result<(), KbError> {
    let mut archive =
        ZipArchive::new(fs::File::open(path).map_err(|error| io("open backup", path, error))?)
            .map_err(|error| archive_io("read backup", path, error))?;
    for directory in &manifest.directories {
        fs::create_dir_all(stage.join(directory.to_native_path()))
            .map_err(|error| io("create restored directory", stage, error))?;
    }
    for expected in &manifest.files {
        let mut input = archive
            .by_name(expected.path.as_str())
            .map_err(|error| archive_io("read backup file", path, error))?;
        let output_path = stage.join(expected.path.to_native_path());
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io("create restored directory", parent, error))?;
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|error| io("create restored file", &output_path, error))?;
        let (size, sha256) = copy_with_hash(&mut input, &mut output, &output_path)?;
        output
            .sync_all()
            .map_err(|error| io("synchronize restored file", &output_path, error))?;
        if size != expected.size || sha256 != expected.sha256 {
            return Err(invalid_archive(&format!(
                "restored bytes do not match manifest for {}",
                expected.path.as_str()
            )));
        }
    }
    Ok(())
}

fn parse_archive_name(name: &str, directory: bool) -> Result<PortableRelativePath, KbError> {
    let value = if directory {
        name.strip_suffix('/')
            .ok_or_else(|| invalid_archive("directory entry lacks trailing slash"))?
    } else {
        if name.ends_with('/') {
            return Err(invalid_archive("file entry has a trailing slash"));
        }
        name
    };
    PortableRelativePath::parse(value).map_err(|error| invalid_archive(&error.message))
}

fn validate_restore_target(target: &Path) -> Result<bool, KbError> {
    let parent = target.parent().ok_or_else(|| {
        KbError::new(
            ErrorCode::UnsafePath,
            "Restore target has no parent directory.",
            false,
            "Choose a target below an existing directory.",
        )
    })?;
    ensure_not_link_or_reparse_point(parent)?;
    if !parent.is_dir() {
        return Err(KbError::new(
            ErrorCode::UnsafePath,
            format!("Restore parent does not exist: {}", parent.display()),
            false,
            "Create the parent directory and run restore again.",
        ));
    }
    match fs::symlink_metadata(target) {
        Ok(_) => ensure_not_link_or_reparse_point(target)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io("inspect restore target", target, error)),
    }
    if !target.is_dir()
        || fs::read_dir(target)
            .map_err(|error| io("read restore target", target, error))?
            .next()
            .is_some()
    {
        return Err(target_exists(target));
    }
    Ok(true)
}

fn copy_with_hash(
    input: &mut impl Read,
    output: &mut impl Write,
    path: &Path,
) -> Result<(u64, String), KbError> {
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| io("read", path, error))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| io("write", path, error))?;
        digest.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| invalid_archive("file size overflow"))?;
    }
    Ok((size, hex::encode(digest.finalize())))
}

fn read_hash(input: &mut impl Read, path: &Path) -> Result<(u64, String), KbError> {
    copy_with_hash(input, &mut std::io::sink(), path)
}

fn verify_report(path: &Path, manifest: &BackupManifest) -> BackupVerifyReport {
    BackupVerifyReport {
        archive: path.to_path_buf(),
        vault_schema_version: manifest.vault_schema_version,
        vault_id: manifest.vault_id,
        complete_source_evidence: manifest.complete_source_evidence,
        file_count: manifest.files.len() as u64,
        total_bytes: manifest.total_bytes(),
    }
}

fn target_exists(path: &Path) -> KbError {
    KbError::new(
        ErrorCode::TargetNotEmpty,
        format!("Target already exists or is not empty: {}", path.display()),
        false,
        "Choose a nonexistent output or an empty restore directory.",
    )
}

fn invalid_archive(reason: &str) -> KbError {
    KbError::new(
        ErrorCode::BackupVerificationFailed,
        format!("Backup verification failed: {reason}"),
        false,
        "Use an intact Knowledge-Brain backup and run verify again.",
    )
    .with_details(serde_json::json!({
        "reason": reason,
        "legacy_code": "restore_failed",
    }))
}

fn archive_io(action: &str, path: &Path, error: impl std::fmt::Display) -> KbError {
    invalid_archive(&format!("cannot {action} {}: {error}", path.display()))
}
