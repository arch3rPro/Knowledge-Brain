#![cfg(unix)]

use std::{fs, path::Path, process::Command};

use zip::ZipArchive;

#[test]
fn unix_release_archive_has_the_target_specific_name_and_required_entries() {
    let temp = tempfile::tempdir().unwrap();
    let stage = temp.path().join("stage");
    fs::create_dir(&stage).unwrap();
    write_stage(&stage, "kb");

    let asset = temp
        .path()
        .join("knowledge-brain-v0.1.0-x86_64-unknown-linux-gnu.tar.gz");
    let output = package(&stage, &asset, "0.1.0", "x86_64-unknown-linux-gnu");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(asset.is_file());

    let verification = Command::new("bash")
        .arg(project_root().join("scripts/verify-release-package.sh"))
        .arg(&asset)
        .arg("x86_64-unknown-linux-gnu")
        .output()
        .unwrap();
    assert!(
        verification.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&verification.stderr)
    );

    let entries = Command::new("tar")
        .args(["-tzf", asset.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(entries.status.success());
    assert_eq!(
        String::from_utf8(entries.stdout).unwrap().lines().collect::<Vec<_>>(),
        ["INSTALL.md", "LICENSE", "kb"]
    );
}

#[test]
fn unix_release_package_rejects_a_binary_name_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let stage = temp.path().join("stage");
    fs::create_dir(&stage).unwrap();
    write_stage(&stage, "kb");
    let asset = temp
        .path()
        .join("knowledge-brain-v0.1.0-x86_64-pc-windows-msvc.zip");

    let output = package(&stage, &asset, "0.1.0", "x86_64-pc-windows-msvc");

    assert!(!output.status.success());
    assert!(!asset.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("kb.exe"));
}

#[test]
fn windows_archive_layout_is_readable_when_created_by_the_platform_script() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("knowledge-brain-v0.1.0-x86_64-pc-windows-msvc.zip");
    create_windows_layout_fixture(&archive);

    let mut archive = ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
    let mut names = (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["INSTALL.md", "LICENSE", "kb.exe"]);
}

fn package(stage: &Path, asset: &Path, version: &str, target: &str) -> std::process::Output {
    Command::new("bash")
        .arg(project_root().join("scripts/package-release.sh"))
        .arg(stage)
        .arg(asset)
        .arg(version)
        .arg(target)
        .output()
        .unwrap()
}

fn write_stage(stage: &Path, binary_name: &str) {
    fs::write(stage.join(binary_name), b"binary").unwrap();
    fs::write(stage.join("LICENSE"), b"license").unwrap();
    fs::write(stage.join("INSTALL.md"), b"install").unwrap();
}

fn create_windows_layout_fixture(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for name in ["kb.exe", "LICENSE", "INSTALL.md"] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, b"fixture").unwrap();
    }
    writer.finish().unwrap();
}

fn project_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
