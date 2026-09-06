use std::path::{Path, PathBuf};

use kb_core::{
    detect_portability_collisions, ensure_not_link_or_reparse_point, find_vault_root,
    validate_admission_directory,
};

#[test]
fn admission_accepts_one_portable_component_only() {
    assert!(validate_admission_directory(Path::new("Reading")).is_ok());
    for path in [
        "../Reading",
        "a/b",
        "Wiki",
        ".kb",
        "CON",
        "con.txt",
        "notes.",
        "notes ",
        "C:/notes",
    ] {
        assert!(
            validate_admission_directory(Path::new(path)).is_err(),
            "{path}"
        );
    }
}

#[test]
fn equivalent_case_and_unicode_names_collide() {
    let paths = vec![
        PathBuf::from("Wiki/Café.md"),
        PathBuf::from("wiki/Cafe\u{301}.md"),
    ];
    let collisions = detect_portability_collisions(&paths).unwrap();
    assert_eq!(collisions.len(), 1);
    assert_eq!(collisions[0].paths, paths);
}

#[test]
fn vault_root_is_found_from_a_descendant() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let nested = root.join("Notes/meetings");
    std::fs::create_dir_all(root.join(".kb")).unwrap();
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join(".kb/config.yml"), "schema_version: \"v1.0\"\n").unwrap();

    assert_eq!(find_vault_root(&nested).unwrap(), root);
}

#[cfg(unix)]
#[test]
fn symbolic_link_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let link = temp.path().join("link");
    std::fs::create_dir(&target).unwrap();
    symlink(&target, &link).unwrap();

    assert!(ensure_not_link_or_reparse_point(&link).is_err());
}
