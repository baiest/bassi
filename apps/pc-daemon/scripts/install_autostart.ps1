# Registers nala-pc-daemon.exe to start automatically when the current
# user logs in, via a shortcut in the per-user Startup folder. Per-user,
# no admin rights needed — matches the plan's "tray application with
# per-user autostart, not a Windows service" decision (a service runs in
# session 0, isolated from the desktop, and can't run `start`/SendKeys or
# draw an overlay).
#
# Usage: powershell -ExecutionPolicy Bypass -File install_autostart.ps1 [path\to\pc-daemon.exe]

param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\..\..\target\release\pc-daemon.exe")
)

$ExePath = (Resolve-Path $ExePath).Path
$startupDir = [Environment]::GetFolderPath("Startup")
$shortcutPath = Join-Path $startupDir "Nala PC Daemon.lnk"

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $ExePath
$shortcut.WorkingDirectory = Split-Path $ExePath
$shortcut.Description = "Nala PC device daemon"
$shortcut.Save()

Write-Host "Installed autostart shortcut: $shortcutPath -> $ExePath"
Write-Host "To remove it: Remove-Item '$shortcutPath'"
