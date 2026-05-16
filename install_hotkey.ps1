param(
    [string]$Hotkey = "CTRL+ALT+R",
    [string]$ExePath = "C:\Users\davrob\apps\vsrecent\vsrecent.exe",
    [string]$ShortcutName = "VS Recent"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $ExePath)) {
    Write-Error "EXE not found: $ExePath"
    exit 1
}

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
if (-not (Test-Path $startMenuDir)) {
    New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
}

$lnkPath = Join-Path $startMenuDir ($ShortcutName + ".lnk")
$ws  = New-Object -ComObject WScript.Shell
$lnk = $ws.CreateShortcut($lnkPath)
$lnk.TargetPath       = $ExePath
$lnk.WorkingDirectory = Split-Path $ExePath -Parent
$lnk.IconLocation     = "$ExePath,0"
$lnk.Hotkey           = $Hotkey
$lnk.WindowStyle      = 1
$lnk.Description      = "Quick launcher for VSCode recent projects"
$lnk.Save()

Write-Host "Shortcut created:"
Write-Host "  Path:   $lnkPath"
Write-Host "  Target: $ExePath"
Write-Host "  Hotkey: $Hotkey"
Write-Host ""
Write-Host "Press $Hotkey from anywhere on the desktop to launch VS Recent."
