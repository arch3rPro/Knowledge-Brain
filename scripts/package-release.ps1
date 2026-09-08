[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Stage,
    [Parameter(Mandatory = $true)][string]$Asset,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Target
)

$ErrorActionPreference = 'Stop'

if ($Target -ne 'x86_64-pc-windows-msvc') {
    throw "unsupported Windows release target: $Target"
}

$expectedName = "knowledge-brain-v$Version-$Target.zip"
if ((Split-Path -Leaf $Asset) -ne $expectedName) {
    throw "asset must be named $expectedName"
}

foreach ($required in @('kb.exe', 'LICENSE', 'INSTALL.md')) {
    if (-not (Test-Path -LiteralPath (Join-Path $Stage $required) -PathType Leaf)) {
        throw "stage is missing required file: $required"
    }
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Asset) | Out-Null
if (Test-Path -LiteralPath $Asset) {
    Remove-Item -LiteralPath $Asset -Force
}

Compress-Archive -LiteralPath @(
    (Join-Path $Stage 'kb.exe'),
    (Join-Path $Stage 'LICENSE'),
    (Join-Path $Stage 'INSTALL.md')
) -DestinationPath $Asset -CompressionLevel Optimal
