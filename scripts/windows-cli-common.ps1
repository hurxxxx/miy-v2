# PowerShell 5.1/7. Run from an extracted, verified release outside InstallDir.
# Credentials, projects, application databases and global execution policy are
# outside this installer's scope. No process is terminated to replace a file.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Stop-MiyInstall([string]$Message, [int]$Code = 7) {
    $exception = [System.InvalidOperationException]::new($Message)
    $exception.Data['MiyExitCode'] = $Code
    throw $exception
}

function Get-MiyInstallFailure($ErrorRecord) {
    $exception = $ErrorRecord.Exception
    while ($null -ne $exception) {
        if ($exception.Data.Contains('MiyExitCode')) {
            return @{ Code = [int]$exception.Data['MiyExitCode']; Message = $exception.Message; Known = $true }
        }
        $exception = $exception.InnerException
    }
    return @{ Code = 7; Message = 'The installer could not complete. Preserve the installation journal and backups, close running miy processes, and retry the external installer.'; Known = $false }
}

function Assert-MiyWindows {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
        Stop-MiyInstall 'Use 64-bit PowerShell on Windows x64.'
    }
    if ($PSVersionTable.PSVersion -lt [version]'5.1') { Stop-MiyInstall 'PowerShell 5.1 or newer is required.' }
}

function Assert-MiyPath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if ($full.StartsWith('\\')) { Stop-MiyInstall 'Use a local drive for the CLI installation.' }
    $cursor = $full
    while (-not [string]::IsNullOrEmpty($cursor)) {
        if (Test-Path -LiteralPath $cursor) {
            if (([IO.File]::GetAttributes($cursor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                Stop-MiyInstall 'Installation paths must not traverse a symbolic link or junction.' 6
            }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    return $full
}

function Get-MiyHash([string]$Path) {
    $null = Assert-MiyPath $Path
    if (-not [IO.File]::Exists($Path)) { return $null }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-MiyBytesHash([byte[]]$Bytes) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($Bytes)).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function ConvertTo-MiyBytes($Value) {
    return [Text.UTF8Encoding]::new($false, $true).GetBytes(($Value | ConvertTo-Json -Depth 12 -Compress) + "`n")
}

function Write-MiyAtomic([string]$Path, [byte[]]$Bytes) {
    $null = Assert-MiyPath $Path
    $temporary = $Path + '.write-' + [Guid]::NewGuid().ToString('N')
    try {
        $stream = [IO.FileStream]::new($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $stream.Write($Bytes, 0, $Bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
        if ([IO.File]::Exists($Path)) { [IO.File]::Replace($temporary, $Path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $Path) }
    } finally { if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) } }
}

function Read-MiyJson([string]$Path) {
    $null = Assert-MiyPath $Path
    if ((Get-Item -LiteralPath $Path -Force).Length -gt 1048576) { Stop-MiyInstall 'Installation metadata is too large.' }
    try { return [IO.File]::ReadAllText($Path, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json }
    catch { Stop-MiyInstall 'Installation metadata is invalid; preserve it for inspection.' }
}

function Assert-MiyReplaceable([string]$Path) {
    $null = Assert-MiyPath $Path
    if (-not [IO.File]::Exists($Path)) { return }
    try {
        $probe = [IO.FileStream]::new($Path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $probe.Dispose()
    } catch { Stop-MiyInstall 'The installed CLI or metadata is in use. Close its CLI/MCP processes and retry this external installer.' 6 }
}

function Open-MiyInstallation([string]$InstallDir, [string]$InstallerDirectory, [switch]$Create) {
    Assert-MiyWindows
    $root = Assert-MiyPath $InstallDir
    if ($root -eq [IO.Path]::GetPathRoot($root)) { Stop-MiyInstall 'Choose a dedicated installation directory, not a drive root.' 6 }
    $root = $root.TrimEnd('\')
    $source = [IO.Path]::GetFullPath($InstallerDirectory).TrimEnd('\')
    if ($source.Equals($root, [StringComparison]::OrdinalIgnoreCase) -or $source.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
        Stop-MiyInstall 'Run the installer from an extracted release outside the installation directory.' 6
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $ownership = Join-Path $root '.miy-install-root.json'
    if (-not [IO.Directory]::Exists($root)) {
        if (-not $Create) { return $null }
        $null = [IO.Directory]::CreateDirectory($root)
    }
    if (-not [IO.File]::Exists($ownership)) {
        if (-not $Create) { Stop-MiyInstall 'This directory is not owned by the miy installer.' 6 }
        if (@(Get-ChildItem -LiteralPath $root -Force).Count -ne 0) {
            Stop-MiyInstall 'The installation directory contains unmanaged files. Choose an empty directory.' 6
        }
        # Restrict the new install directory before creating files within it.
        $acl = [Security.AccessControl.DirectorySecurity]::new()
        $acl.SetOwner($identity)
        $acl.SetAccessRuleProtection($true, $false)
        $rule = [Security.AccessControl.FileSystemAccessRule]::new($identity, [Security.AccessControl.FileSystemRights]::FullControl,
            [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit', [Security.AccessControl.PropagationFlags]::None,
            [Security.AccessControl.AccessControlType]::Allow)
        $acl.AddAccessRule($rule)
        Set-Acl -LiteralPath $root -AclObject $acl
        Write-MiyAtomic $ownership (ConvertTo-MiyBytes @{ schemaVersion = 1; application = 'miy-cli'; directory = $root; ownerSid = $identity.Value })
    }
    $owner = Read-MiyJson $ownership
    if ($owner.schemaVersion -ne 1 -or $owner.application -ne 'miy-cli' -or $owner.ownerSid -ne $identity.Value -or
        -not $root.Equals([string]$owner.directory, [StringComparison]::OrdinalIgnoreCase)) {
        Stop-MiyInstall 'Installation ownership does not match this user and directory.' 6
    }
    $acl = Get-Acl -LiteralPath $root
    if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $identity.Value) { Stop-MiyInstall 'The install directory is owned by another user.' 6 }
    $writeRights = [Security.AccessControl.FileSystemRights]'Write, Delete, ChangePermissions, TakeOwnership'
    foreach ($rule in $acl.Access) {
        if ($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
            ($rule.FileSystemRights -band $writeRights) -ne 0 -and
            $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -ne $identity.Value) {
            Stop-MiyInstall 'The install directory permits another identity to modify its files.' 6
        }
    }
    $null = Assert-MiyPath (Join-Path $root '.miy-install.lock')
    try { $lock = [IO.FileStream]::new((Join-Path $root '.miy-install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
    catch { Stop-MiyInstall 'Another installer or uninstaller is using this installation.' 6 }
    return @{ Root = $root; Lock = $lock }
}

function Get-MiyJournalPaths([string]$Root, [string]$Id) {
    $parsed = [Guid]::Empty
    if (-not [Guid]::TryParseExact($Id, 'N', [ref]$parsed)) { Stop-MiyInstall 'The installation recovery ID is invalid.' }
    $stage = Assert-MiyPath (Join-Path $Root ('.miy-stage-' + $Id))
    $backup = Assert-MiyPath (Join-Path (Join-Path $Root '.miy-backups') $Id)
    return @{ Stage = $stage; Backup = $backup }
}

function Restore-MiyFile([string]$Target, [string]$Backup, $Before, $After) {
    $actual = Get-MiyHash $Target
    if ($actual -eq $Before) { return }
    if ($actual -ne $After) { Stop-MiyInstall 'A file changed outside the interrupted installation. Preserve the journal and backups for recovery.' 6 }
    Assert-MiyReplaceable $Target
    if ($null -eq $Before) {
        if ([IO.File]::Exists($Target)) { [IO.File]::Delete($Target) }
    } else {
        if ((Get-MiyHash $Backup) -ne $Before) { Stop-MiyInstall 'The installation backup failed verification. Current files were preserved.' }
        Write-MiyAtomic $Target ([IO.File]::ReadAllBytes($Backup))
    }
}

function Remove-MiyStage([string]$Stage) {
    $null = Assert-MiyPath $Stage
    if (-not [IO.Directory]::Exists($Stage)) { return }
    $items = @(Get-ChildItem -LiteralPath $Stage -Force)
    foreach ($item in $items) {
        $null = Assert-MiyPath $item.FullName
        if ($item.PSIsContainer -or $item.Name -notin @('release.zip', 'archive.sha256', 'miy.exe', 'release.json')) {
            Stop-MiyInstall 'The installer stage contains an unmanaged item; preserve it for inspection.' 6
        }
    }
    foreach ($item in $items) { [IO.File]::Delete($item.FullName) }
    [IO.Directory]::Delete($Stage)
}

function Repair-MiyInstallation([string]$Root) {
    $path = Join-Path $Root '.miy-install-journal.json'
    if (-not [IO.File]::Exists($path)) { return }
    $journal = Read-MiyJson $path
    if ($journal.schemaVersion -ne 1 -or $journal.directory -ne $Root -or $journal.phase -notin @('prepared', 'applying', 'committed')) {
        Stop-MiyInstall 'The installation recovery journal is invalid.'
    }
    foreach ($field in @('beforeBinary', 'afterBinary', 'beforeMarker', 'afterMarker')) {
        if ($null -ne $journal.$field -and $journal.$field -notmatch '^[a-f0-9]{64}$') { Stop-MiyInstall 'The installation recovery hashes are invalid.' }
    }
    $paths = Get-MiyJournalPaths $Root $journal.id
    if ($journal.phase -ne 'committed') {
        Restore-MiyFile (Join-Path $Root 'miy.exe') (Join-Path $paths.Backup 'miy.exe') $journal.beforeBinary $journal.afterBinary
        Restore-MiyFile (Join-Path $Root '.miy-install.json') (Join-Path $paths.Backup 'install.json') $journal.beforeMarker $journal.afterMarker
        $journal.phase = 'rolled_back'
    }
    Write-MiyAtomic (Join-Path $paths.Backup 'transaction.json') (ConvertTo-MiyBytes $journal)
    Remove-MiyStage $paths.Stage
    [IO.File]::Delete($path)
}

function Assert-MiyOwnedBinary([string]$Root) {
    $binary = Join-Path $Root 'miy.exe'
    $marker = Join-Path $Root '.miy-install.json'
    if (-not [IO.File]::Exists($marker)) {
        if ([IO.File]::Exists($binary)) { Stop-MiyInstall 'An unmanaged miy.exe exists in the install directory.' 6 }
        return
    }
    $state = Read-MiyJson $marker
    if ($state.schemaVersion -ne 1 -or $state.target -ne 'x86_64-pc-windows-msvc' -or $state.binarySha256 -ne (Get-MiyHash $binary)) {
        Stop-MiyInstall 'The installed binary differs from its ownership record. Preserve it before reinstalling.' 6
    }
}

function Start-MiyFileUpdate([string]$Root, [string]$Id, $AfterBinary, $AfterMarker) {
    $binary = Join-Path $Root 'miy.exe'
    $marker = Join-Path $Root '.miy-install.json'
    Assert-MiyReplaceable $binary
    Assert-MiyReplaceable $marker
    $paths = Get-MiyJournalPaths $Root $Id
    $null = [IO.Directory]::CreateDirectory($paths.Backup)
    $beforeBinary = Get-MiyHash $binary
    $beforeMarker = Get-MiyHash $marker
    if ($null -ne $beforeBinary) { [IO.File]::Copy($binary, (Join-Path $paths.Backup 'miy.exe')) }
    if ($null -ne $beforeMarker) { [IO.File]::Copy($marker, (Join-Path $paths.Backup 'install.json')) }
    $journal = @{ schemaVersion = 1; directory = $Root; id = $Id; phase = 'prepared';
        beforeBinary = $beforeBinary; afterBinary = $AfterBinary; beforeMarker = $beforeMarker; afterMarker = $AfterMarker }
    Write-MiyAtomic (Join-Path $Root '.miy-install-journal.json') (ConvertTo-MiyBytes $journal)
    return $journal
}

function Set-MiyUserPath([string]$Root, [switch]$Remove) {
    $value = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::User)
    $parts = @($value -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $others = @($parts | Where-Object { -not $_.TrimEnd('\').Equals($Root, [StringComparison]::OrdinalIgnoreCase) })
    if (-not $Remove) { $others += $Root }
    [Environment]::SetEnvironmentVariable('Path', ($others -join ';'), [EnvironmentVariableTarget]::User)
}

function Get-MiyReleaseArchive([string]$Stage, [string]$ArchivePath, [string]$Sha256, [string]$Version, [string]$Repository) {
    $archive = Join-Path $Stage 'release.zip'
    if (-not [string]::IsNullOrEmpty($ArchivePath)) {
        if ($Sha256 -notmatch '^[a-fA-F0-9]{64}$') { Stop-MiyInstall 'A local ZIP requires its expected SHA-256.' 2 }
        [IO.File]::Copy((Assert-MiyPath $ArchivePath), $archive)
    } else {
        if ($Version -notmatch '^\d+\.\d+\.\d+$' -or $Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
            Stop-MiyInstall 'Provide an explicit release version and owner/repository.' 2
        }
        $name = 'miy-' + $Version + '-x86_64-pc-windows-msvc.zip'
        $url = 'https://github.com/' + $Repository + '/releases/download/v' + $Version + '/' + $name
        $previousTls = [Net.ServicePointManager]::SecurityProtocol
        try {
            [Net.ServicePointManager]::SecurityProtocol = $previousTls -bor [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archive -TimeoutSec 120
            $checksum = Join-Path $Stage 'archive.sha256'
            Invoke-WebRequest -UseBasicParsing -Uri ($url + '.sha256') -OutFile $checksum -TimeoutSec 30
            $line = [IO.File]::ReadAllText($checksum).Trim()
            if ($line -notmatch ('^([a-fA-F0-9]{64})\s+\*?' + [regex]::Escape($name) + '$')) { Stop-MiyInstall 'The release checksum file is invalid.' }
            $Sha256 = $Matches[1]
        } catch {
            if ((Get-MiyInstallFailure $_).Known) { throw }
            Stop-MiyInstall 'The release could not be downloaded or its checksum could not be read.' 8
        }
        finally { [Net.ServicePointManager]::SecurityProtocol = $previousTls }
    }
    if ((Get-Item -LiteralPath $archive).Length -gt 536870912 -or (Get-MiyHash $archive) -ne $Sha256.ToLowerInvariant()) {
        Stop-MiyInstall 'The release ZIP failed SHA-256 verification. Installed files were preserved.'
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $allowed = @('miy.exe', 'release.json', 'install-cli.ps1', 'update-cli.ps1', 'uninstall-cli.ps1', 'windows-cli-common.ps1')
        $seen = @{}
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName -notin $allowed -or $seen.ContainsKey($entry.FullName) -or $entry.Length -gt 536870912) {
                Stop-MiyInstall 'The release ZIP contains an unexpected or duplicate path.'
            }
            $seen[$entry.FullName] = $true
            if ($entry.FullName -in @('miy.exe', 'release.json')) {
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $Stage $entry.FullName), $false)
            }
        }
        if (-not $seen.ContainsKey('miy.exe') -or -not $seen.ContainsKey('release.json')) { Stop-MiyInstall 'The release ZIP is missing its binary or provenance.' }
    } finally { $zip.Dispose() }
    $metadata = Read-MiyJson (Join-Path $Stage 'release.json')
    if ($metadata.schemaVersion -ne 1 -or $metadata.target -ne 'x86_64-pc-windows-msvc' -or
        $metadata.version -notmatch '^\d+\.\d+\.\d+$' -or $metadata.commit -notmatch '^[a-f0-9]{40}$' -or
        (-not [string]::IsNullOrEmpty($Version) -and $metadata.version -ne $Version) -or
        $metadata.binarySha256 -ne (Get-MiyHash (Join-Path $Stage 'miy.exe'))) { Stop-MiyInstall 'The release provenance or binary hash is invalid.' }
    $binary = Join-Path $Stage 'miy.exe'
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($binary))
    try {
        if ($reader.ReadUInt16() -ne 0x5a4d) { Stop-MiyInstall 'The release is not a Windows executable.' }
        $reader.BaseStream.Position = 0x3c
        $offset = $reader.ReadUInt32()
        if ($offset -gt $reader.BaseStream.Length - 6) { Stop-MiyInstall 'The executable header is invalid.' }
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { Stop-MiyInstall 'The release is not a Windows x64 executable.' }
    } finally { $reader.Dispose() }
    $reported = & $binary '--version'
    $nativeExit = $LASTEXITCODE
    if ($nativeExit -ne 0 -or ($reported -join "`n").Trim() -ne ('miy ' + $metadata.version)) { Stop-MiyInstall 'The staged executable failed its version check.' }
    return $metadata
}

function Invoke-MiyInstall([string]$InstallDir, [string]$InstallerDirectory, [string]$ArchivePath,
    [string]$Sha256, [string]$Version, [string]$Repository = 'hurxxxx/miy-v2', [switch]$AddToPath, [switch]$RequireExisting) {
    $installation = Open-MiyInstallation $InstallDir $InstallerDirectory -Create
    $root = $installation.Root
    $stage = $null
    try {
        Repair-MiyInstallation $root
        Assert-MiyOwnedBinary $root
        if ($RequireExisting -and -not [IO.File]::Exists((Join-Path $root '.miy-install.json'))) { Stop-MiyInstall 'No owned CLI is installed; run install-cli.ps1 first.' 5 }
        $id = [Guid]::NewGuid().ToString('N')
        $paths = Get-MiyJournalPaths $root $id
        $stage = $paths.Stage
        $null = [IO.Directory]::CreateDirectory($stage)
        $metadata = Get-MiyReleaseArchive $stage $ArchivePath $Sha256 $Version $Repository
        $state = @{ schemaVersion = 1; version = $metadata.version; target = $metadata.target; commit = $metadata.commit; binarySha256 = $metadata.binarySha256 }
        $markerBytes = ConvertTo-MiyBytes $state
        Assert-MiyOwnedBinary $root
        $journal = Start-MiyFileUpdate $root $id $metadata.binarySha256 (Get-MiyBytesHash $markerBytes)
        $journal.phase = 'applying'
        Write-MiyAtomic (Join-Path $root '.miy-install-journal.json') (ConvertTo-MiyBytes $journal)
        Write-MiyAtomic (Join-Path $root 'miy.exe') ([IO.File]::ReadAllBytes((Join-Path $stage 'miy.exe')))
        Write-MiyAtomic (Join-Path $root '.miy-install.json') $markerBytes
        $journal.phase = 'committed'
        Write-MiyAtomic (Join-Path $root '.miy-install-journal.json') (ConvertTo-MiyBytes $journal)
        Repair-MiyInstallation $root
        if ($AddToPath) { Set-MiyUserPath $root }
        return @{ installed = $true; version = $metadata.version; directory = $root; userPathChanged = [bool]$AddToPath; backup = $paths.Backup }
    } catch {
        $original = $_
        try { Repair-MiyInstallation $root } catch { Stop-MiyInstall 'Installation recovery is pending. Preserve the journal and backups, close CLI/MCP processes, and retry the external installer.' 6 }
        throw $original
    } finally {
        if ($null -ne $stage -and [IO.Directory]::Exists($stage) -and -not [IO.File]::Exists((Join-Path $root '.miy-install-journal.json'))) {
            Remove-MiyStage $stage
        }
        $installation.Lock.Dispose()
    }
}

function Invoke-MiyUninstall([string]$InstallDir, [string]$InstallerDirectory, [switch]$RemoveFromPath) {
    $installation = Open-MiyInstallation $InstallDir $InstallerDirectory
    if ($null -eq $installation) { return @{ removed = $false; reason = 'not_installed' } }
    $root = $installation.Root
    try {
        Repair-MiyInstallation $root
        Assert-MiyOwnedBinary $root
        $id = [Guid]::NewGuid().ToString('N')
        $journal = Start-MiyFileUpdate $root $id $null $null
        $journal.phase = 'applying'
        Write-MiyAtomic (Join-Path $root '.miy-install-journal.json') (ConvertTo-MiyBytes $journal)
        foreach ($name in @('miy.exe', '.miy-install.json')) {
            $path = Join-Path $root $name
            if ([IO.File]::Exists($path)) { [IO.File]::Delete($path) }
        }
        $journal.phase = 'committed'
        Write-MiyAtomic (Join-Path $root '.miy-install-journal.json') (ConvertTo-MiyBytes $journal)
        Repair-MiyInstallation $root
        if ($RemoveFromPath) { Set-MiyUserPath $root -Remove }
        return @{ removed = $true; directory = $root; userPathChanged = [bool]$RemoveFromPath; preserved = @('projects', 'configuration', 'credentials', 'application data', 'backups', 'unmanaged files') }
    } catch {
        $original = $_
        try { Repair-MiyInstallation $root } catch { Stop-MiyInstall 'Uninstall recovery is pending. Preserve the journal and backups and retry after closing CLI/MCP processes.' 6 }
        throw $original
    } finally { $installation.Lock.Dispose() }
}
