# Served by Core; all downloads are bound to the configured platform origin.
& {
    $ErrorActionPreference = 'Stop'
    $Server = @MIY_SERVER@
    $Url = @MIY_URL@
    $ExpectedSha256 = @MIY_SHA256@
    $BinarySha256 = @MIY_BINARY_SHA256@
    $Version = @MIY_VERSION@
    $Architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    if ($env:OS -ne 'Windows_NT' -or $Architecture -ne 'AMD64') { throw 'This installer requires Windows x64.' }
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $Temporary = Join-Path ([IO.Path]::GetTempPath()) ('miy-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $Temporary | Out-Null
    try {
        $Archive = Join-Path $Temporary 'cli.zip'
        Write-Host "Downloading miy $Version for Windows x64..."
        Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Archive -TimeoutSec 600
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $Archive).Hash.ToLowerInvariant() -ne $ExpectedSha256) { throw 'CLI archive checksum mismatch; nothing installed.' }
        $Extract = Join-Path $Temporary 'package'
        Expand-Archive -LiteralPath $Archive -DestinationPath $Extract
        $Binary = Join-Path $Extract 'miy.exe'
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $Binary).Hash.ToLowerInvariant() -ne $BinarySha256) { throw 'CLI binary checksum mismatch; nothing installed.' }
        $ActualVersion = & $Binary --version
        if ($LASTEXITCODE -ne 0 -or $ActualVersion -ne "miy $Version") { throw 'CLI version mismatch or incompatible system.' }
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Extract 'install-cli.ps1') -ArchivePath $Archive -Sha256 $ExpectedSha256 -Server $Server -NonInteractive -AddToPath
        if ($LASTEXITCODE -ne 0) { throw 'CLI installation or server registration failed.' }
        $InstallDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\miy\bin'
        if (($env:PATH -split ';') -notcontains $InstallDir) { $env:PATH = "$InstallDir;$env:PATH" }
        Write-Host 'Installed. Next: miy auth login'
    } finally {
        Remove-Item -LiteralPath $Temporary -Recurse -Force
    }
}
