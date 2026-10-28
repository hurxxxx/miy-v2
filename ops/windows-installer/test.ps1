# Native Windows validation of the exact packaged executable and embedded bridge.
# All test configuration lives in an isolated private directory, never evidence.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
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
        Assert-True ($metadata.cliCommit -eq 'b67798bfb84f299257ba4326beaf034b125cde7a') 'Unexpected CLI source commit.'
        Assert-True ($metadata.cliBinarySha256 -eq '99f590c62b44e4b61391b8986d5fa6175ebee1d072efd066bbc85747e38eac71') 'Unexpected native binary hash.'
        Assert-True ($metadata.installerCommit -match '^[a-f0-9]{40}$') 'Missing installer source provenance.'
    }
    Case 'native_winforms_render_and_blank_initial_server' {
        $destination = Join-Path $evidence 'initial-form.png'
        Assert-True ((Run-Setup @('--render', $destination)) -eq 0) 'Native WinForms construction/render failed.'
        Assert-True ((Get-Item -LiteralPath $destination).Length -gt 5000) 'The rendered installer image is empty.'
    }
    Case 'fresh_inspection_does_not_create_settings' {
        $reply = Invoke-Bridge @{ action = 'inspect' }
        Assert-True (-not $reply.data.exists -and $null -eq $reply.data.activeOrigin) 'Fresh installation unexpectedly has a server.'
        Assert-True (-not [IO.File]::Exists($config)) 'Inspection created configuration.'
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
        $reply = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; addToPath = $true; expectedConfigHash = '' }
        Assert-True ($reply.data.directory -ceq $installDir) 'Unicode installation path was corrupted in the bridge response.'
        Assert-True ($reply.data.context -eq 'default' -and $reply.data.origin -eq $originA) 'The explicit server was not selected.'
        Assert-True ((Hash $binary) -eq '99f590c62b44e4b61391b8986d5fa6175ebee1d072efd066bbc85747e38eac71') 'Installed binary hash differs.'
        $version = & $binary --version
        Assert-True ($LASTEXITCODE -eq 0 -and $version -eq 'miy 0.6.0') 'Installed native CLI did not execute.'
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        Assert-True (($userPath -split ';') -contains $installDir) 'User PATH was not updated.'
    }
    $original = [IO.File]::ReadAllBytes($config)
    $originalHash = Hash $config
    $originalConfig = [IO.File]::ReadAllText($config) | ConvertFrom-Json
    $sentinel = Join-Path $env:MIY_CONFIG_DIR 'credential-preservation.sentinel'
    [IO.File]::WriteAllText($sentinel, 'Test-only credential sentinel; not a real credential.')
    $sentinelHash = Hash $sentinel
    Case 'rerun_preserves_existing_config_and_credentials_exactly' {
        $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; addToPath = $true; expectedConfigHash = $originalHash }
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
    Case 'changed_config_snapshot_is_rejected_before_install' {
        $before = Hash $config
        $null = Invoke-Bridge @{ action = 'install'; server = $originA; installDir = $installDir; expectedConfigHash = ('0' * 64) } 6
        Assert-True ((Hash $config) -eq $before) 'Stale GUI settings changed configuration.'
    }
    Case 'login_refuses_a_context_whose_server_changed' {
        $null = Invoke-Bridge @{ action = 'login'; server = $originB; installDir = $installDir; contextName = 'default' } 6
    }
    Case 'existing_empty_configuration_requires_explicit_add' {
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
            $reply = Invoke-Bridge @{ action = 'install'; server = $originB; installDir = $installDir; selectContext = $true; contextName = 'office' }
            Assert-True ($reply.data.context -eq 'office') 'Empty configuration could not be explicitly populated.'
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
        $reply = Invoke-Bridge @{ action = 'uninstall'; installDir = $installDir; removeFromPath = $true }
        Assert-True ($reply.data.removed -and -not [IO.File]::Exists($binary)) 'Owned CLI was not removed.'
        Assert-True ((Hash $config) -eq $brokenHash -and (Hash $sentinel) -eq $sentinelHash -and [IO.File]::Exists($unmanaged)) 'Uninstall changed config, credentials or unmanaged files.'
        Assert-True (-not (([Environment]::GetEnvironmentVariable('Path', 'User') -split ';') -contains $installDir)) 'Uninstall left its user PATH entry.'
        [IO.File]::WriteAllBytes($config, $original)
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
}
if ($failed) { throw 'Native installer validation failed; inspect the sanitized summary.json and initial-form.png artifacts.' }
Write-Output ('Native Windows installer validation passed: ' + $cases.Count + ' cases.')
