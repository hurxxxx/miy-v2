# Platform-independent journal/byte tests. This is not a Windows installation test.
[CmdletBinding()]
param([string]$CliPath)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-cli-common.ps1')
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('miy recovery ' + [char]0xd55c + [char]0xae00 + ' ' + [Guid]::NewGuid().ToString('N'))
$null = [IO.Directory]::CreateDirectory($fixture)
$binary = Join-Path $fixture 'miy.exe'
$marker = Join-Path $fixture '.miy-install.json'
$journalPath = Join-Path $fixture '.miy-install-journal.json'
$oldBytes = [Text.Encoding]::UTF8.GetBytes('old executable fixture')
$newBytes = [Text.Encoding]::UTF8.GetBytes('new executable fixture')
$oldMarker = ConvertTo-MiyBytes @{ schemaVersion = 1; version = 'old'; text = ([string][char]0xd55c + [char]0xae00) }
$newMarker = ConvertTo-MiyBytes @{ schemaVersion = 1; version = 'new' }
$originalConfigDir = [Environment]::GetEnvironmentVariable('MIY_CONFIG_DIR', [EnvironmentVariableTarget]::Process)
$originalContext = [Environment]::GetEnvironmentVariable('MIY_CONTEXT', [EnvironmentVariableTarget]::Process)
$env:MIY_CONFIG_DIR = Join-Path $fixture 'isolated client configuration'
$env:MIY_CONTEXT = 'unrelated-missing-context'

function Assert-Recovery([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Prepare-InterruptedUpdate {
    Write-MiyAtomic $binary $oldBytes
    Write-MiyAtomic $marker $oldMarker
    $id = [Guid]::NewGuid().ToString('N')
    $journal = Start-MiyFileUpdate $fixture $id (Get-MiyBytesHash $newBytes) (Get-MiyBytesHash $newMarker)
    $journal.phase = 'applying'
    Write-MiyAtomic $journalPath (ConvertTo-MiyBytes $journal)
    Write-MiyAtomic $binary $newBytes
    return $journal
}

try {
    $journal = Prepare-InterruptedUpdate
    Repair-MiyInstallation $fixture
    Assert-Recovery ((Get-MiyHash $binary) -eq (Get-MiyBytesHash $oldBytes)) 'Interrupted binary was not restored.'
    Assert-Recovery ((Get-MiyHash $marker) -eq (Get-MiyBytesHash $oldMarker)) 'Original UTF-8 metadata changed.'
    Assert-Recovery (-not [IO.File]::Exists($journalPath)) 'Successful rollback retained an active journal.'
    Repair-MiyInstallation $fixture

    $journal = Prepare-InterruptedUpdate
    $paths = Get-MiyJournalPaths $fixture $journal.id
    $backup = Join-Path $paths.Backup 'miy.exe'
    [IO.File]::WriteAllText($backup, 'corrupt backup')
    $refused = $false
    try { Repair-MiyInstallation $fixture } catch { $refused = $true }
    Assert-Recovery $refused 'Corrupt backup was accepted.'
    Assert-Recovery ((Get-MiyHash $binary) -eq (Get-MiyBytesHash $newBytes)) 'Corrupt backup overwrote the current binary.'
    Assert-Recovery ([IO.File]::Exists($journalPath)) 'Failed recovery discarded its journal.'
    [IO.File]::WriteAllBytes($backup, $oldBytes)
    Repair-MiyInstallation $fixture

    $journal = Prepare-InterruptedUpdate
    [IO.File]::WriteAllText($binary, 'user edit')
    $refused = $false
    try { Repair-MiyInstallation $fixture } catch { $refused = $true }
    Assert-Recovery $refused 'An unrelated user edit was accepted for rollback.'
    Assert-Recovery ([IO.File]::ReadAllText($binary) -eq 'user edit') 'An unrelated user edit was overwritten.'
    Write-MiyAtomic $binary $oldBytes
    Repair-MiyInstallation $fixture

    $journal = Prepare-InterruptedUpdate
    Write-MiyAtomic $marker $newMarker
    $journal.phase = 'committed'
    Write-MiyAtomic $journalPath (ConvertTo-MiyBytes $journal)
    Repair-MiyInstallation $fixture
    Assert-Recovery ((Get-MiyHash $binary) -eq (Get-MiyBytesHash $newBytes)) 'A committed binary was rolled back.'
    Assert-Recovery ((Get-MiyHash $marker) -eq (Get-MiyBytesHash $newMarker)) 'Committed metadata was rolled back.'
    Assert-Recovery (-not [IO.File]::Exists($journalPath)) 'Committed recovery retained an active journal.'
    $scenarios = @('interruption and idempotence', 'corrupt backup refusal', 'user edit preservation', 'committed recovery')

    if (-not [string]::IsNullOrEmpty($CliPath)) {
        $cli = [IO.Path]::GetFullPath($CliPath)
        $required = Initialize-MiyContext $cli ''
        Assert-Recovery ($required.status -eq 'required' -and $required.setupRequired) 'The adapter did not report missing server setup.'
        Assert-Recovery (-not [IO.File]::Exists((Join-Path $env:MIY_CONFIG_DIR 'config.json'))) 'The adapter invented a server when no address was supplied.'
        $scenarios += 'native omitted address requires later registration'
        $result = Initialize-MiyContext $cli 'https://configured.example/'
        Assert-Recovery ($result.status -eq 'initialized' -and $result.origin -eq 'https://configured.example' -and $result.active -eq 'default') 'The adapter did not verify a fresh context.'
        $configurationPath = Join-Path $env:MIY_CONFIG_DIR 'config.json'
        $configuration = Read-MiyJson $configurationPath
        Assert-Recovery ($configuration.active -eq 'default' -and $configuration.contexts.default.origin -eq 'https://configured.example') 'The adapter did not initialize the supplied server.'
        $scenarios += 'native context initialization adapter'

        $configuration.contexts.default.userId = 'fixture-user'
        Write-MiyAtomic $configurationPath (ConvertTo-MiyBytes $configuration)
        $configurationBytes = [IO.File]::ReadAllBytes($configurationPath)
        $configurationHash = Get-MiyBytesHash $configurationBytes
        $result = Initialize-MiyContext $cli 'https://different.example'
        Assert-Recovery ($result.status -eq 'preserved' -and $result.active -eq 'default') 'The adapter did not report preserved configuration.'
        Assert-Recovery ($result.serverMismatch -and $result.activeOrigin -eq 'https://configured.example' -and $result.requestedOrigin -eq 'https://different.example') 'The adapter hid a different requested server.'
        Assert-Recovery ((Get-MiyHash $configurationPath) -eq $configurationHash) 'The adapter changed a server or authentication binding.'
        $scenarios += 'native existing context preservation'

        $failure = $null
        try { $null = Initialize-MiyContext $cli 'https://invalid.example/path' } catch { $failure = Get-MiyInstallFailure $_ }
        Assert-Recovery ($null -ne $failure -and $failure.Code -eq 2 -and $failure.Message -match 'CLI binary is installed') 'The adapter ignored a native nonzero exit or hid the committed binary.'
        Assert-Recovery ((Get-MiyHash $configurationPath) -eq $configurationHash) 'An invalid server URL changed configuration.'
        $scenarios += 'native nonzero exit propagation'

        $invalidBytes = [Text.Encoding]::UTF8.GetBytes('{invalid fixture')
        Write-MiyAtomic $configurationPath $invalidBytes
        $failure = $null
        try { $null = Initialize-MiyContext $cli 'https://configured.example' } catch { $failure = Get-MiyInstallFailure $_ }
        Assert-Recovery ($null -ne $failure -and $failure.Code -eq 7) 'The adapter accepted malformed existing configuration.'
        Assert-Recovery ((Get-MiyHash $configurationPath) -eq (Get-MiyBytesHash $invalidBytes)) 'The adapter reset malformed existing configuration.'
        $scenarios += 'native malformed configuration refusal'

        $configuration.active = $null
        $configuration.contexts = [pscustomobject]@{}
        Write-MiyAtomic $configurationPath (ConvertTo-MiyBytes $configuration)
        $configurationHash = Get-MiyHash $configurationPath
        $result = Initialize-MiyContext $cli 'https://configured.example'
        Assert-Recovery ($result.status -eq 'preserved' -and $null -eq $result.active) 'The adapter did not preserve an intentionally empty configuration.'
        Assert-Recovery ($result.setupRequired -and $result.next[0].args[0] -eq 'context' -and $result.next[0].args[1] -eq 'add') 'The adapter did not provide recovery for an intentionally empty configuration.'
        Assert-Recovery ((Get-MiyHash $configurationPath) -eq $configurationHash) 'The adapter rewrote an intentionally empty configuration.'
        $scenarios += 'native empty configuration preservation'
    }
    @{ passed = $scenarios.Count; powershell = $PSVersionTable.PSVersion.ToString(); nativeWindowsInstallation = $false;
       scenarios = $scenarios } | ConvertTo-Json -Compress
} finally {
    [Environment]::SetEnvironmentVariable('MIY_CONFIG_DIR', $originalConfigDir, [EnvironmentVariableTarget]::Process)
    [Environment]::SetEnvironmentVariable('MIY_CONTEXT', $originalContext, [EnvironmentVariableTarget]::Process)
    $null = Assert-MiyPath $fixture
    if ([IO.Directory]::Exists($fixture)) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
