[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Asset,
    [Parameter(Mandatory = $true)][string]$Target
)

$ErrorActionPreference = 'Stop'
if ($Target -ne 'x86_64-pc-windows-msvc') {
    throw "unsupported Windows release target: $Target"
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($Asset)
try {
    $entries = @($archive.Entries | ForEach-Object FullName | Sort-Object)
    $expected = @('INSTALL.md', 'LICENSE', 'kb.exe')
    if (Compare-Object -ReferenceObject $expected -DifferenceObject $entries) {
        throw "unexpected archive entries: $($entries -join ', ')"
    }
}
finally {
    $archive.Dispose()
}
