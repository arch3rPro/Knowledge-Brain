use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::{Cursor, Write},
};

use kb_update::{
    AvailableRelease, BuildIdentity, RELEASES_LATEST_URL, ReleaseTarget, ReleaseTransport,
    UpdateError, parse_latest_release, verify_release,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const TEST_KEY: &str = "RWRzq51bKcS8oJvZ4xEm+nRvGYPdsNRD3ciFPu1YJEL8Bl/3daWaj72r";

#[test]
fn resolver_rejects_prerelease_and_missing_target_archive() {
    let identity = BuildIdentity::official("0.1.0", "x86_64-unknown-linux-gnu", TEST_KEY).unwrap();
    let prerelease = json!({
        "tag_name": "v0.2.0-rc.1",
        "prerelease": true,
        "draft": false,
        "assets": [],
    });

    assert!(matches!(
        parse_latest_release(&identity, &prerelease),
        Err(UpdateError::InvalidRelease(_))
    ));

    let missing_target = json!({
        "tag_name": "v0.2.0",
        "prerelease": false,
        "draft": false,
        "assets": [
            { "name": "knowledge-brain-v0.2.0-aarch64-apple-darwin.tar.gz", "browser_download_url": "https://example.invalid/macos" },
            { "name": "SHA256SUMS", "browser_download_url": "https://example.invalid/checksums" },
            { "name": "SHA256SUMS.minisig", "browser_download_url": "https://example.invalid/signature" }
        ],
    });

    assert!(matches!(
        parse_latest_release(&identity, &missing_target),
        Err(UpdateError::MissingAsset { target: ReleaseTarget::LinuxX64, .. })
    ));
}

#[test]
fn resolver_accepts_only_a_strictly_newer_stable_release() {
    let identity = BuildIdentity::official("0.2.0", "x86_64-unknown-linux-gnu", TEST_KEY).unwrap();
    let release = json!({
        "tag_name": "v0.2.0",
        "prerelease": false,
        "draft": false,
        "assets": [
            { "name": "knowledge-brain-v0.2.0-x86_64-unknown-linux-gnu.tar.gz", "browser_download_url": "https://example.invalid/archive" },
            { "name": "SHA256SUMS", "browser_download_url": "https://example.invalid/checksums" },
            { "name": "SHA256SUMS.minisig", "browser_download_url": "https://example.invalid/signature" }
        ],
    });

    let check = parse_latest_release(&identity, &release).unwrap();
    assert_eq!(check.current.to_string(), "0.2.0");
    assert!(check.latest.is_none());
    assert!(!check.update_available);
    assert_eq!(RELEASES_LATEST_URL, "https://api.github.com/repos/arch3rPro/Knowledge-Brain/releases/latest");
}

#[test]
fn verifier_rejects_a_bad_signature_before_downloading_the_archive() {
    let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let identity = BuildIdentity::official(
        "0.1.0",
        "x86_64-pc-windows-msvc",
        &keypair.pk.to_base64(),
    )
    .unwrap();
    let release = release();
    let transport = MemoryTransport::new([
        (release.checksums_url.clone(), b"00 archive.zip\n".to_vec()),
        (release.signature_url.clone(), b"not a signature".to_vec()),
        (release.archive_url.clone(), b"must not be downloaded".to_vec()),
    ]);

    let error = verify_release(&identity, release.clone(), &transport, tempfile::tempdir().unwrap().path())
        .unwrap_err();

    assert!(matches!(error, UpdateError::VerificationFailed(_)));
    assert_eq!(transport.requests(), [release.checksums_url, release.signature_url]);
}

#[test]
fn verifier_rejects_a_digest_mismatch_and_an_archive_path_escape() {
    let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let identity = BuildIdentity::official(
        "0.1.0",
        "x86_64-pc-windows-msvc",
        &keypair.pk.to_base64(),
    )
    .unwrap();
    let release = release();
    let valid_archive = archive(&["kb.exe", "LICENSE", "INSTALL.md"]);
    let wrong_digest_value = "0".repeat(64);
    let wrong_digest = signed_checksums(&keypair, &release, &wrong_digest_value);
    let mismatch = MemoryTransport::new([
        (release.checksums_url.clone(), wrong_digest.0),
        (release.signature_url.clone(), wrong_digest.1),
        (release.archive_url.clone(), valid_archive),
    ]);
    let mismatch_error = verify_release(
        &identity,
        release.clone(),
        &mismatch,
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap_err();
    assert!(matches!(mismatch_error, UpdateError::VerificationFailed(_)));

    let escaped_archive = archive(&["../kb.exe", "LICENSE", "INSTALL.md"]);
    let escaped_digest = sha256(&escaped_archive);
    let escaped = signed_checksums(&keypair, &release, &escaped_digest);
    let escaped_transport = MemoryTransport::new([
        (release.checksums_url.clone(), escaped.0),
        (release.signature_url.clone(), escaped.1),
        (release.archive_url.clone(), escaped_archive),
    ]);
    let escaped_error = verify_release(
        &identity,
        release,
        &escaped_transport,
        tempfile::tempdir().unwrap().path(),
    )
    .unwrap_err();
    assert!(matches!(escaped_error, UpdateError::VerificationFailed(_)));
}

fn release() -> AvailableRelease {
    AvailableRelease {
        version: "0.2.0".parse().unwrap(),
        target: ReleaseTarget::WindowsX64,
        archive_url: "https://example.invalid/archive".into(),
        checksums_url: "https://example.invalid/checksums".into(),
        signature_url: "https://example.invalid/signature".into(),
    }
}

fn signed_checksums(
    keypair: &minisign::KeyPair,
    release: &AvailableRelease,
    digest: &str,
) -> (Vec<u8>, Vec<u8>) {
    let checksums = format!("{digest}  {}\n", release.target.asset_name(&release.version));
    let signature = minisign::sign(
        Some(&keypair.pk),
        &keypair.sk,
        Cursor::new(checksums.as_bytes()),
        None,
        None,
    )
    .unwrap()
    .into_string()
    .into_bytes();
    (checksums.into_bytes(), signature)
}

fn archive(entries: &[&str]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        writer
            .start_file(*entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"fixture binary").unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

struct MemoryTransport {
    values: BTreeMap<String, Vec<u8>>,
    requests: RefCell<Vec<String>>,
}

impl MemoryTransport {
    fn new<const N: usize>(entries: [(String, Vec<u8>); N]) -> Self {
        Self {
            values: BTreeMap::from(entries),
            requests: RefCell::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.borrow().clone()
    }
}

impl ReleaseTransport for MemoryTransport {
    fn get_json(&self, _url: &str) -> Result<serde_json::Value, UpdateError> {
        Err(UpdateError::Transport("JSON is not used by this test".into()))
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, UpdateError> {
        self.requests.borrow_mut().push(url.to_owned());
        self.values
            .get(url)
            .cloned()
            .ok_or_else(|| UpdateError::Transport(format!("missing fixture: {url}")))
    }
}
