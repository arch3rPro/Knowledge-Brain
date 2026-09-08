use kb_update::{BuildIdentity, ReleaseTarget};

const TEST_KEY: &str = "RWRzq51bKcS8oJvZ4xEm+nRvGYPdsNRD3ciFPu1YJEL8Bl/3daWaj72r";

#[test]
fn official_identity_requires_a_supported_target_and_valid_public_key() {
    let identity = BuildIdentity::official("0.1.0", "aarch64-apple-darwin", TEST_KEY).unwrap();

    assert!(identity.can_update());
    assert_eq!(identity.target(), Some(ReleaseTarget::MacosArm64));
    assert_eq!(
        identity.target().unwrap().asset_name(identity.version()),
        "knowledge-brain-v0.1.0-aarch64-apple-darwin.tar.gz"
    );
    assert!(BuildIdentity::official("0.1.0", "darwin-x64", TEST_KEY).is_err());
    assert!(BuildIdentity::official("0.1.0", "aarch64-apple-darwin", "not-a-key").is_err());
}

#[test]
fn development_identity_cannot_be_an_update_target() {
    let identity = BuildIdentity::development("0.1.0").unwrap();

    assert!(!identity.can_update());
    assert_eq!(identity.target(), None);
}

#[test]
fn targets_name_their_executable_and_release_asset() {
    let identity = BuildIdentity::development("0.1.0").unwrap();
    assert_eq!(ReleaseTarget::LinuxX64.executable_name(), "kb");
    assert_eq!(ReleaseTarget::MacosArm64.executable_name(), "kb");
    assert_eq!(ReleaseTarget::WindowsX64.executable_name(), "kb.exe");
    assert_eq!(
        ReleaseTarget::WindowsX64.asset_name(identity.version()),
        "knowledge-brain-v0.1.0-x86_64-pc-windows-msvc.zip"
    );
}
