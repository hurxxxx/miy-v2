[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$Sha256,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{40}$')][string]$InstallerCommit
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitProcess) { throw 'Build on Windows x64.' }
$archive = (Resolve-Path -LiteralPath $ArchivePath).Path
$expectedArchive = 'd7cc2e1b4cca63b215b525a51f85372792db5241c218185635fe693a714ea87f'
$expectedBinary = '99f590c62b44e4b61391b8986d5fa6175ebee1d072efd066bbc85747e38eac71'
$cliCommit = 'b67798bfb84f299257ba4326beaf034b125cde7a'
if ($Sha256.ToLowerInvariant() -ne $expectedArchive -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedArchive) { throw 'The archive must be the verified Windows CLI 0.6.0 release.' }
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
        if ($release.schemaVersion -ne 1 -or $release.version -ne '0.6.0' -or $release.commit -ne $cliCommit -or $release.binarySha256 -ne $expectedBinary -or $release.target -ne 'x86_64-pc-windows-msvc') { throw 'The CLI provenance is invalid.' }
    } finally { $zip.Dispose() }
    $metadata = [ordered]@{ schemaVersion = 1; version = '0.6.0'; target = 'windows-x64'; installerCommit = $InstallerCommit;
        cliCommit = $cliCommit; archiveSha256 = $expectedArchive; cliBinarySha256 = $expectedBinary }
    $metadataPath = Join-Path $stage 'installer.json'
    [IO.File]::WriteAllText($metadataPath, ($metadata | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    # Windows PowerShell 5.1 decodes BOM-less scripts using its ANSI code page.
    # The bridge itself is ASCII; the Korean UI is C# /codepage:65001 source.
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    if (-not (Test-Path -LiteralPath $compiler)) { throw 'The Windows .NET Framework C# compiler is unavailable.' }
    $exe = Join-Path $output 'MiySetup-0.6.0-windows-x64.exe'
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
