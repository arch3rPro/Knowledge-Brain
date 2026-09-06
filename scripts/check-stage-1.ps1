$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$artifactDir = if ($env:STAGE1_ARTIFACT_DIR) {
    $env:STAGE1_ARTIFACT_DIR
} else {
    Join-Path $repoRoot "target/stage-1-artifacts"
}
New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

$systemTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$runRoot = Join-Path $systemTemp ("knowledge-brain-stage1-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $runRoot | Out-Null

try {
    $env:KB_CONFIG_DIR = Join-Path $runRoot "user-config"
    $env:KB_STATE_DIR = Join-Path $runRoot "user-state"
    $env:KB_CACHE_DIR = Join-Path $runRoot "user-cache"
    Set-Location $repoRoot

    cargo fmt --all --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed" }
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
    cargo test --workspace --all-targets
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
    cargo build --release -p kb-cli
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    $kb = Join-Path $repoRoot "target/release/kb.exe"
    & $kb version --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "version.json")
    if ($LASTEXITCODE -ne 0) { throw "kb version failed" }
    & $kb capabilities --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "capabilities.json")
    if ($LASTEXITCODE -ne 0) { throw "kb capabilities failed" }

    $newVault = Join-Path $runRoot "new-vault"
    & $kb init $newVault --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "init.json")
    if ($LASTEXITCODE -ne 0) { throw "kb init failed" }
    New-Item -ItemType Directory -Path (Join-Path $newVault "Notes") | Out-Null
    & $kb config admission add notes Notes --vault $newVault --yes --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "admission.json")
    if ($LASTEXITCODE -ne 0) { throw "kb admission add failed" }
    $match = Select-String -Path (Join-Path $newVault ".kb/config.yml") -Pattern '^vault_id: "([^"]+)"$'
    if (-not $match) { throw "Could not read vault_id" }
    $vaultId = $match.Matches[0].Groups[1].Value

    $movedVault = Join-Path $runRoot "moved-vault"
    Move-Item -Path $newVault -Destination $movedVault
    & $kb vault rebind $vaultId $movedVault --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "rebind.json")
    if ($LASTEXITCODE -ne 0) { throw "kb vault rebind failed" }
    & $kb status --vault $vaultId --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "reopened-status.json")
    if ($LASTEXITCODE -ne 0) { throw "kb status failed" }
    & $kb doctor --vault $vaultId --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "doctor.json")
    if ($LASTEXITCODE -ne 0) { throw "kb doctor failed" }

    $existing = Join-Path $runRoot "existing"
    New-Item -ItemType Directory -Path (Join-Path $existing "Notes") -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $existing "Notes/keep.md"), "# Existing`n`nHuman text.`n")
    $planPath = Join-Path $artifactDir "adopt-plan.json"
    & $kb adopt $existing --json | Set-Content -Encoding utf8 $planPath
    if ($LASTEXITCODE -ne 0) { throw "kb adopt failed" }
    $operationId = (Get-Content -Raw $planPath | ConvertFrom-Json).data.operation_id
    & $kb apply $operationId --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "adopt-result.json")
    if ($LASTEXITCODE -ne 0) { throw "kb apply failed" }
    & $kb status --vault $existing --json | Set-Content -Encoding utf8 (Join-Path $artifactDir "adopted-status.json")
    if ($LASTEXITCODE -ne 0) { throw "kb adopted status failed" }
    if (Test-Path (Join-Path $existing ".git")) { throw "adopt created an unauthorized .git directory" }
} finally {
    $fullRunRoot = [IO.Path]::GetFullPath($runRoot)
    if ($fullRunRoot.StartsWith($systemTemp, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path $fullRunRoot)) {
        Remove-Item -LiteralPath $fullRunRoot -Recurse -Force
    }
}
