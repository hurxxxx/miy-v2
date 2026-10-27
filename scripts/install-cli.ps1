[CmdletBinding(DefaultParameterSetName = 'Remote')]
param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Remote')][string]$Version,
    [Parameter(Mandatory = $true, ParameterSetName = 'Local')][string]$ArchivePath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Local')][string]$Sha256,
    [string]$InstallDir = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\miy\bin'),
    [string]$Repository = 'hurxxxx/miy-v2',
    [ValidateNotNullOrEmpty()][string]$Server,
    [switch]$NonInteractive,
    [switch]$AddToPath
)
. (Join-Path $PSScriptRoot 'windows-cli-common.ps1')
try {
    Invoke-MiyInstall -InstallDir $InstallDir -InstallerDirectory $PSScriptRoot -ArchivePath $ArchivePath -Sha256 $Sha256 -Version $Version -Repository $Repository -Server $Server -NonInteractive:$NonInteractive -AddToPath:$AddToPath | ConvertTo-Json -Depth 8 -Compress
    exit 0
} catch {
    $failure = Get-MiyInstallFailure $_
    [Console]::Error.WriteLine($failure.Message)
    exit $failure.Code
}
