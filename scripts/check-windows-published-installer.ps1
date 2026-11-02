# Public download verification and read-only before/after evidence. Never installs or executes an EXE.
[CmdletBinding()]
param(
    [ValidateSet('Prepare', 'BeforeInstall', 'AfterInstall', 'BeforeUpdate', 'AfterUpdate', 'BeforeRemove', 'AfterRemove')]
    [string]$Phase = 'Prepare',
    [ValidatePattern('^run-[0-9]{8}T[0-9]{6}Z-[a-f0-9]{32}$')][string]$RunId,
    [ValidatePattern('^step-[0-9]{8}T[0-9]{6}Z-[a-f0-9]{32}$')][string]$BaselineStep,
    [string[]]$ProjectPath = @()
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$origin = 'https://v2.dwdcc.kr'
$expectedCommit = '8d7d04271f9467d8bbdd16308e73715b33e4dc0c'
$expectedVersion = '0.6.3'
$target = 'x86_64-pc-windows-msvc'
$utf8 = New-Object Text.UTF8Encoding($false, $true)
$stepDirectory = $null
$environment = $null
$step = $null
$exitCode = 1
$stage = 'environment'

function Require([bool]$Condition) {
    if (-not $Condition) { throw 'Acceptance check failed.' }
}
function Safe-Path([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    Require ($full -match '^[A-Za-z]:\\')
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            Require (((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    return $full
}
function Private-Directory([string]$Path) {
    $path = Safe-Path $Path
    Require (-not (Test-Path -LiteralPath $path))
    # No data is written until the newly-created empty directory has a protected DACL.
    $null = [IO.Directory]::CreateDirectory($path)
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl.SetOwner($identity)
    $acl.SetAccessRuleProtection($true, $false)
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', 'ContainerInherit, ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
    Set-Acl -LiteralPath $path -AclObject $acl
    Assert-Private $path
    return $path
}
function Assert-Private([string]$Path) {
    $null = Safe-Path $Path
    $acl = Get-Acl -LiteralPath $Path
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    Require ($acl.AreAccessRulesProtected -and $acl.GetOwner([Security.Principal.SecurityIdentifier]).Equals($sid))
    $allowed = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]) | Where-Object { $_.AccessControlType -eq 'Allow' })
    Require ($allowed.Count -gt 0)
    foreach ($rule in $allowed) { Require ($rule.IdentityReference.Equals($sid)) }
}
function Write-Json([string]$Path, $Value) {
    $bytes = $utf8.GetBytes(($Value | ConvertTo-Json -Depth 12) + "`n")
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}
function Read-Json([string]$Path) {
    $null = Safe-Path $Path
    Require ((Get-Item -LiteralPath $Path -Force).Length -le 4194304)
    return ([IO.File]::ReadAllText($Path, $utf8) | ConvertFrom-Json)
}
function Hash-File([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Download([string]$Relative, [string]$Destination, [long]$Limit) {
    Require ($Relative -match '^/(api/v1/cli/install|cli/(downloads|installers)/[a-f0-9]{40}/[A-Za-z0-9._-]+)$')
    $request = [Net.HttpWebRequest]::Create($origin + $Relative)
    $request.AllowAutoRedirect = $false
    $request.UseDefaultCredentials = $false
    $request.Timeout = 30000
    $request.ReadWriteTimeout = 30000
    $request.Headers['Accept-Encoding'] = 'identity'
    $response = $null
    $inputStream = $null
    $outputStream = $null
    try {
        $response = $request.GetResponse()
        Require ([int]$response.StatusCode -eq 200 -and $response.ContentLength -le $Limit)
        $inputStream = $response.GetResponseStream()
        $outputStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $buffer = New-Object byte[] 65536
        [long]$total = 0
        while (($count = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $total += $count
            Require ($total -le $Limit)
            $outputStream.Write($buffer, 0, $count)
        }
        Require ($total -gt 0)
        $outputStream.Flush($true)
    } finally {
        if ($null -ne $outputStream) { $outputStream.Dispose() }
        if ($null -ne $inputStream) { $inputStream.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
    }
}
function Mac-Bytes([byte[]]$Bytes) {
    $mac = New-Object Security.Cryptography.HMACSHA256
    try { $mac.Key = $script:runKey; return [BitConverter]::ToString($mac.ComputeHash($Bytes)).Replace('-', '').ToLowerInvariant() }
    finally { $mac.Dispose() }
}
function Mac-Text([string]$Text) { return Mac-Bytes ($utf8.GetBytes($Text)) }
function Mac-File([string]$Path) {
    Require ((Get-Item -LiteralPath $Path -Force).Length -le 536870912)
    $stream = [IO.File]::OpenRead($Path)
    $mac = New-Object Security.Cryptography.HMACSHA256
    try { $mac.Key = $script:runKey; return [BitConverter]::ToString($mac.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $mac.Dispose(); $stream.Dispose() }
}
function Tree-Snapshot([string]$Path, [bool]$Project) {
    $root = Safe-Path $Path
    $files = @{}
    $exists = Test-Path -LiteralPath $root
    if ($exists) {
        $pending = New-Object 'Collections.Generic.Stack[string]'
        $pending.Push($root)
        while ($pending.Count -gt 0) {
            $current = $pending.Pop()
            $item = Get-Item -LiteralPath $current -Force
            Require (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)
            if ($item.PSIsContainer) {
                foreach ($child in @(Get-ChildItem -LiteralPath $current -Force)) {
                    if ($Project -and $child.PSIsContainer -and $child.Name -in @('.git', 'node_modules', 'target', 'dist', '.local')) { continue }
                    $pending.Push($child.FullName)
                }
            } else {
                Require ($files.Count -lt 10000)
                $files[(Mac-Text $current.ToLowerInvariant())] = Mac-File $current
            }
        }
    }
    return @{ root = (Mac-Text $root.ToLowerInvariant()); exists = $exists; files = $files }
}
function Snapshot {
    $configRoot = if ($env:MIY_CONFIG_DIR) { $env:MIY_CONFIG_DIR } else { Join-Path $env:APPDATA 'miy' }
    $configRoot = Safe-Path $configRoot
    $contexts = @{}
    $configPath = Join-Path $configRoot 'config.json'
    $configHash = $null
    $client = $null
    if ([IO.File]::Exists($configPath)) {
        $config = Read-Json $configPath
        $configHash = Mac-File $configPath
        $client = Mac-Text ([string]$config.clientId)
        foreach ($property in $config.contexts.PSObject.Properties) {
            $context = $property.Value
            $canonical = [ordered]@{ origin = $context.origin; caFile = $context.caFile; userId = $context.userId }
            $contexts[(Mac-Text $property.Name)] = Mac-Text ($canonical | ConvertTo-Json -Compress)
        }
    }
    $projects = @{}
    foreach ($path in $ProjectPath) {
        $tree = Tree-Snapshot $path $true
        Require $tree.exists
        Require (-not $projects.ContainsKey($tree.root))
        $projects[$tree.root] = $tree
    }
    $installed = Join-Path $env:LOCALAPPDATA 'Programs\miy\bin\miy.exe'
    $null = Safe-Path $installed
    return @{ configRoot = (Mac-Text $configRoot.ToLowerInvariant()); configHash = $configHash; client = $client; contexts = $contexts;
        credentialFiles = (Tree-Snapshot (Join-Path $configRoot 'credentials') $false); projects = $projects;
        installed = [IO.File]::Exists($installed); installedSha256 = $(if ([IO.File]::Exists($installed)) { Hash-File $installed } else { $null });
        credentialVault = 'NOT_READ'; projectExclusions = @('.git', 'node_modules', 'target', 'dist', '.local') }
}
function Compare-Entries($Before, [hashtable]$After, [bool]$Exact) {
    $properties = @($Before.PSObject.Properties)
    if ($Exact -and $properties.Count -ne $After.Count) { return $false }
    foreach ($property in $properties) {
        if (-not $After.ContainsKey($property.Name) -or $After[$property.Name] -cne $property.Value) { return $false }
    }
    return $true
}

try {
    Require ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT -and [Environment]::Is64BitProcess)
    Require ($PSVersionTable.PSVersion.Major -ge 5 -and $env:PROCESSOR_ARCHITECTURE -eq 'AMD64')
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    Require (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))
    $os = Get-CimInstance Win32_OperatingSystem
    Require ($os.ProductType -eq 1 -and [int]$os.BuildNumber -ge 22000)
    $repository = Split-Path -Parent $PSScriptRoot
    $outputRoot = Safe-Path (Join-Path $repository '.local\windows-published-acceptance')
    $null = [IO.Directory]::CreateDirectory($outputRoot)
    $stage = 'private_evidence'
    if ($Phase -eq 'Prepare') {
        Require (-not $RunId -and -not $BaselineStep -and $ProjectPath.Count -eq 0)
        $RunId = 'run-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N')
        $runDirectory = Private-Directory (Join-Path $outputRoot $RunId)
        $key = New-Object byte[] 32
        $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
        try { $rng.GetBytes($key) } finally { $rng.Dispose() }
        [IO.File]::WriteAllBytes((Join-Path $runDirectory 'comparison.key'), $key)
    } else {
        Require (-not [string]::IsNullOrEmpty($RunId))
        $runDirectory = Safe-Path (Join-Path $outputRoot $RunId)
        Assert-Private $runDirectory
    }
    $step = 'step-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N')
    $stepDirectory = Private-Directory (Join-Path $runDirectory $step)
    $environment = @{ osVersion = [string]$os.Version; osBuild = [string]$os.BuildNumber; productType = [int]$os.ProductType;
        architecture = 'x64'; powershell = $PSVersionTable.PSVersion.ToString(); elevated = $false; windows11 = $true }
    $details = @{}
    if ($Phase -eq 'Prepare') {
        $stage = 'public_api'
        $downloads = Private-Directory (Join-Path $runDirectory 'downloads')
        $apiPath = Join-Path $stepDirectory 'public-api.json'
        $oldTls = [Net.ServicePointManager]::SecurityProtocol
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Download '/api/v1/cli/install' $apiPath 1048576
            $guide = Read-Json $apiPath
            Require ($guide.schemaVersion -eq 1 -and $guide.serverUrl -ceq $origin -and $guide.cliVersion -ceq $expectedVersion -and $guide.sourceCommit -ceq $expectedCommit)
            $rows = @($guide.downloads | Where-Object { $_.target -ceq $target })
            Require ($rows.Count -eq 1)
            $cli = $rows[0]
            $installer = $guide.windowsInstaller
            Require ($cli.schemaVersion -eq 1 -and $cli.version -ceq $expectedVersion -and $cli.commit -ceq $expectedCommit)
            Require ($cli.file -ceq "miy-$expectedVersion-$target.zip" -and $cli.sha256 -cmatch '^[a-f0-9]{64}$' -and $cli.binarySha256 -cmatch '^[a-f0-9]{64}$')
            Require ($installer.schemaVersion -eq 1 -and $installer.version -ceq $expectedVersion -and $installer.platform -ceq 'windows-x64')
            Require ($installer.cliCommit -ceq $expectedCommit -and $installer.installerCommit -ceq $expectedCommit)
            Require ($installer.file -ceq "MiySetup-$expectedVersion-windows-x64.exe" -and $installer.sha256 -cmatch '^[a-f0-9]{64}$')
            Require ($installer.archiveSha256 -ceq $cli.sha256 -and $installer.cliBinarySha256 -ceq $cli.binarySha256)
            $stage = 'downloads_and_checksums'
            foreach ($pair in @(@{ row = $cli; kind = 'downloads' }, @{ row = $installer; kind = 'installers' })) {
                $row = $pair.row
                $relative = '/cli/' + $pair.kind + '/' + $expectedCommit + '/' + $row.file
                Require ($row.url -ceq $relative -and $row.checksumUrl -ceq ($relative + '.sha256'))
                $destination = Join-Path $downloads $row.file
                Download ($relative + '.sha256') ($destination + '.sha256') 1024
                Require ([IO.File]::ReadAllText($destination + '.sha256').Trim() -ceq ($row.sha256 + '  ' + $row.file))
                Download $relative $destination 134217728
                Require ((Hash-File $destination) -ceq $row.sha256)
            }
        } finally { [Net.ServicePointManager]::SecurityProtocol = $oldTls }
        $stage = 'archive_provenance'
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archivePath = Join-Path $downloads $cli.file
        $archive = [IO.Compression.ZipFile]::OpenRead($archivePath)
        try {
            $names = @('miy.exe', 'release.json', 'install-cli.ps1', 'update-cli.ps1', 'uninstall-cli.ps1', 'windows-cli-common.ps1')
            Require ($archive.Entries.Count -eq $names.Count)
            $seen = @{}
            [long]$expanded = 0
            foreach ($entry in $archive.Entries) {
                Require ($entry.FullName -cin $names -and -not $seen.ContainsKey($entry.FullName) -and $entry.Length -gt 0 -and $entry.Length -le 536870912)
                $seen[$entry.FullName] = $true
                $expanded += $entry.Length
            }
            Require ($expanded -le 541065216)
            $metadata = $archive.GetEntry('release.json')
            Require ($metadata.Length -le 65536)
            $reader = New-Object IO.StreamReader($metadata.Open(), $utf8)
            try { $release = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
            Require ($release.schemaVersion -eq 1 -and $release.version -ceq $expectedVersion -and $release.commit -ceq $expectedCommit -and $release.target -ceq $target -and $release.binarySha256 -ceq $cli.binarySha256)
            $stream = $archive.GetEntry('miy.exe').Open()
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $binaryHash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
            finally { $sha.Dispose(); $stream.Dispose() }
            Require ($binaryHash -ceq $cli.binarySha256)
        } finally { $archive.Dispose() }
        $stage = 'installer_provenance'
        # Inspect bytes only. Do not load or execute the downloaded assembly.
        Add-Type -TypeDefinition @'
public static class MiyAcceptanceBytes {
    public static int Count(byte[] haystack, byte[] needle) {
        int count = 0;
        for (int i = 0; i <= haystack.Length - needle.Length; i++) {
            if (haystack[i] != needle[0]) continue;
            int j = 1; while (j < needle.Length && haystack[i+j] == needle[j]) j++;
            if (j == needle.Length) count++;
        }
        return count;
    }
}
'@
        $exePath = Join-Path $downloads $installer.file
        $exe = [IO.File]::ReadAllBytes($exePath)
        Require ($exe.Length -eq $installer.bytes -and $exe.Length -ge 256 -and $exe[0] -eq 77 -and $exe[1] -eq 90)
        $pe = [BitConverter]::ToInt32($exe, 60)
        Require ($pe -ge 64 -and $pe -le $exe.Length - 94)
        Require ([BitConverter]::ToUInt32($exe, $pe) -eq 17744 -and [BitConverter]::ToUInt16($exe, $pe + 4) -eq 34404)
        Require ([BitConverter]::ToUInt16($exe, $pe + 24) -eq 523 -and [BitConverter]::ToUInt16($exe, $pe + 92) -eq 2)
        $embedded = [ordered]@{ schemaVersion = 1; version = $expectedVersion; target = 'windows-x64'; installerCommit = $expectedCommit;
            cliCommit = $expectedCommit; archiveSha256 = $cli.sha256; cliBinarySha256 = $cli.binarySha256 }
        Require ([MiyAcceptanceBytes]::Count($exe, $utf8.GetBytes(($embedded | ConvertTo-Json -Compress))) -eq 1)
        Require ([MiyAcceptanceBytes]::Count($exe, [IO.File]::ReadAllBytes($archivePath)) -eq 1)
        $candidate = @{ schemaVersion = 1; version = $expectedVersion; expectedBundleVersion = '0.7.1'; bundleVersionVerification = 'NOT_RUN'; cliCommit = $expectedCommit; installerCommit = $expectedCommit;
            archiveSha256 = $cli.sha256; binarySha256 = $cli.binarySha256; installerSha256 = $installer.sha256; installerBytes = $installer.bytes;
            cliFile = $cli.file; installerFile = $installer.file; origin = $origin }
        Write-Json (Join-Path $runDirectory 'candidate.json') $candidate
        $details = @{ publishedDownloads = 'PASS'; embeddedZipAndProvenance = 'PASS'; authenticode = [string](Get-AuthenticodeSignature -LiteralPath $exePath).Status; guiAcceptance = 'NOT_RUN' }
    } else {
        $candidate = Read-Json (Join-Path $runDirectory 'candidate.json')
        Require ($candidate.cliCommit -ceq $expectedCommit -and $candidate.version -ceq $expectedVersion)
        $script:runKey = [IO.File]::ReadAllBytes((Safe-Path (Join-Path $runDirectory 'comparison.key')))
        Require ($script:runKey.Length -eq 32)
        $stage = 'local_snapshot'
        $snapshot = Snapshot
        if ($Phase -eq 'BeforeInstall') { Require (-not $snapshot.installed) }
        if ($Phase -eq 'BeforeUpdate') { Require ($snapshot.installed -and $snapshot.installedSha256 -cne $candidate.binarySha256) }
        if ($Phase -eq 'BeforeRemove') { Require $snapshot.installed }
        Write-Json (Join-Path $stepDirectory 'snapshot.json') $snapshot
        $details = @{ installed = $snapshot.installed; projectScopes = $snapshot.projects.Count; credentialVault = 'NOT_READ'; guiAcceptance = 'NOT_RUN' }
        if ($Phase.StartsWith('After')) {
            Require (-not [string]::IsNullOrEmpty($BaselineStep))
            $baselineDirectory = Safe-Path (Join-Path $runDirectory $BaselineStep)
            Assert-Private $baselineDirectory
            $previous = Read-Json (Join-Path $baselineDirectory 'result.json')
            Require ($previous.outcome -ceq 'PASS' -and $previous.phase -ceq $Phase.Replace('After', 'Before'))
            $before = Read-Json (Join-Path $baselineDirectory 'snapshot.json')
            $stage = 'preservation_comparison'
            Require ($before.configRoot -ceq $snapshot.configRoot -and ($null -eq $before.client -or $before.client -ceq $snapshot.client))
            Require (Compare-Entries $before.contexts $snapshot.contexts $false)
            Require ($before.credentialFiles.root -ceq $snapshot.credentialFiles.root)
            Require (Compare-Entries $before.credentialFiles.files $snapshot.credentialFiles.files $true)
            Require (@($before.projects.PSObject.Properties).Count -eq $snapshot.projects.Count)
            foreach ($project in $before.projects.PSObject.Properties) {
                Require ($snapshot.projects.ContainsKey($project.Name))
                Require (Compare-Entries $project.Value.files $snapshot.projects[$project.Name].files $true)
            }
            if ($Phase -eq 'AfterRemove') {
                Require (-not $snapshot.installed -and $before.configHash -ceq $snapshot.configHash)
            } else {
                Require ($snapshot.installed -and $snapshot.installedSha256 -ceq $candidate.binarySha256)
            }
            if ($Phase -eq 'AfterUpdate') { Require ($before.installed -and $before.installedSha256 -cne $candidate.binarySha256) }
            if ($Phase -eq 'AfterInstall') { Require (-not $before.installed) }
            if ($Phase -eq 'AfterRemove') { Require $before.installed }
            $details.existingContexts = @($before.contexts.PSObject.Properties).Count
            $details.existingCredentialFiles = @($before.credentialFiles.files.PSObject.Properties).Count
            $details.contextPreservation = $(if ($details.existingContexts -gt 0) { 'PASS' } else { 'NOT_RUN' })
            $details.credentialFilePreservation = $(if ($details.existingCredentialFiles -gt 0) { 'PASS' } else { 'NOT_RUN' })
            $details.projectPreservation = $(if ($snapshot.projects.Count -gt 0) { 'PASS' } else { 'NOT_RUN' })
            $details.credentialVaultUsability = 'USER_CHECK_REQUIRED'
            $details.configBytesUnchanged = ($before.configHash -ceq $snapshot.configHash)
            $details.baselineStep = $BaselineStep
        } else { Require (-not $BaselineStep) }
    }
    Write-Json (Join-Path $stepDirectory 'result.json') @{ schemaVersion = 1; phase = $Phase; outcome = 'PASS'; utc = [DateTime]::UtcNow.ToString('o');
        exitCode = 0; runId = $RunId; stepId = $step; environment = $environment; details = $details; expectedCommit = $expectedCommit;
        automation = 'download-and-read-only-checks'; userPathsRecorded = $false; secretsRecorded = $false }
    Write-Output ("PASS phase={0} run={1} step={2}; evidence under .local/windows-published-acceptance/" -f $Phase, $RunId, $step)
    $exitCode = 0
} catch {
    # Raw .NET/HTTP exceptions can contain a user profile path or URL. Never persist them.
    if ($null -ne $stepDirectory) {
        try { Write-Json (Join-Path $stepDirectory 'failure.json') @{ schemaVersion = 1; phase = $Phase; outcome = 'FAIL'; stage = $stage;
                utc = [DateTime]::UtcNow.ToString('o'); exitCode = 1; runId = $RunId; stepId = $step; environment = $environment;
                expectedCommit = $expectedCommit; guiAcceptance = 'NOT_RUN'; rawErrorRecorded = $false } } catch {}
    }
    [Console]::Error.WriteLine('FAIL stage=' + $stage + '; inspect the documented prerequisite or repeat into a fresh run. No installer was executed.')
}
exit $exitCode
