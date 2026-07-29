[CmdletBinding()]
param(
    [ValidateNotNullOrEmpty()]
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\VS Recent"),

    [ValidateNotNullOrEmpty()]
    [string]$Version = "latest",

    [string]$Hotkey = "",

    [switch]$NoShortcut,

    [switch]$Launch
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "VS Recent only supports Windows."
}

$architecture = if (-not [string]::IsNullOrWhiteSpace($env:PROCESSOR_ARCHITEW6432)) {
    $env:PROCESSOR_ARCHITEW6432
}
else {
    $env:PROCESSOR_ARCHITECTURE
}

$rid = switch ($architecture.ToUpperInvariant()) {
    "AMD64" { "win-x64" }
    "ARM64" { "win-arm64" }
    default { throw "Unsupported Windows architecture: $architecture" }
}

$repository = "calebrob6/vsrecent"
$headers = @{
    Accept = "application/vnd.github+json"
    "User-Agent" = "vsrecent-installer"
    "X-GitHub-Api-Version" = "2022-11-28"
}

if ([string]::Equals($Version, "latest", [StringComparison]::OrdinalIgnoreCase)) {
    $releaseUri = "https://api.github.com/repos/$repository/releases/latest"
}
else {
    $tag = if ($Version.StartsWith("v", [StringComparison]::OrdinalIgnoreCase)) {
        $Version
    }
    else {
        "v$Version"
    }
    $encodedTag = [Uri]::EscapeDataString($tag)
    $releaseUri = "https://api.github.com/repos/$repository/releases/tags/$encodedTag"
}

Write-Host "Resolving VS Recent $Version for $rid..."
$release = Invoke-RestMethod -Uri $releaseUri -Headers $headers
$assetName = "vsrecent-$rid.exe"
$asset = @($release.assets) |
    Where-Object { $_.name -eq $assetName } |
    Select-Object -First 1

if ($null -eq $asset) {
    $available = @($release.assets | ForEach-Object { $_.name }) -join ", "
    throw "Release $($release.tag_name) does not contain $assetName. Available assets: $available"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$exePath = Join-Path $InstallDir "vsrecent.exe"
$downloadPath = "$exePath.download-$PID"

Write-Host "Downloading $assetName..."
$previousProgressPreference = $ProgressPreference
$ProgressPreference = "SilentlyContinue"
try {
    Invoke-WebRequest -Uri $asset.browser_download_url -Headers $headers `
        -OutFile $downloadPath -UseBasicParsing

    if ((Get-Item $downloadPath).Length -eq 0) {
        throw "The downloaded executable is empty."
    }

    Move-Item -Path $downloadPath -Destination $exePath -Force
}
finally {
    $ProgressPreference = $previousProgressPreference
    if (Test-Path $downloadPath) {
        Remove-Item -Path $downloadPath -Force
    }
}

$shortcutPath = $null
if (-not $NoShortcut) {
    $startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
    $shortcutPath = Join-Path $startMenuDir "VS Recent.lnk"

    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    try {
        $shortcut.TargetPath = $exePath
        $shortcut.WorkingDirectory = $InstallDir
        $shortcut.IconLocation = "$exePath,0"
        if (-not [string]::IsNullOrWhiteSpace($Hotkey)) {
            $shortcut.Hotkey = $Hotkey
        }
        $shortcut.WindowStyle = 1
        $shortcut.Description = "Open recent VS Code folders"
        $shortcut.Save()
    }
    finally {
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($shortcut)
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($shell)
    }
}

Write-Host ""
Write-Host "Installed VS Recent $($release.tag_name):"
Write-Host "  Executable: $exePath"
if ($null -ne $shortcutPath) {
    Write-Host "  Shortcut:   $shortcutPath"
}

if ($Launch) {
    Start-Process -FilePath $exePath
}
