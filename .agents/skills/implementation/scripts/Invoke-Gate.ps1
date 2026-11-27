<#
.SYNOPSIS
  Runs the gate of the repository: the command of the `gate` key of `.agents/config.json` (references/gate.md).
  Prints its output and exits with its exit code. Without `gate` (key missing or false) it prints `skipped: no gate configured` and exits 0.
.PARAMETER WhatIf
  Prints the command and exits 0 without running it.
#>
param([switch]$WhatIf)
$ErrorActionPreference = 'Stop'

$root = (& git rev-parse --show-toplevel 2>$null)
if ($LASTEXITCODE -ne 0 -or -not $root) { $root = (Get-Location).Path }
$configFile = Join-Path $root '.agents/config.json'
$gate = $null
if (Test-Path -LiteralPath $configFile -PathType Leaf) {
  try { $cfg = Get-Content -LiteralPath $configFile -Raw | ConvertFrom-Json -ErrorAction Stop }
  catch { Write-Error "error: .agents/config.json is not valid JSON ($($_.Exception.Message))" -ErrorAction Continue; exit 1 }
  $prop = if ($cfg -is [pscustomobject]) { $cfg.PSObject.Properties['gate'] } else { $null }
  if ($prop) { $gate = $prop.Value }
}
if ($gate -isnot [string] -or -not $gate.Trim()) { Write-Output 'skipped: no gate configured'; exit 0 }
if ($WhatIf) { Write-Output $gate; exit 0 }

Push-Location -LiteralPath $root
try {
  if ($IsWindows) { & cmd.exe /d /s /c $gate } else { & sh -c $gate }
  exit $LASTEXITCODE
}
finally { Pop-Location }
