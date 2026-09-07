use assert_cmd::Command;
use serde_json::Value;
use std::{fs, path::Path};

#[test]
fn optional_bm25f_is_ranked_explainable_incremental_and_strict_when_requested() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let vault = base.join("vault");
    let path = vault.to_str().unwrap();
    run(base, &["init", path, "--json"]);
    fs::write(
        vault.join("Wiki/articles/title.md"),
        article("Needle", "普通正文"),
    )
    .unwrap();
    fs::write(
        vault.join("Wiki/articles/body.md"),
        article("Other", "needle"),
    )
    .unwrap();
    run(
        base,
        &[
            "config",
            "set",
            "search.mode",
            "bm25",
            "--vault",
            path,
            "--yes",
            "--json",
        ],
    );

    let fallback = run(base, &["query", "needle", "--vault", path, "--json"]);
    assert!(!fallback["data"]["warnings"].as_array().unwrap().is_empty());
    run(base, &["cache", "rebuild", "--vault", path, "--json"]);
    let ranked = run(base, &["query", "needle", "--vault", path, "--json"]);
    assert_ranked(&ranked);
    let cjk = run(base, &["query", "知识检索", "--vault", path, "--json"]);
    assert!(
        !cjk["data"]["groups"][0]["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    fs::write(
        vault.join("Wiki/articles/body.md"),
        article("Other", "new-index-content"),
    )
    .unwrap();
    let stale = run(
        base,
        &["query", "new-index-content", "--vault", path, "--json"],
    );
    assert_eq!(
        stale["data"]["groups"][0]["results"][0]["backend"],
        "direct"
    );
    let strict = command(base)
        .args([
            "query",
            "new-index-content",
            "--vault",
            path,
            "--strict-backend",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(!strict.status.success());
    assert!(strict.stderr.is_empty());
    let error: Value = serde_json::from_slice(&strict.stdout).unwrap();
    assert_eq!(error["error"]["code"], "index_stale");

    run(base, &["cache", "rebuild", "--vault", path, "--json"]);
    let refreshed = run(
        base,
        &[
            "query",
            "new-index-content",
            "--vault",
            path,
            "--strict-backend",
            "--json",
        ],
    );
    assert_eq!(
        refreshed["data"]["groups"][0]["results"][0]["backend"],
        "bm25f"
    );
}

fn assert_ranked(response: &Value) {
    let hit = &response["data"]["groups"][0]["results"][0];
    assert_eq!(hit["path"], "Wiki/articles/title.md");
    assert_eq!(hit["backend"], "bm25f");
    assert!(hit["score_micros"].as_u64().unwrap() > 0);
    assert!(
        hit["explanation"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field["field"] == "title")
    );
}

fn article(title: &str, body: &str) -> String {
    format!(
        "---\ntype: Article\ntitle: {title}\naliases: [知识检索]\ntags: [local]\n---\n\n# Section\n{body}\n"
    )
}

fn run(base: &Path, arguments: &[&str]) -> Value {
    let output = command(base).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "args={arguments:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn command(base: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}
