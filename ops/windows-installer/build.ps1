[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$Sha256,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{40}$')][string]$InstallerCommit,
    [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{40}$')][string]$CliCommit,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{64}$')][string]$CliBinarySha256
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitProcess) { throw 'Build on Windows x64.' }
$archive = (Resolve-Path -LiteralPath $ArchivePath).Path
$expectedArchive = $Sha256.ToLowerInvariant()
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedArchive) { throw 'The CLI archive checksum differs from the selected candidate.' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
$null = New-Item -ItemType Directory -Path $output -Force
$stage = Join-Path ([IO.Path]::GetTempPath()) ('miy-setup-build-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $stage
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $entry = $zip.GetEntry('release.json')
        if ($null -eq $entry) { throw 'The release provenance is missing.' }
        $reader = New-Object IO.StreamReader($entry.Open())
        try { $release = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
        if ($release.schemaVersion -ne 1 -or $release.version -cne $Version -or $release.commit -cne $CliCommit -or $release.binarySha256 -cne $CliBinarySha256 -or $release.target -ne 'x86_64-pc-windows-msvc') { throw 'The CLI provenance differs from the selected candidate.' }
        $binary = $zip.GetEntry('miy.exe')
        if ($null -eq $binary -or $binary.Length -gt 536870912) { throw 'The CLI binary is missing or oversized.' }
        $stream = $binary.Open()
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { $actualBinary = [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
        finally { $stream.Dispose(); $hasher.Dispose() }
        if ($actualBinary -cne $CliBinarySha256) { throw 'The CLI binary checksum differs from the selected candidate.' }
    } finally { $zip.Dispose() }
    $metadata = [ordered]@{ schemaVersion = 1; version = $Version; target = 'windows-x64'; installerCommit = $InstallerCommit;
        cliCommit = $CliCommit; archiveSha256 = $expectedArchive; cliBinarySha256 = $CliBinarySha256 }
    $metadataPath = Join-Path $stage 'installer.json'
    [IO.File]::WriteAllText($metadataPath, ($metadata | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    # Windows PowerShell 5.1 decodes BOM-less scripts using its ANSI code page.
    # The bridge itself is ASCII; the Korean UI is C# /codepage:65001 source.
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    if (-not (Test-Path -LiteralPath $compiler)) { throw 'The Windows .NET Framework C# compiler is unavailable.' }
    $exe = Join-Path $output ('MiySetup-' + $Version + '-windows-x64.exe')
    $arguments = @('/nologo', '/target:winexe', '/platform:x64', '/optimize+', '/codepage:65001',
        ('/out:' + $exe), ('/win32manifest:' + (Join-Path $PSScriptRoot 'app.manifest')),
        '/reference:System.dll', '/reference:System.Core.dll', '/reference:System.Drawing.dll',
        '/reference:System.Windows.Forms.dll', '/reference:System.Web.Extensions.dll',
        '/reference:System.IO.Compression.dll', '/reference:System.IO.Compression.FileSystem.dll',
        ('/resource:' + $archive + ',Miy.Cli.zip'),
        ('/resource:' + (Join-Path $PSScriptRoot 'bridge.ps1') + ',Miy.Bridge.ps1'),
        ('/resource:' + $metadataPath + ',Miy.Installer.json'), (Join-Path $PSScriptRoot 'MiySetup.cs'))
    & $compiler @arguments
    if ($LASTEXITCODE -ne 0) { throw ('C# compilation failed with exit ' + $LASTEXITCODE) }
    [IO.File]::WriteAllText((Join-Path $output 'installer-provenance.json'), ($metadata | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
    Write-Output $exe
} finally { Remove-Item -LiteralPath $stage -Recurse -Force }
