# Builds the CLI and the MCP server and copies them to src-tauri/binaries as
# <name>-<target-triple>[.exe], where Tauri's bundle.externalBin expects them.
# Usage: scripts/stage-sidecars.ps1 [-Target <target-triple>]   (default: the host triple)
param([string]$Target = "")
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
if (-not $Target) {
  $Target = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
}
if (-not $Target) { throw "cannot determine the target triple" }
$ext = if ($Target -like "*windows*") { ".exe" } else { "" }

Set-Location $root
cargo build --release --locked --target $Target -p agentlens-cli -p agentlens-mcp
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$out = Join-Path $root "src-tauri/binaries"
New-Item -ItemType Directory -Force -Path $out | Out-Null
foreach ($name in "agentlens", "agentlens-mcp") {
  $dest = Join-Path $out "$name-$Target$ext"
  Copy-Item "target/$Target/release/$name$ext" $dest -Force
  Write-Host "staged $dest"
}
