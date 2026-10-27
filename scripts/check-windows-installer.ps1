[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$ArchivePath)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-cli-common.ps1')
Assert-MiyWindows
$shell = (Get-Process -Id $PID).Path
$archive = [IO.Path]::GetFullPath($ArchivePath)
$checksum = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('miy installer ' + [char]0xd55c + [char]0xae00 + ' ' + [Guid]::NewGuid().ToString('N'))
$install = Join-Path $fixture 'installed cli'
$null = [IO.Directory]::CreateDirectory($fixture)
$config = Join-Path $fixture 'configuration-must-survive.txt'
[IO.File]::WriteAllText($config, 'preserve configuration')
$originalPath = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User)

function Assert-Installer([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Invoke-InstallerFixture([string]$Script, [string[]]$Arguments, [int]$Expected) {
    $previous = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $captured = & $shell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot $Script) @Arguments 2>&1
        $nativeExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    Assert-Installer ($nativeExit -eq $Expected) ('Unexpected installer exit code: ' + $nativeExit + '; expected ' + $Expected)
}

try {
    $arguments = @('-ArchivePath', $archive, '-Sha256', $checksum, '-InstallDir', $install)
    Invoke-InstallerFixture 'install-cli.ps1' @('-ArchivePath', $archive, '-Sha256', ('0' * 64), '-InstallDir', $install) 7
    Assert-Installer (-not [IO.File]::Exists((Join-Path $install 'miy.exe'))) 'A checksum failure installed a binary.'
    Invoke-InstallerFixture 'install-cli.ps1' $arguments 0
    $binary = Join-Path $install 'miy.exe'
    $initialHash = Get-MiyHash $binary
    $unmanaged = Join-Path $install 'user-file.txt'
    [IO.File]::WriteAllText($unmanaged, 'preserve unmanaged data')
    $lock = [IO.FileStream]::new($binary, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try { Invoke-InstallerFixture 'update-cli.ps1' $arguments 6 } finally { $lock.Dispose() }
    Assert-Installer ((Get-MiyHash $binary) -eq $initialHash) 'A locked update changed the old binary.'
    Invoke-InstallerFixture 'update-cli.ps1' $arguments 0

    # Simulate a process loss after the binary replacement and before the
    # marker replacement. The next external installer must restore/retry it.
    $installation = Open-MiyInstallation $install $PSScriptRoot
    try {
        $id = [Guid]::NewGuid().ToString('N')
        $originalBytes = [IO.File]::ReadAllBytes($binary)
        $interruptedBytes = [byte[]]($originalBytes + [byte]0)
        $marker = Read-MiyJson (Join-Path $install '.miy-install.json')
        $marker.binarySha256 = Get-MiyBytesHash $interruptedBytes
        $markerBytes = ConvertTo-MiyBytes $marker
        $journal = Start-MiyFileUpdate $install $id (Get-MiyBytesHash $interruptedBytes) (Get-MiyBytesHash $markerBytes)
        $journal.phase = 'applying'
        Write-MiyAtomic (Join-Path $install '.miy-install-journal.json') (ConvertTo-MiyBytes $journal)
        Write-MiyAtomic $binary $interruptedBytes
        $paths = Get-MiyJournalPaths $install $id
        $backup = Join-Path $paths.Backup 'miy.exe'
        [IO.File]::WriteAllText($backup, 'corrupt backup fixture')
    } finally { $installation.Lock.Dispose() }
    Invoke-InstallerFixture 'update-cli.ps1' $arguments 6
    Assert-Installer ((Get-MiyHash $binary) -eq (Get-MiyBytesHash $interruptedBytes)) 'Corrupt backup recovery overwrote current files.'
    Assert-Installer ([IO.File]::Exists((Join-Path $install '.miy-install-journal.json'))) 'Failed recovery discarded its journal.'
    [IO.File]::WriteAllBytes($backup, $originalBytes)
    Invoke-InstallerFixture 'update-cli.ps1' $arguments 0
    Assert-Installer ((Get-MiyHash $binary) -eq $initialHash) 'Interrupted installation did not recover the expected version.'
    Assert-Installer (-not [IO.File]::Exists((Join-Path $install '.miy-install-journal.json'))) 'Successful recovery left an active journal.'
    Invoke-InstallerFixture 'uninstall-cli.ps1' @('-InstallDir', $install) 0
    Assert-Installer (-not [IO.File]::Exists($binary)) 'Uninstall retained the installed binary.'
    Assert-Installer ([IO.File]::ReadAllText($unmanaged) -eq 'preserve unmanaged data') 'Uninstall removed an unmanaged file.'
    Assert-Installer ([IO.File]::ReadAllText($config) -eq 'preserve configuration') 'Installer changed configuration outside its directory.'
    Assert-Installer ([Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User) -eq $originalPath) 'Installer changed PATH without opt-in.'
    @{ passed = 6; powershell = $PSVersionTable.PSVersion.ToString(); os = [Environment]::OSVersion.VersionString;
       scenarios = @('checksum refusal', 'install/update', 'file lock', 'corrupt backup refusal', 'interruption recovery', 'uninstall preservation');
       windows11E2E = $false } | ConvertTo-Json -Compress
} finally {
    # This UUID directory contains only fixtures created by this script.
    $null = Assert-MiyPath $fixture
    if ([IO.Directory]::Exists($fixture)) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
