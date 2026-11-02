# Native Windows validation of the exact packaged executable and embedded bridge.
# All test configuration lives in an isolated private directory, never evidence.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{40}$')][string]$CliCommit,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{64}$')][string]$CliBinarySha256
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitProcess) { throw 'Run native installer validation on Windows x64.' }
$installer = (Resolve-Path -LiteralPath $InstallerPath).Path
$archive = (Resolve-Path -LiteralPath $ArchivePath).Path
$evidence = [IO.Path]::GetFullPath($OutputDirectory)
$null = New-Item -ItemType Directory -Path $evidence -Force
$root = Join-Path ([IO.Path]::GetTempPath()) ('MiySetup-test-' + [Guid]::NewGuid().ToString('N'))
$security = New-Object Security.AccessControl.DirectorySecurity
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().User
$security.SetOwner($identity)
$security.SetAccessRuleProtection($true, $false)
$security.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
$null = [IO.Directory]::CreateDirectory($root, $security)
$savedConfig = $env:MIY_CONFIG_DIR
$savedContext = $env:MIY_CONTEXT
$savedPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$cases = New-Object Collections.Generic.List[object]
$uiStates = [ordered]@{}
$serverA = $null
$serverB = $null
$failed = $false
$sequence = 0

function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Write-Json([string]$Path, $Value) { [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 16 -Compress), [Text.UTF8Encoding]::new($false)) }
function Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Quote([string]$Value) { return '"' + [regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"' }
function Run-Setup([string[]]$Arguments) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $installer
    $start.Arguments = ($Arguments | ForEach-Object { Quote $_ }) -join ' '
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try { $null = $process.Start(); $process.WaitForExit(); return $process.ExitCode }
    finally { $process.Dispose() }
}
function Invoke-Bridge($Request, [int]$ExpectedExit = 0) {
    $script:sequence += 1
    $inputPath = Join-Path $root ('request-' + $script:sequence + '.json')
    $resultPath = Join-Path $root ('result-' + $script:sequence + '.json')
    Write-Json $inputPath $Request
    $code = Run-Setup @('--bridge', $inputPath, $resultPath)
    Assert-True ($code -eq $ExpectedExit) ('Unexpected process exit code: expected ' + $ExpectedExit + ', actual ' + $code)
    Assert-True ([IO.File]::Exists($resultPath)) 'The packaged executable did not write its bridge response.'
    $reply = [IO.File]::ReadAllText($resultPath, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json
    Assert-True ($reply.ok -eq ($code -eq 0)) 'The response and actual process exit code disagree.'
    if ($code -ne 0) { Assert-True ($reply.error.code -eq $code) 'The public failure code does not match the process exit.' }
    return $reply
}
function Read-UiState([string]$Name, $Request) {
    $script:sequence += 1
    $inputPath = Join-Path $root ('ui-request-' + $script:sequence + '.json')
    $resultPath = Join-Path $root ('ui-result-' + $script:sequence + '.json')
    Write-Json $inputPath $Request
    Assert-True ((Run-Setup @('--render-state', $inputPath, $resultPath)) -eq 0) 'Native UI state inspection failed.'
    $reply = [IO.File]::ReadAllText($resultPath, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json
    Assert-True $reply.ok 'The native UI did not return its control state.'
    Assert-True ($reply.data.buttonCount -eq 3 -and $reply.data.textBoxCount -eq 1 -and $reply.data.checkBoxCount -eq 0) 'The simplified form contains unexpected login, context, folder or PATH controls.'
    # Only public fixture state is published, never request paths/configuration.
    $uiStates[$Name] = $reply.data
    return $reply.data
}
function Installation-Snapshot([string]$Directory) {
    if (-not [IO.Directory]::Exists($Directory)) { return 'absent' }
    $files = @(Get-ChildItem -LiteralPath $Directory -Force -Recurse -File | Sort-Object FullName | ForEach-Object {
        [ordered]@{ name = $_.FullName.Substring($Directory.Length); hash = (Hash $_.FullName); modified = $_.LastWriteTimeUtc.Ticks }
    })
    return ([ordered]@{ acl = (Get-Acl -LiteralPath $Directory).Sddl; files = $files } | ConvertTo-Json -Depth 6 -Compress)
}
function Case([string]$Name, [scriptblock]$Body) {
    try {
        $null = & $Body
        $cases.Add(@{ name = $Name; passed = $true })
        Write-Output ('PASS ' + $Name)
    } catch {
        $cases.Add(@{ name = $Name; passed = $false })
        # No requests, config, auth data, environment or filesystem paths in CI logs.
        throw ('Native installer case failed: ' + $Name + '. ' + $_.Exception.Message)
    }
}

try {
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
public sealed class MiyInstallerFixture : IDisposable {
    readonly TcpListener listener;
    readonly Thread thread;
    volatile bool stopped;
    public volatile string Mode = "good";
    public readonly int Port;
    public MiyInstallerFixture() {
        listener = new TcpListener(IPAddress.Loopback, 0); listener.Start();
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        thread = new Thread(Serve); thread.IsBackground = true; thread.Start();
    }
    void Serve() {
        while (!stopped) {
            try {
                using (TcpClient client = listener.AcceptTcpClient()) {
                    client.ReceiveTimeout = 2000; client.SendTimeout = 2000;
                    using (NetworkStream stream = client.GetStream()) {
                        StringBuilder header = new StringBuilder();
                        while (header.Length < 16384) { int b = stream.ReadByte(); if (b < 0) break; header.Append((char)b); if (header.ToString().EndsWith("\r\n\r\n")) break; }
                        string body = "{\"schemaVersion\":1,\"cliContractVersion\":1,\"features\":[\"auth.device.v1\",\"auth.sessions.v1\"]}";
                        string status = "200 OK"; string extra = "";
                        if (Mode == "wrong") body = "{\"schemaVersion\":99}";
                        if (Mode == "redirect") { status = "302 Found"; extra = "Location: https://foreign.invalid/\r\n"; body = ""; }
                        if (Mode == "large") body = new string('x', 140000);
                        byte[] payload = Encoding.UTF8.GetBytes(body);
                        byte[] response = Encoding.ASCII.GetBytes("HTTP/1.1 " + status + "\r\n" + extra + "Content-Type: application/json\r\nContent-Length: " + payload.Length + "\r\nConnection: close\r\n\r\n");
                        stream.Write(response, 0, response.Length); stream.Write(payload, 0, payload.Length);
                    }
                }
            } catch { if (stopped) return; }
        }
    }
    public void Dispose() { stopped = true; listener.Stop(); thread.Join(3000); }
}
'@
    $serverA = New-Object MiyInstallerFixture
    $serverB = New-Object MiyInstallerFixture
    $originA = 'http://127.0.0.1:' + $serverA.Port
    $originB = 'http://127.0.0.1:' + $serverB.Port
    $env:MIY_CONFIG_DIR = Join-Path $root 'settings'
    $env:MIY_CONTEXT = 'irrelevant-inherited-context'
    $config = Join-Path $env:MIY_CONFIG_DIR 'config.json'
    # Exercise an actual non-ASCII Windows username/install path, including spaces.
    $korean = [string][char]0xd55c + [string][char]0xae00
    $installDir = Join-Path $root ('company user ' + $korean + '\miy\bin')
    $binary = Join-Path $installDir 'miy.exe'

    Case 'embedded_archive_and_cli_provenance' {
        $destination = Join-Path $evidence 'embedded-provenance.json'
        Assert-True ((Run-Setup @('--verify', $destination)) -eq 0) 'Embedded verification failed.'
        $metadata = [IO.File]::ReadAllText($destination) | ConvertFrom-Json
        Assert-True ($metadata.archiveSha256 -eq (Hash $archive)) 'The packaged archive differs from the verified build input.'
        Assert-True ($metadata.cliCommit -ceq $CliCommit -and $metadata.version -ceq $Version) 'Unexpected CLI source commit/version.'
        Assert-True ($metadata.cliBinarySha256 -ceq $CliBinarySha256) 'Unexpected native binary hash.'
        Assert-True ($metadata.installerCommit -match '^[a-f0-9]{40}$') 'Missing installer source provenance.'
    }
    Case 'native_winforms_initial_state_is_simple_and_read_only' {
        $destination = Join-Path $evidence 'initial-form.png'
        $state = Read-UiState 'fresh' @{ installDir = $installDir; screenshot = $destination }
        Assert-True ($state.server -eq '' -and -not $state.installEnabled -and -not $state.removeEnabled -and -not $state.present -and -not $state.current) 'Fresh UI enables installation/removal or invents a server/installation.'
        Assert-True ($state.configurationReady -and $state.installationReady) 'Fresh configuration/installation inspection failed.'
        Assert-True ((Get-Item -LiteralPath $destination).Length -gt 5000) 'The rendered installer image is empty.'
        Assert-True (-not [IO.Directory]::Exists($installDir) -and -not [IO.File]::Exists($config)) 'Opening the installer created installation/configuration files.'
    }
    Case 'fresh_inspection_does_not_create_settings' {
        $reply = Invoke-Bridge @{ action = 'inspect' }
        Assert-True (-not $reply.data.exists -and $null -eq $reply.data.activeOrigin) 'Fresh installation unexpectedly has a server.'
        Assert-True (-not [IO.File]::Exists($config)) 'Inspection created configuration.'
        $reply = Invoke-Bridge @{ action = 'installation'; installDir = $installDir }
        Assert-True (-not $reply.data.present -and -not $reply.data.current -and -not $reply.data.recoveryRequired) 'Fresh installation state is incorrect.'
        Assert-True (-not [IO.Directory]::Exists($installDir)) 'Installation inspection created its target directory.'
    }
    Case 'missing_server_refuses_install_without_mutation' {
        $null = Invoke-Bridge @{ action = 'install'; installDir = $installDir } 2
        Assert-True (-not [IO.File]::Exists($binary) -and -not [IO.File]::Exists($config)) 'Missing-server install changed files.'
    }
    Case 'invalid_and_credential_bearing_origins_are_rejected' {
        foreach ($bad in @('example.invalid', 'file:///C:/miy', 'http://user:secret@127.0.0.1', ($originA + '/path'), ($originA + '?token=value'))) {
            $null = Invoke-Bridge @{ action = 'probe'; server = $bad } 2
        }
    }
    Case 'failed_probe_does_not_install_or_register' {
        $serverA.Mode = 'wrong'
        try { $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir } 4 }
        finally { $serverA.Mode = 'good' }
        Assert-True (-not [IO.File]::Exists($binary) -and -not [IO.File]::Exists($config)) 'Failed probe changed installation or settings.'
        $state = Read-UiState 'failedInstall' @{ installDir = $installDir; serverOverride = $originA }
        Assert-True ($state.installEnabled -and -not $state.removeEnabled -and -not $state.present) 'A failed install enables removal or prevents retry.'
        Assert-True (-not [IO.Directory]::Exists($installDir) -and -not [IO.File]::Exists($config)) 'Inspecting a failed install created installation/configuration files.'
    }
    Case 'redirects_and_oversized_capabilities_are_rejected' {
        try {
            foreach ($mode in @('redirect', 'large')) { $serverA.Mode = $mode; $null = Invoke-Bridge @{ action = 'probe'; server = $originA } 4 }
        } finally { $serverA.Mode = 'good' }
    }
    Case 'explicit_http_ip_and_port_are_supported' {
        $reply = Invoke-Bridge @{ action = 'probe'; server = $originA }
        Assert-True ($reply.data.compatible -and $reply.data.origin -eq $originA -and $reply.data.transport -eq 'http') 'Explicit HTTP IP server was rejected or changed.'
    }
    Case 'fresh_native_install_unicode_path_and_explicit_server' {
        $reply = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; addToPath = $true; autoSelectContext = $true; expectedConfigHash = '' }
        Assert-True ($reply.data.directory -ceq $installDir) 'Unicode installation path was corrupted in the bridge response.'
        Assert-True ($reply.data.context -eq 'default' -and $reply.data.origin -eq $originA) 'The explicit server was not selected.'
        Assert-True ((Hash $binary) -ceq $CliBinarySha256) 'Installed binary hash differs.'
        $installedVersion = & $binary --version
        Assert-True ($LASTEXITCODE -eq 0 -and $installedVersion -ceq ('miy ' + $Version)) 'Installed native CLI did not execute.'
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        Assert-True (($userPath -split ';') -contains $installDir) 'User PATH was not updated.'
    }
    $original = [IO.File]::ReadAllBytes($config)
    $originalHash = Hash $config
    $originalConfig = [IO.File]::ReadAllText($config) | ConvertFrom-Json
    $sentinel = Join-Path $env:MIY_CONFIG_DIR 'credential-preservation.sentinel'
    [IO.File]::WriteAllText($sentinel, 'Test-only credential sentinel; not a real credential.')
    $sentinelHash = Hash $sentinel
    Case 'installed_ui_disables_install_across_reopen_and_server_edit' {
        $before = Installation-Snapshot $installDir
        $reply = Invoke-Bridge @{ action = 'installation'; installDir = $installDir }
        Assert-True ($reply.data.present -and $reply.data.current -and -not $reply.data.recoveryRequired) 'Installed CLI is not recognized as current.'
        $state = Read-UiState 'installed' @{ installDir = $installDir; screenshot = (Join-Path $evidence 'completed-form.png') }
        Assert-True (-not $state.installEnabled -and $state.removeEnabled -and $state.current -and $state.server -eq $originA) 'Reopened installer does not reflect installed state.'
        $state = Read-UiState 'installedServerEdited' @{ installDir = $installDir; serverOverride = $originB }
        Assert-True (-not $state.installEnabled -and $state.removeEnabled) 'Editing the server enabled reinstall of the current CLI.'
        $state = Read-UiState 'busy' @{ installDir = $installDir; busy = $true }
        Assert-True ($state.busy -and -not $state.installEnabled -and -not $state.removeEnabled -and -not $state.serverEnabled -and -not $state.probeEnabled) 'Busy UI still allows a conflicting user action.'
        Assert-True ((Installation-Snapshot $installDir) -ceq $before) 'Inspection changed installation files, timestamps or ACLs.'
        Assert-True ((Hash $config) -eq $originalHash -and (Hash $sentinel) -eq $sentinelHash) 'Opening the form changed settings or credentials.'
        Assert-True ((Get-Item -LiteralPath (Join-Path $evidence 'completed-form.png')).Length -gt 5000) 'The completed installer screenshot is empty.'
    }
    Case 'different_owned_binary_allows_update' {
        $markerPath = Join-Path $installDir '.miy-install.json'
        $binaryBytes = [IO.File]::ReadAllBytes($binary)
        $markerBytes = [IO.File]::ReadAllBytes($markerPath)
        try {
            [IO.File]::WriteAllBytes($binary, [byte[]]($binaryBytes + [byte]0))
            $marker = [IO.File]::ReadAllText($markerPath) | ConvertFrom-Json
            $marker.binarySha256 = Hash $binary
            Write-Json $markerPath $marker
            $before = Installation-Snapshot $installDir
            $state = Read-UiState 'updateAvailable' @{ installDir = $installDir }
            Assert-True ($state.present -and -not $state.current -and $state.installEnabled -and $state.removeEnabled) 'An owned different binary cannot be updated or removed.'
            Assert-True ((Installation-Snapshot $installDir) -ceq $before) 'Update inspection mutated the existing installation.'
        } finally {
            [IO.File]::WriteAllBytes($binary, $binaryBytes)
            [IO.File]::WriteAllBytes($markerPath, $markerBytes)
        }
    }
    Case 'pending_recovery_is_detected_without_mutation' {
        $journalPath = Join-Path $installDir '.miy-install-journal.json'
        # Detection must leave even an unreadable journal for the transaction engine.
        [IO.File]::WriteAllText($journalPath, '{ interrupted fixture')
        try {
            $before = Installation-Snapshot $installDir
            $reply = Invoke-Bridge @{ action = 'installation'; installDir = $installDir }
            Assert-True ($reply.data.present -and -not $reply.data.current -and $reply.data.recoveryRequired) 'Pending transaction was reported as a completed installation.'
            $state = Read-UiState 'recoveryRequired' @{ installDir = $installDir }
            Assert-True ($state.recoveryRequired -and -not $state.current -and $state.installEnabled) 'Pending recovery cannot be retried through the UI.'
            Assert-True ((Installation-Snapshot $installDir) -ceq $before) 'Inspection repaired or changed a pending transaction.'
        } finally { [IO.File]::Delete($journalPath) }
    }
    Case 'rerun_preserves_existing_config_and_credentials_exactly' {
        $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; addToPath = $true; autoSelectContext = $true; expectedConfigHash = $originalHash }
        Assert-True ((Hash $config) -eq $originalHash -and (Hash $sentinel) -eq $sentinelHash) 'Existing config or credential sentinel changed.'
        Assert-True (@(([Environment]::GetEnvironmentVariable('Path', 'User') -split ';') | Where-Object { $_ -eq $installDir }).Count -eq 1) 'Rerunning the installer duplicated PATH.'
    }
    Case 'different_server_requires_explicit_context_consent' {
        $null = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir } 2
        Assert-True ((Hash $config) -eq $originalHash -and (Hash $sentinel) -eq $sentinelHash) 'A different server silently overwrote settings.'
    }
    Case 'existing_context_cannot_be_rebound' {
        $null = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir; selectContext = $true; contextName = 'default' } 2
        Assert-True ((Hash $config) -eq $originalHash) 'Existing context was rebound.'
    }
    Case 'explicit_context_add_and_select_preserves_other_bindings' {
        $reply = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir; selectContext = $true; contextName = 'company-b' }
        $current = [IO.File]::ReadAllText($config) | ConvertFrom-Json
        Assert-True ($reply.data.context -eq 'company-b' -and $current.active -eq 'company-b' -and $current.contexts.default.origin -eq $originA) 'Explicit selection lost the original server.'
        Assert-True ($current.clientId -eq $originalConfig.clientId -and (Hash $sentinel) -eq $sentinelHash) 'Existing client identity or credential sentinel changed.'
        $reply = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; selectContext = $true; contextName = 'default' }
        Assert-True ($reply.data.context -eq 'default') 'An existing matching context could not be selected.'
    }
    Case 'automatic_context_selection_reuses_matching_binding' {
        $reply = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir; autoSelectContext = $true; expectedConfigHash = (Hash $config) }
        $current = [IO.File]::ReadAllText($config) | ConvertFrom-Json
        Assert-True ($reply.data.context -eq 'company-b' -and $current.active -eq 'company-b') 'Automatic selection did not reuse an existing server binding.'
        Assert-True (@($current.contexts.PSObject.Properties).Count -eq 2 -and $current.contexts.default.origin -eq $originA) 'Automatic selection duplicated or changed existing contexts.'
        Assert-True ($current.clientId -eq $originalConfig.clientId -and (Hash $sentinel) -eq $sentinelHash) 'Automatic selection changed identity or credentials.'
        $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; autoSelectContext = $true }
    }
    Case 'automatic_context_names_avoid_collisions_and_preserve_bindings' {
        $current = [IO.File]::ReadAllText($config) | ConvertFrom-Json
        $current.contexts = [pscustomobject]@{
            default = $current.contexts.default
            work = [pscustomobject]@{ origin = 'https://preserved-one.invalid'; caFile = $null; userId = 'fixture-one' }
            'work-2' = [pscustomobject]@{ origin = 'https://preserved-two.invalid'; caFile = $null; userId = 'fixture-two' }
        }
        Write-Json $config $current
        $reply = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir; autoSelectContext = $true; expectedConfigHash = (Hash $config) }
        $current = [IO.File]::ReadAllText($config) | ConvertFrom-Json
        Assert-True ($reply.data.context -eq 'work-3' -and $current.active -eq 'work-3' -and $current.contexts.'work-3'.origin -eq $originB) 'Automatic selection did not allocate the next unused context name.'
        Assert-True ($current.contexts.work.origin -eq 'https://preserved-one.invalid' -and $current.contexts.work.userId -eq 'fixture-one' -and $current.contexts.'work-2'.origin -eq 'https://preserved-two.invalid' -and $current.contexts.'work-2'.userId -eq 'fixture-two') 'Automatic naming overwrote an existing server or authentication binding.'
        Assert-True ($current.contexts.default.origin -eq $originA -and $current.clientId -eq $originalConfig.clientId -and (Hash $sentinel) -eq $sentinelHash) 'Automatic naming changed original context, identity or credentials.'
        $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; autoSelectContext = $true }
    }
    Case 'changed_config_snapshot_is_rejected_before_install' {
        $before = Hash $config
        $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; expectedConfigHash = ('0' * 64) } 6
        Assert-True ((Hash $config) -eq $before) 'Stale GUI settings changed configuration.'
    }
    Case 'installer_has_no_browser_login_action' {
        $before = Hash $config
        $null = Invoke-Bridge @{ action = 'login'; server = $originA; installDir = $installDir; contextName = 'default' } 2
        Assert-True ((Hash $config) -eq $before -and (Hash $sentinel) -eq $sentinelHash) 'Removed login action changed settings or credentials.'
    }
    Case 'existing_empty_configuration_supports_automatic_context' {
        $emptyRoot = Join-Path $root 'empty-settings'
        $saved = $env:MIY_CONFIG_DIR
        try {
            $env:MIY_CONFIG_DIR = $emptyRoot
            $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir }
            $emptyPath = Join-Path $emptyRoot 'config.json'
            $empty = [IO.File]::ReadAllText($emptyPath) | ConvertFrom-Json
            $empty.active = $null; $empty.contexts = [pscustomobject]@{}
            Write-Json $emptyPath $empty
            $emptyHash = Hash $emptyPath
            $null = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir } 2
            Assert-True ((Hash $emptyPath) -eq $emptyHash) 'Empty existing configuration was overwritten.'
            $reply = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir; autoSelectContext = $true; expectedConfigHash = $emptyHash }
            $updated = [IO.File]::ReadAllText($emptyPath) | ConvertFrom-Json
            Assert-True ($reply.data.context -eq 'work' -and $updated.active -eq 'work' -and $updated.clientId -eq $empty.clientId) 'Empty configuration could not be populated automatically without changing client identity.'
        } finally { $env:MIY_CONFIG_DIR = $saved }
    }
    Case 'relative_config_override_survives_temporary_package_cleanup' {
        $saved = $env:MIY_CONFIG_DIR
        try {
            $env:MIY_CONFIG_DIR = 'relative-settings'
            $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir }
            $relativePath = Join-Path $root 'relative-settings\config.json'
            Assert-True ([IO.File]::Exists($relativePath)) 'Relative config was created inside disposable package extraction.'
            $reply = Invoke-Bridge @{ action = 'inspect' }
            Assert-True ($reply.data.activeOrigin -eq $originA) 'Relative config did not survive installer exit.'
        } finally { $env:MIY_CONFIG_DIR = $saved }
    }
    Case 'uninstall_preserves_even_broken_config_and_unmanaged_files' {
        [IO.File]::WriteAllText($config, '{ intentionally broken configuration')
        $brokenHash = Hash $config
        $unmanaged = Join-Path $installDir 'keep-user-file.txt'
        [IO.File]::WriteAllText($unmanaged, 'This is not owned by the installer.')
        $state = Read-UiState 'brokenConfiguration' @{ installDir = $installDir }
        Assert-True (-not $state.configurationReady -and $state.installationReady -and $state.removeEnabled -and -not $state.installEnabled) 'Invalid configuration prevents removal or enables installation.'
        $reply = Invoke-Bridge @{ action = 'uninstall'; installDir = $installDir; removeFromPath = $true }
        Assert-True ($reply.data.removed -and -not [IO.File]::Exists($binary)) 'Owned CLI was not removed.'
        Assert-True ((Hash $config) -eq $brokenHash -and (Hash $sentinel) -eq $sentinelHash -and [IO.File]::Exists($unmanaged)) 'Uninstall changed config, credentials or unmanaged files.'
        Assert-True (-not (([Environment]::GetEnvironmentVariable('Path', 'User') -split ';') -contains $installDir)) 'Uninstall left its user PATH entry.'
        [IO.File]::WriteAllBytes($config, $original)
    }
    Case 'removed_ui_disables_remove_and_allows_install_again' {
        $before = Installation-Snapshot $installDir
        $reply = Invoke-Bridge @{ action = 'installation'; installDir = $installDir }
        Assert-True (-not $reply.data.present -and -not $reply.data.current -and -not $reply.data.recoveryRequired) 'Removed CLI still reports installed.'
        $state = Read-UiState 'removed' @{ installDir = $installDir }
        Assert-True (-not $state.removeEnabled -and $state.installEnabled -and -not $state.present -and $state.server -eq $originA) 'Removed UI does not allow reinstallation with the preserved server.'
        Assert-True ((Installation-Snapshot $installDir) -ceq $before -and (Hash $config) -eq $originalHash) 'Opening the removed UI changed installation remnants or preserved settings.'
    }
} catch {
    $failed = $true
    Write-Warning ('Native Windows installer validation failed in case ' + $(if ($cases.Count -gt 0) { $cases[$cases.Count - 1].name } else { 'fixture_setup' }) + '. ' + $_.Exception.Message)
} finally {
    if ($null -ne $serverA) { $serverA.Dispose() }
    if ($null -ne $serverB) { $serverB.Dispose() }
    $env:MIY_CONFIG_DIR = $savedConfig
    $env:MIY_CONTEXT = $savedContext
    [Environment]::SetEnvironmentVariable('Path', $savedPath, 'User')
    try { Remove-Item -LiteralPath $root -Recurse -Force }
    catch { $failed = $true; $cases.Add(@{ name = 'isolated_fixture_cleanup'; passed = $false }) }
    Write-Json (Join-Path $evidence 'summary.json') ([ordered]@{ schemaVersion = 1; passed = (-not $failed); powershellVersion = $PSVersionTable.PSVersion.ToString(); cases = @($cases.ToArray()) })
    Write-Json (Join-Path $evidence 'ui-state.json') ([ordered]@{ schemaVersion = 1; passed = (-not $failed); states = $uiStates })
}
if ($failed) { throw 'Native installer validation failed; inspect the sanitized summary.json and initial-form.png artifacts.' }
Write-Output ('Native Windows installer validation passed: ' + $cases.Count + ' cases.')
