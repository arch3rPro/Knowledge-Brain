use std::{collections::BTreeMap, fs, path::Path};

use kb_app::{
    OperationState, UserPaths, apply_operation, create_adoption_plan, inspect_operation,
    list_vaults,
};
use kb_core::{ErrorCode, OperationKind};

#[test]
fn adopt_planning_does_not_change_the_target() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir_all(target.join("Notes")).unwrap();
    fs::write(target.join("Notes/keep.md"), b"human text\n").unwrap();
    let user_paths = test_user_paths(temp.path());
    let before = snapshot_tree(&target);

    let plan = create_adoption_plan(&target, &user_paths).unwrap();

    assert_eq!(snapshot_tree(&target), before);
    assert_eq!(plan.kind, OperationKind::AdoptVault);
    assert!(
        plan.creates
            .iter()
            .all(|file| !file.relative_path.as_str().starts_with("Notes/"))
    );
    assert!(matches!(
        inspect_operation(&user_paths, plan.operation_id).unwrap(),
        OperationState::Planned(_)
    ));
}

#[test]
fn stale_plan_changes_nothing_and_success_preserves_existing_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir_all(target.join("Notes")).unwrap();
    let note = target.join("Notes/keep.md");
    fs::write(&note, b"human text\n").unwrap();
    let user_paths = test_user_paths(temp.path());

    let stale = create_adoption_plan(&target, &user_paths).unwrap();
    fs::write(&note, b"changed after review\n").unwrap();
    let before_failed_apply = snapshot_tree(&target);
    let error = apply_operation(&user_paths, stale.operation_id).unwrap_err();
    assert_eq!(error.code, ErrorCode::PlanStale);
    assert_eq!(snapshot_tree(&target), before_failed_apply);

    let plan = create_adoption_plan(&target, &user_paths).unwrap();
    let result = apply_operation(&user_paths, plan.operation_id).unwrap();
    assert_eq!(fs::read(&note).unwrap(), b"changed after review\n");
    assert!(target.join(".kb/config.yml").is_file());
    assert!(target.join("Wiki/articles").is_dir());
    assert!(!target.join(".git").exists());
    let admission = fs::read_to_string(target.join("admission.yml")).unwrap();
    assert!(admission.contains("directories: []"));
    assert_eq!(list_vaults(&user_paths).unwrap()[0].path, target);

    let repeated = apply_operation(&user_paths, plan.operation_id).unwrap();
    assert_eq!(repeated, result);
    assert!(matches!(
        inspect_operation(&user_paths, plan.operation_id).unwrap(),
        OperationState::Applied(_)
    ));
}

#[test]
fn adoption_rejects_framework_collisions_and_links() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("KB.md"), b"mine\n").unwrap();
    let error = create_adoption_plan(&target, &test_user_paths(temp.path())).unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsafePath);
}

#[test]
fn apply_never_removes_an_unrecorded_user_created_template_file() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("existing");
    fs::create_dir(&target).unwrap();
    let user_paths = test_user_paths(temp.path());
    let plan = create_adoption_plan(&target, &user_paths).unwrap();
    let user_file = target.join("KB.md");
    fs::write(
        &user_file,
        include_bytes!("../../../assets/vault-template/KB.md"),
    )
    .unwrap();

    let error = apply_operation(&user_paths, plan.operation_id).unwrap_err();

    assert_eq!(error.code, ErrorCode::PlanStale);
    assert_eq!(
        fs::read(&user_file).unwrap(),
        include_bytes!("../../../assets/vault-template/KB.md")
    );
    assert!(!target.join(".kb").exists());
}

fn test_user_paths(root: &Path) -> UserPaths {
    UserPaths::new(
        root.join("user-config"),
        root.join("user-state"),
        root.join("user-cache"),
    )
}

fn snapshot_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                output.insert(format!("{relative}/"), Vec::new());
                visit(root, &path, output);
            } else {
                output.insert(relative, fs::read(path).unwrap());
            }
        }
    }

    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}
