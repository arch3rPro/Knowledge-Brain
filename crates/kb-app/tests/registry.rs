use std::collections::BTreeMap;

use kb_app::{
    InitRequest, UserPaths, VaultSelection, init_and_register_vault, init_vault, list_vaults,
    rebind_vault, register_vault, resolve_vault, unregister_vault,
};

#[test]
fn initialization_and_registration_share_one_application_workflow() {
    let temp = tempfile::tempdir().unwrap();
    let user_paths = UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    );
    let root = temp.path().join("vault");

    let report = init_and_register_vault(
        &InitRequest {
            target: root.clone(),
        },
        &user_paths,
    )
    .unwrap();

    assert!(report.warnings.is_empty());
    assert_eq!(list_vaults(&user_paths).unwrap()[0].path, root);
}

#[test]
fn moved_vault_can_be_rebound_by_stable_id() {
    let temp = tempfile::tempdir().unwrap();
    let user_paths = UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    );
    let old = temp.path().join("old");
    let report = init_vault(&InitRequest {
        target: old.clone(),
    })
    .unwrap();
    register_vault(&user_paths, &old).unwrap();

    let new = temp.path().join("moved");
    std::fs::rename(&old, &new).unwrap();
    let selection = VaultSelection {
        explicit: Some(report.vault_id.to_string()),
        environment: BTreeMap::new(),
        current_dir: temp.path().to_path_buf(),
    };
    assert!(resolve_vault(&user_paths, &selection).is_err());

    rebind_vault(&user_paths, report.vault_id, &new).unwrap();
    assert_eq!(resolve_vault(&user_paths, &selection).unwrap().root, new);
    assert_eq!(list_vaults(&user_paths).unwrap().len(), 1);

    unregister_vault(&user_paths, report.vault_id).unwrap();
    assert!(list_vaults(&user_paths).unwrap().is_empty());
    assert!(new.join(".kb/config.yml").is_file());
}

#[test]
fn selection_is_explicit_then_environment_then_ancestor_then_sole_registry() {
    let temp = tempfile::tempdir().unwrap();
    let user_paths = UserPaths::new(
        temp.path().join("config"),
        temp.path().join("state"),
        temp.path().join("cache"),
    );
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let first_report = init_vault(&InitRequest {
        target: first.clone(),
    })
    .unwrap();
    init_vault(&InitRequest {
        target: second.clone(),
    })
    .unwrap();
    register_vault(&user_paths, &first).unwrap();
    register_vault(&user_paths, &second).unwrap();

    let nested = first.join("Wiki/articles");
    let mut environment = BTreeMap::new();
    environment.insert("KB_VAULT".to_owned(), second.display().to_string());
    let selected = resolve_vault(
        &user_paths,
        &VaultSelection {
            explicit: Some(first_report.vault_id.to_string()),
            environment: environment.clone(),
            current_dir: nested.clone(),
        },
    )
    .unwrap();
    assert_eq!(selected.root, first);

    let selected = resolve_vault(
        &user_paths,
        &VaultSelection {
            explicit: None,
            environment,
            current_dir: nested.clone(),
        },
    )
    .unwrap();
    assert_eq!(selected.root, second);

    let selected = resolve_vault(
        &user_paths,
        &VaultSelection {
            explicit: None,
            environment: BTreeMap::new(),
            current_dir: nested,
        },
    )
    .unwrap();
    assert_eq!(selected.root, first);
}
