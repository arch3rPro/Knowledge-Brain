use kb_update::{ReleaseTarget, UpdateError, validate_staged_identity};
use semver::Version;

#[test]
fn accepts_the_exact_official_version_and_target() {
    let output = br#"{
        "schema_version":"v1.0",
        "data":{
            "app_version":"0.2.0",
            "distribution":{
                "official_release":true,
                "target":"x86_64-unknown-linux-gnu"
            }
        }
    }"#;

    validate_staged_identity(
        output,
        &Version::parse("0.2.0").unwrap(),
        ReleaseTarget::LinuxX64,
    )
    .unwrap();
}

#[test]
fn rejects_a_development_build_or_wrong_target() {
    for output in [
        br#"{"data":{"app_version":"0.2.0","distribution":{"official_release":false,"target":"x86_64-unknown-linux-gnu"}}}"#.as_slice(),
        br#"{"data":{"app_version":"0.2.0","distribution":{"official_release":true,"target":"aarch64-apple-darwin"}}}"#.as_slice(),
    ] {
        assert!(matches!(
            validate_staged_identity(
                output,
                &Version::parse("0.2.0").unwrap(),
                ReleaseTarget::LinuxX64,
            ),
            Err(UpdateError::VerificationFailed(_))
        ));
    }
}
