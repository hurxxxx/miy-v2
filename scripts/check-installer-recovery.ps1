# Platform-independent journal/byte tests. This is not a Windows installation test.
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
    @{ passed = 4; powershell = $PSVersionTable.PSVersion.ToString(); nativeWindowsInstallation = $false;
       scenarios = @('interruption and idempotence', 'corrupt backup refusal', 'user edit preservation', 'committed recovery') } | ConvertTo-Json -Compress
} finally {
    $null = Assert-MiyPath $fixture
    if ([IO.Directory]::Exists($fixture)) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
