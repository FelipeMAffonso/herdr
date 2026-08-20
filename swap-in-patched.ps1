# Swap the patched herdr build (felipe/smart-paste fork) in place of the installed
# release, with a backup so the release channel is one copy away.
#
# THIS RESTARTS THE HERDR SERVER: every pane closes. herdr-resurrect brings the
# herd back on next launch (save a snapshot first if you care about exact state).
# Run it at a moment you choose:  powershell -File C:\work\herdr-src\swap-in-patched.ps1
$installed = "$env:LOCALAPPDATA\Programs\Herdr\bin\herdr.exe"
$patched = "C:\work\herdr-src\target\release\herdr.exe"
$backup = "$installed.release-backup"

if (-not (Test-Path $patched)) { Write-Host "No patched build at $patched - build first."; exit 1 }
Write-Host "Stopping the herdr server (panes will close; resurrect restores them)..."
herdr server stop 2>$null
Start-Sleep -Seconds 2
if (-not (Test-Path $backup)) { Copy-Item $installed $backup }
Copy-Item $patched $installed -Force
Write-Host "Patched build installed. Backup at: $backup"
Write-Host "Relaunch Herdr from your shortcut - right-click any pane: Paste is in the menu."
Write-Host "To go back to the release build: copy the backup over herdr.exe the same way."
