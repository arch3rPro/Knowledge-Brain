use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    process::{Child, Command as StdCommand, Stdio},
};

use assert_cmd::Command;

struct RunningChild(Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_reports_its_port_and_enforces_auth_on_real_requests() {
    let temporary = tempfile::tempdir().unwrap();
    let vault = temporary.path().join("vault");
    run(
        temporary.path(),
        &["init", vault.to_str().unwrap(), "--json"],
    );
    let token_file = temporary.path().join("token");
    std::fs::write(&token_file, "secret\n").unwrap();

    let mut command = spawn_command(temporary.path());
    let mut child = command
        .args([
            "serve",
            "--bind",
            "127.0.0.1:0",
            "--token-file",
            token_file.to_str().unwrap(),
            "--vault",
            vault.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout);
    let mut startup = String::new();
    lines.read_line(&mut startup).unwrap();
    let startup: serde_json::Value = serde_json::from_str(&startup).unwrap();
    assert_eq!(startup["schema_version"], "v1.0");
    assert_eq!(startup["data"]["authentication_required"], true);
    assert_eq!(startup["data"]["allow_write"], false);
    let address = startup["data"]["bind"].as_str().unwrap();
    let child = RunningChild(child);

    let (status, response) = http(address, None);
    assert_eq!(status, 401);
    assert_eq!(response["error"]["code"], "auth_denied");

    let (status, response) = http(address, Some("secret"));
    assert_eq!(status, 200);
    assert_eq!(response["schema_version"], "v1.0");
    assert_eq!(response["data"]["http"], true);

    drop(child);
}

fn http(address: &str, token: Option<&str>) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(address).unwrap();
    let authorization = token.map_or_else(String::new, |value| {
        format!("Authorization: Bearer {value}\r\n")
    });
    write!(
        stream,
        "GET /capabilities HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{authorization}\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

fn run(base: &Path, arguments: &[&str]) {
    let output = command(base).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn command(base: &Path) -> Command {
    let mut command = Command::cargo_bin("kb").unwrap();
    command
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}

fn spawn_command(base: &Path) -> StdCommand {
    let mut command = StdCommand::new(assert_cmd::cargo::cargo_bin("kb"));
    command
        .env("KB_CONFIG_DIR", base.join("config"))
        .env("KB_STATE_DIR", base.join("state"))
        .env("KB_CACHE_DIR", base.join("cache"));
    command
}
