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
$originalConfigDir = [Environment]::GetEnvironmentVariable('MIY_CONFIG_DIR', [EnvironmentVariableTarget]::Process)
$originalContext = [Environment]::GetEnvironmentVariable('MIY_CONTEXT', [EnvironmentVariableTarget]::Process)
$env:MIY_CONFIG_DIR = Join-Path $fixture 'client configuration'
$env:MIY_CONTEXT = 'unrelated-missing-context'
$clientConfig = Join-Path $env:MIY_CONFIG_DIR 'config.json'

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
    $script:InstallerResult = $null
    $script:InstallerFailure = ''
    if ($nativeExit -eq 0) {
        $script:InstallerResult = ($captured -join "`n") | ConvertFrom-Json
    } else {
        $script:InstallerFailure = ($captured -join "`n")
    }
}

try {
    # Omitted -Server never guesses an address in a noninteractive installation.
    $genericInstall = Join-Path $fixture 'generic cli'
    $genericArguments = @('-ArchivePath', $archive, '-Sha256', $checksum, '-InstallDir', $genericInstall)
    Invoke-InstallerFixture 'install-cli.ps1' $genericArguments 0
    Assert-Installer (-not [IO.File]::Exists($clientConfig)) 'A generic install created server configuration.'
    Assert-Installer ($InstallerResult.contextInitialization.status -eq 'required' -and $InstallerResult.contextInitialization.setupRequired) 'A generic install did not report pending server setup.'
    Assert-Installer ($InstallerResult.next -contains 'miy context init') 'A generic install omitted later server registration.'
    Invoke-InstallerFixture 'update-cli.ps1' $genericArguments 0
    Assert-Installer (-not [IO.File]::Exists($clientConfig)) 'A generic update created server configuration.'
    Invoke-InstallerFixture 'uninstall-cli.ps1' @('-InstallDir', $genericInstall) 0

    $arguments = @('-ArchivePath', $archive, '-Sha256', $checksum, '-InstallDir', $install, '-Server', 'https://platform.example')
    Invoke-InstallerFixture 'install-cli.ps1' @('-ArchivePath', $archive, '-Sha256', ('0' * 64), '-InstallDir', $install) 7
    Assert-Installer (-not [IO.File]::Exists((Join-Path $install 'miy.exe'))) 'A checksum failure installed a binary.'
    Invoke-InstallerFixture 'install-cli.ps1' $arguments 0
    Assert-Installer ($InstallerResult.contextInitialization.status -eq 'initialized') 'Fresh installation did not report context initialization.'
    Assert-Installer ($InstallerResult.contextInitialization.origin -eq 'https://platform.example') 'Fresh installation reported a different server.'
    Assert-Installer ($InstallerResult.next -contains 'miy --context default auth login') 'Fresh installation omitted login guidance.'
    $configuration = Read-MiyJson $clientConfig
    Assert-Installer ($configuration.active -eq 'default' -and $configuration.contexts.default.origin -eq 'https://platform.example') 'Fresh installation did not register and select the requested server.'

    # Preserve a user's selected context, other contexts and authentication binding.
    $configuration.contexts | Add-Member -NotePropertyName custom -NotePropertyValue ([pscustomobject]@{
        origin = 'https://user-selected.example'; caFile = $null; userId = 'fixture-user'
    })
    $configuration.active = 'custom'
    $configuration.contexts.default.userId = 'default-fixture-user'
    Write-MiyAtomic $clientConfig (ConvertTo-MiyBytes $configuration)
    $configBytes = [IO.File]::ReadAllBytes($clientConfig)
    $configHash = Get-MiyBytesHash $configBytes
    $credentialSentinel = Join-Path $env:MIY_CONFIG_DIR 'credentials-must-survive.txt'
    [IO.File]::WriteAllText($credentialSentinel, 'synthetic credential fixture')
    $arguments = @('-ArchivePath', $archive, '-Sha256', $checksum, '-InstallDir', $install, '-Server', 'https://another-platform.example')
    $binary = Join-Path $install 'miy.exe'
    $initialHash = Get-MiyHash $binary
    $unmanaged = Join-Path $install 'user-file.txt'
    [IO.File]::WriteAllText($unmanaged, 'preserve unmanaged data')
    $lock = [IO.FileStream]::new($binary, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try { Invoke-InstallerFixture 'update-cli.ps1' $arguments 6 } finally { $lock.Dispose() }
    Assert-Installer ((Get-MiyHash $binary) -eq $initialHash) 'A locked update changed the old binary.'
    Invoke-InstallerFixture 'update-cli.ps1' $arguments 0
    Assert-Installer ($InstallerResult.contextInitialization.status -eq 'preserved' -and $InstallerResult.contextInitialization.active -eq 'custom') 'Update did not preserve the selected context.'
    Assert-Installer ($InstallerResult.contextInitialization.serverMismatch -and $InstallerResult.contextInitialization.activeOrigin -eq 'https://user-selected.example' -and $InstallerResult.contextInitialization.requestedOrigin -eq 'https://another-platform.example') 'Update hid a different requested server.'
    Assert-Installer ((Get-MiyHash $clientConfig) -eq $configHash) 'Update changed existing contexts or authentication fields.'

    # Configuration errors happen after the binary commit and must be visible.
    $invalidBytes = [Text.Encoding]::UTF8.GetBytes('{invalid fixture')
    Write-MiyAtomic $clientConfig $invalidBytes
    Invoke-InstallerFixture 'update-cli.ps1' $arguments 7
    Assert-Installer ($InstallerFailure -match 'CLI binary is installed') 'Configuration failure hid the successful binary installation.'
    Assert-Installer ((Get-MiyHash $binary) -eq $initialHash) 'Configuration failure rolled back or damaged the installed binary.'
    Assert-Installer ((Get-MiyHash $clientConfig) -eq (Get-MiyBytesHash $invalidBytes)) 'Configuration failure reset the existing file.'
    Assert-Installer (-not [IO.File]::Exists((Join-Path $install '.miy-install-journal.json'))) 'Configuration failure retained a binary transaction.'
    Write-MiyAtomic $clientConfig $configBytes

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
    Assert-Installer ((Get-MiyHash $clientConfig) -eq $configHash) 'Recovery or uninstall changed existing server configuration.'
    Assert-Installer ([IO.File]::ReadAllText($credentialSentinel) -eq 'synthetic credential fixture') 'Installer or uninstaller changed credentials.'
    Assert-Installer ([Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User) -eq $originalPath) 'Installer changed PATH without opt-in.'
    @{ passed = 9; powershell = $PSVersionTable.PSVersion.ToString(); os = [Environment]::OSVersion.VersionString;
       scenarios = @('generic install/update without configuration', 'checksum refusal', 'fresh server registration',
           'server and authentication preservation on update', 'configuration failure after binary commit', 'file lock',
           'corrupt backup refusal', 'interruption recovery', 'uninstall preservation');
       windows11E2E = $false } | ConvertTo-Json -Compress
} finally {
    [Environment]::SetEnvironmentVariable('MIY_CONFIG_DIR', $originalConfigDir, [EnvironmentVariableTarget]::Process)
    [Environment]::SetEnvironmentVariable('MIY_CONTEXT', $originalContext, [EnvironmentVariableTarget]::Process)
    # This UUID directory contains only fixtures created by this script.
    $null = Assert-MiyPath $fixture
    if ([IO.Directory]::Exists($fixture)) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
