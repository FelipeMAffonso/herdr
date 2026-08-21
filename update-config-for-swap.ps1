# Turn on the fork's new sidebar tokens in Felipe's config (2026-08-20). Runs
# INSIDE the detached swap, after the new binary is installed, because the old
# binary rejects unknown tokens like need_edge/waiting at config check.
$config = Join-Path $env:APPDATA "herdr\config.toml"
$text = [IO.File]::ReadAllText($config)
$old = @'
claude = [
  ["state_icon", { token = "workspace", fg = "#ECEAE4", bold = true }],
  [{ token = "terminal_title_stripped", fg = "#8A8880" }],
]
'@
$new = @'
claude = [
  ["need_edge", "state_icon", { token = "workspace", fg = "#ECEAE4", bold = true }],
  ["need_edge", { token = "terminal_title_stripped", fg = "#8A8880" }],
  ["need_edge", "waiting"],
]
'@
if ($text.Contains($old)) {
  [IO.File]::WriteAllText($config, $text.Replace($old, $new))
  Write-Output "config: claude rows now carry need_edge + waiting"
} elseif ($text.Contains('"need_edge"')) {
  Write-Output "config: need_edge rows already present"
} else {
  Write-Output "config: claude block not found verbatim - rows NOT changed (swap unaffected)"
}
& "$env:LOCALAPPDATA\Programs\Herdr\bin\herdr.exe" config check
