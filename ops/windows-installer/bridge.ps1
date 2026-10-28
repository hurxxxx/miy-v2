# Private implementation resource embedded in MiySetup. The archive's verified
# installation engine remains responsible for ownership, transactions and PATH.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$RequestPath)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
. (Join-Path $PSScriptRoot 'windows-cli-common.ps1')

function Get-Option($Object, [string]$Name, $Default = $null) {
    if ($null -eq $Object) { return $Default }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $Default }
    return $property.Value
}

function Quote-Native([string]$Value) {
    # CommandLineToArgvW quoting, including a final backslash before the quote.
    return '"' + [regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
}

function Invoke-Client([string]$Binary, [string[]]$Arguments) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Binary
    $start.Arguments = (@('--output', 'json', '--no-input') + $Arguments | ForEach-Object { Quote-Native $_ }) -join ' '
    $start.WorkingDirectory = $PSScriptRoot
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
    $start.EnvironmentVariables.Remove('MIY_CONTEXT')
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { Stop-MiyInstall 'The bundled CLI could not start.' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        # These are local config operations, never the installer transaction.
        if (-not $process.WaitForExit(30000)) {
            $process.Kill()
            Stop-MiyInstall 'The CLI configuration operation timed out. Close other CLI processes and retry.' 6
        }
        $body = $stdout.GetAwaiter().GetResult()
        $null = $stderr.GetAwaiter().GetResult()
        $code = $process.ExitCode
        if ($body.Length -gt 1048576) { Stop-MiyInstall 'The CLI returned an oversized configuration response.' }
        try { $reply = $body | ConvertFrom-Json -ErrorAction Stop }
        catch { Stop-MiyInstall 'The CLI configuration response could not be read.' }
        if ((Get-Option $reply 'schemaVersion') -ne 1 -or (Get-Option $reply 'ok') -ne ($code -eq 0)) {
            Stop-MiyInstall 'The CLI configuration response could not be verified.'
        }
        if ($code -ne 0) {
            $errorCode = Get-Option (Get-Option $reply 'error') 'code' 'unknown'
            if ($errorCode -notmatch '^[a-z0-9_]+$') { $errorCode = 'unknown' }
            Stop-MiyInstall ('CLI configuration failed (' + $errorCode + ', exit ' + $code + '). Existing settings were not reset.') $code
        }
        return $reply.data
    } finally { $process.Dispose() }
}

function Get-ConfigPath {
    if (-not [string]::IsNullOrEmpty($env:MIY_CONFIG_DIR)) { return Join-Path $env:MIY_CONFIG_DIR 'config.json' }
    return Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'miy\config.json'
}

function Get-Settings([string]$Binary) {
    $configPath = Get-ConfigPath
    $null = Assert-MiyPath $configPath
    $contexts = @{}
    $cursor = $null
    $active = $null
    do {
        $arguments = @('--limit', '200', 'context', 'list')
        if ($null -ne $cursor) { $arguments += @('--cursor', $cursor) }
        $page = Invoke-Client $Binary $arguments
        $active = $page.active
        foreach ($property in $page.contexts.PSObject.Properties) {
            # Only public context bindings leave this bridge. Never read the vault.
            $contexts[$property.Name] = @{ origin = $property.Value.origin }
        }
        if ($contexts.Count -gt 10000) { Stop-MiyInstall 'Too many server contexts to display. Use miy context list to manage them.' }
        $cursor = $page.nextCursor
    } while ($null -ne $cursor)
    $origin = $null
    if ($null -ne $active -and $contexts.ContainsKey($active)) { $origin = $contexts[$active].origin }
    return @{ exists = [IO.File]::Exists($configPath); configHash = (Get-MiyHash $configPath);
        active = $active; activeOrigin = $origin; contexts = $contexts }
}

function Get-Origin([string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) { Stop-MiyInstall 'Enter your company server URL. No default server is selected.' 2 }
    $value = $Value.Trim()
    $uri = $null
    if (-not [Uri]::TryCreate($value, [UriKind]::Absolute, [ref]$uri) -or
        $uri.Scheme -notin @('http', 'https') -or [string]::IsNullOrEmpty($uri.Host) -or
        -not [string]::IsNullOrEmpty($uri.UserInfo) -or $uri.AbsolutePath -ne '/' -or
        -not [string]::IsNullOrEmpty($uri.Query) -or -not [string]::IsNullOrEmpty($uri.Fragment) -or
        $value.Contains('\') -or $value -match '[\x00-\x20\x7f]') {
        Stop-MiyInstall 'Use a complete http:// or https:// server address, with an optional port and no credentials, path, query or fragment.' 2
    }
    return $uri.GetLeftPart([UriPartial]::Authority).TrimEnd('/')
}

function Test-Server([string]$Server) {
    $origin = Get-Origin $Server
    $response = $null
    $stream = $null
    $memory = $null
    try {
        # Windows certificate validation is never replaced or disabled.
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $request = [Net.HttpWebRequest]::Create($origin + '/api/v1/capabilities')
        $request.Method = 'GET'
        $request.AllowAutoRedirect = $false
        $request.Timeout = 8000
        $request.ReadWriteTimeout = 8000
        $request.Accept = 'application/json'
        $request.UserAgent = 'MiySetup/0.6.0'
        $response = $request.GetResponse()
        if ([int]$response.StatusCode -ne 200) { Stop-MiyInstall 'The server did not return its capabilities directly. Enter the final server address (redirects are not followed).' 4 }
        if ($response.ContentLength -gt 131072) { Stop-MiyInstall 'The server capabilities response is too large.' 4 }
        $stream = $response.GetResponseStream()
        $memory = New-Object IO.MemoryStream
        $buffer = New-Object byte[] 4096
        $deadline = [DateTime]::UtcNow.AddSeconds(8)
        while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            if ($memory.Length + $read -gt 131072 -or [DateTime]::UtcNow -gt $deadline) {
                Stop-MiyInstall 'The server capabilities response exceeded its size or time limit.' 4
            }
            $memory.Write($buffer, 0, $read)
        }
        $payload = [Text.UTF8Encoding]::new($false, $true).GetString($memory.ToArray()) | ConvertFrom-Json -ErrorAction Stop
        if ((Get-Option $payload 'schemaVersion') -ne 1 -or (Get-Option $payload 'cliContractVersion') -ne 1 -or
            'auth.device.v1' -notin (Get-Option $payload 'features' @()) -or
            'auth.sessions.v1' -notin (Get-Option $payload 'features' @())) {
            Stop-MiyInstall 'This address does not expose a compatible MIY server and browser login API.' 4
        }
        return @{ origin = $origin; compatible = $true; transport = ([Uri]$origin).Scheme }
    } catch {
        $known = Get-MiyInstallFailure $_
        if ($known.Known) { throw }
        Stop-MiyInstall 'Cannot verify the MIY server. Check its address, network, HTTP/HTTPS scheme and the Windows certificate trust. No settings were changed.' 4
    } finally {
        if ($null -ne $memory) { $memory.Dispose() }
        if ($null -ne $stream) { $stream.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
    }
}

function Get-Selection($Settings, [string]$Origin, $Request) {
    if ($Settings.activeOrigin -eq $Origin -and -not [string]::IsNullOrEmpty($Settings.active)) {
        return @{ name = $Settings.active; action = 'keep' }
    }
    if (-not $Settings.exists) { return @{ name = 'default'; action = 'initialize' } }
    if ((Get-Option $Request 'selectContext' $false) -ne $true) {
        Stop-MiyInstall 'Existing settings are preserved. Explicitly approve adding or selecting a context for this server.' 2
    }
    $name = [string](Get-Option $Request 'contextName' '')
    if ($name -notmatch '^[a-z][a-z0-9-]{0,47}$') { Stop-MiyInstall 'Enter a context name: 1-48 lowercase letters, digits or hyphens, starting with a letter.' 2 }
    if ($Settings.contexts.ContainsKey($name)) {
        if ($Settings.contexts[$name].origin -ne $Origin) {
            Stop-MiyInstall 'That context already points to a different server. Choose a new name; existing contexts are never overwritten.' 2
        }
        return @{ name = $name; action = 'select' }
    }
    return @{ name = $name; action = 'add' }
}

try {
    Assert-MiyWindows
    $request = Read-MiyJson $RequestPath
    $metadata = Read-MiyJson (Join-Path $PSScriptRoot 'installer.json')
    $bundledBinary = Join-Path $PSScriptRoot 'miy.exe'
    if ((Get-MiyHash $bundledBinary) -ne $metadata.cliBinarySha256 -or
        (Get-MiyHash (Join-Path $PSScriptRoot 'cli.zip')) -ne $metadata.archiveSha256) {
        Stop-MiyInstall 'The embedded CLI archive or executable failed its integrity check.'
    }
    $action = [string](Get-Option $request 'action' '')
    $result = switch ($action) {
        'inspect' { Get-Settings $bundledBinary; break }
        'probe' { Test-Server ([string](Get-Option $request 'server' '')); break }
        'install' {
            $probe = Test-Server ([string](Get-Option $request 'server' ''))
            $before = Get-Settings $bundledBinary
            if ($null -ne $request.PSObject.Properties['expectedConfigHash'] -and
                [string]$request.expectedConfigHash -ne [string]$before.configHash) {
                Stop-MiyInstall 'Server settings changed while this window was open. Reopen the installer before continuing.' 6
            }
            $selection = Get-Selection $before $probe.origin $request
            $directory = [string](Get-Option $request 'installDir' '')
            if ([string]::IsNullOrWhiteSpace($directory)) { Stop-MiyInstall 'Choose a per-user installation directory.' 2 }
            $installed = Invoke-MiyInstall -InstallDir $directory -InstallerDirectory $PSScriptRoot `
                -ArchivePath (Join-Path $PSScriptRoot 'cli.zip') -Sha256 $metadata.archiveSha256 `
                -Server $probe.origin -NonInteractive -AddToPath:([bool](Get-Option $request 'addToPath' $false))
            $binary = Join-Path $installed.directory 'miy.exe'
            if ($before.exists -and [string](Get-MiyHash (Get-ConfigPath)) -ne [string]$before.configHash) {
                Stop-MiyInstall 'The binary is installed, but server settings changed concurrently. Reopen the installer before selecting a context; existing settings were not reset.' 6
            }
            # The core command never overwrites an existing configuration. Context
            # changes happen only after the explicit selection above, through CLI.
            if ($selection.action -eq 'add') { $null = Invoke-Client $binary @('context', 'add', $selection.name, '--url', $probe.origin) }
            if ($selection.action -in @('add', 'select')) { $null = Invoke-Client $binary @('context', 'use', $selection.name) }
            $after = Get-Settings $binary
            if ($after.active -ne $selection.name -or $after.activeOrigin -ne $probe.origin) {
                Stop-MiyInstall 'The binary is installed, but server selection changed concurrently. Inspect miy context list; no settings were reset.' 6
            }
            @{ installed = $true; directory = $installed.directory; version = $installed.version;
                context = $selection.name; origin = $probe.origin; userPathChanged = $installed.userPathChanged }
            break
        }
        'uninstall' {
            $directory = [string](Get-Option $request 'installDir' '')
            if ([string]::IsNullOrWhiteSpace($directory)) { Stop-MiyInstall 'Choose the existing installation directory.' 2 }
            Invoke-MiyUninstall -InstallDir $directory -InstallerDirectory $PSScriptRoot -RemoveFromPath:([bool](Get-Option $request 'removeFromPath' $false))
            break
        }
        'login' {
            $directory = Assert-MiyPath ([string](Get-Option $request 'installDir' ''))
            $binary = Join-Path $directory 'miy.exe'
            if ((Get-MiyHash $binary) -ne $metadata.cliBinarySha256) { Stop-MiyInstall 'The installed CLI changed. Run its current installer before starting login.' 6 }
            $name = [string](Get-Option $request 'contextName' '')
            if ($name -notmatch '^[a-z][a-z0-9-]{0,47}$') { Stop-MiyInstall 'The selected context name is invalid.' 2 }
            $context = Invoke-Client $binary @('context', 'show', $name)
            if ($context.context.origin -ne (Get-Origin ([string](Get-Option $request 'server' '')))) { Stop-MiyInstall 'The selected server changed. Review miy context show before logging in.' 6 }
            $start = New-Object Diagnostics.ProcessStartInfo
            $start.FileName = $binary
            $start.Arguments = '--context ' + (Quote-Native $name) + ' auth login'
            $start.WorkingDirectory = $directory
            $start.UseShellExecute = $true
            $null = [Diagnostics.Process]::Start($start)
            @{ started = $true; context = $name }
            break
        }
        default { Stop-MiyInstall 'Unknown installer action.' 2 }
    }
    @{ ok = $true; data = $result } | ConvertTo-Json -Depth 12 -Compress
    exit 0
} catch {
    $failure = Get-MiyInstallFailure $_
    @{ ok = $false; error = @{ code = $failure.Code; message = $failure.Message } } | ConvertTo-Json -Depth 5 -Compress
    exit $failure.Code
}
