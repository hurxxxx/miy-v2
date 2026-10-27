[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\miy\bin'),
    [switch]$RemoveFromPath
)
. (Join-Path $PSScriptRoot 'windows-cli-common.ps1')
try {
    Invoke-MiyUninstall -InstallDir $InstallDir -InstallerDirectory $PSScriptRoot -RemoveFromPath:$RemoveFromPath | ConvertTo-Json -Depth 5 -Compress
    exit 0
} catch {
    $failure = Get-MiyInstallFailure $_
    [Console]::Error.WriteLine($failure.Message)
    exit $failure.Code
}
