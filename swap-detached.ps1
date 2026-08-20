# Detached runner for swap-in-patched.ps1 (2026-08-20). Runs from a one-shot
# scheduled task so the herdr server restart cannot kill the swap mid-run (the
# swap script's own shell dies with the server when launched from a herdr pane).
Start-Transcript C:\work\herdr-swap-log.txt -Force
& powershell -NoProfile -ExecutionPolicy Bypass -File C:\work\herdr-src\swap-in-patched.ps1
$exe = "$env:LOCALAPPDATA\Programs\Herdr\bin\herdr.exe"
Write-Output "post-swap binary: $((Get-Item $exe).Length) bytes, $((Get-Item $exe).LastWriteTime)"
Write-Output "expected patched size: $((Get-Item C:\work\herdr-src\target\release\herdr.exe).Length) bytes"
Stop-Transcript
