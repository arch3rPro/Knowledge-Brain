$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$previousLocation = Get-Location
$previousBinary = $env:KB_TEST_BINARY
try {
    Set-Location $repoRoot
    cargo fmt --all --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed" }
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
    cargo test --workspace --all-targets
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
    cargo build --release -p kb-cli
    if ($LASTEXITCODE -ne 0) { throw "release build failed" }
    $env:KB_TEST_BINARY = Join-Path $repoRoot "target/release/kb.exe"
    cargo test -p kb-cli --test phase2_journey
    if ($LASTEXITCODE -ne 0) { throw "release CLI journey failed" }
} finally {
    $env:KB_TEST_BINARY = $previousBinary
    Set-Location $previousLocation
}
