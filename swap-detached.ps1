# Detached runner for swap-in-patched.ps1 (2026-08-20). Runs from a one-shot
# scheduled task so the herdr server restart cannot kill the swap mid-run (the
# swap script's own shell dies with the server when launched from a herdr pane).
Start-Transcript C:\work\herdr-swap-log.txt -Force
# Sign the staged build with the locally-trusted cert (2026-08-21): unsigned fork
# builds made Windows Security toast "unrecognized app" on every launch.
$signCert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert |
  Where-Object Subject -eq "CN=Felipe Herdr Local Signing" | Select-Object -First 1
if ($signCert) {
  $sig = Set-AuthenticodeSignature -FilePath C:\work\herdr-src\target\release\herdr.exe -Certificate $signCert -HashAlgorithm SHA256
  Write-Output "staged build signature: $($sig.Status)"
}
& powershell -NoProfile -ExecutionPolicy Bypass -File C:\work\herdr-src\swap-in-patched.ps1
$exe = "$env:LOCALAPPDATA\Programs\Herdr\bin\herdr.exe"
Write-Output "post-swap binary: $((Get-Item $exe).Length) bytes, $((Get-Item $exe).LastWriteTime)"
Write-Output "expected patched size: $((Get-Item C:\work\herdr-src\target\release\herdr.exe).Length) bytes"
# The new binary is in place - now the config rows that use its new tokens.
& powershell -NoProfile -ExecutionPolicy Bypass -File C:\work\herdr-src\update-config-for-swap.ps1
Stop-Transcript
