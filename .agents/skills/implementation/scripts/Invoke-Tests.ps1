<#
.SYNOPSIS
  Runs the tests of the project with the configured command, never with a guessed tool.
  Precedence per language: the `test.<Language>` entry of `.agents/config.json` (`"test": false` disables), then `scripts.test` of
  package.json (ts, ng: `npm test`), then the default of the detected test tool (vitest: `npx vitest run`, jest: `npx jest`;
  py: `uv run pytest`; rust: `cargo test`; go: `go test ./...`; dotnet: `dotnet test`). Without a source and without a detectable tool the run fails: no guess.
  Without -Language every language found in the repository root runs (package.json: ts, or ng with angular.json; pyproject.toml, pytest.ini or setup.py: py; Cargo.toml: rust; go.mod: go; *.sln, *.slnx, *.csproj: dotnet).
.PARAMETER Language
  Key into `test` of `<repo>/.agents/config.json` (ts, ng, py, dotnet, rust, go).
.PARAMETER Targeted
  The targeted run of a commit: the `targeted` command of the entry (or of the tool default: vitest, jest, pytest) with {files} replaced by -Files;
  without -Files or without such a command the full command runs.
.PARAMETER Files
  Test files for -Targeted (comma-separated or several values).
.PARAMETER WhatIf
  Prints the resolved plan as JSON `[{ language, source, commands }]` (source: config, package.json, default) and exits 0 without running anything.
#>
param(
  [string]$Language,
  [switch]$Targeted,
  [string[]]$Files,
  [switch]$WhatIf
)
$ErrorActionPreference = 'Stop'

function Stop-Fail([string]$Message) { Write-Error "error: $Message" -ErrorAction Continue; exit 1 }
function Test-JsonObject($Value) { $null -ne $Value -and $Value.GetType().Name -eq 'PSCustomObject' }

$root = (& git rev-parse --show-toplevel 2>$null)
if ($LASTEXITCODE -ne 0 -or -not $root) { Write-Output 'skipped: not a git repository'; exit 0 }
function Test-RootFile([string]$Pattern) { [bool](Get-ChildItem -LiteralPath $root -Filter $Pattern -File -ErrorAction SilentlyContinue | Select-Object -First 1) }

# --- config
$testCfg = $null; $haveCfg = $false
$configFile = Join-Path $root '.agents/config.json'
if (Test-Path -LiteralPath $configFile -PathType Leaf) {
  try { $cfg = Get-Content -LiteralPath $configFile -Raw | ConvertFrom-Json -ErrorAction Stop }
  catch { Stop-Fail ".agents/config.json is not valid JSON ($($_.Exception.Message))" }
  if (-not (Test-JsonObject $cfg)) { Stop-Fail '.agents/config.json must be a JSON object' }
  $schemaFile = Join-Path $PSScriptRoot '../schema/config.schema.json'
  if (Test-Path -LiteralPath $schemaFile -PathType Leaf) {
    $known = @((Get-Content -LiteralPath $schemaFile -Raw | ConvertFrom-Json).properties.PSObject.Properties.Name)
    foreach ($p in $cfg.PSObject.Properties.Name) {
      if ($known -notcontains $p) { Stop-Fail ".agents/config.json: unknown key '$p' (known: $($known -join ', '))" }
    }
  }
  $tp = $cfg.PSObject.Properties['test']
  if ($tp) {
    $haveCfg = $true; $testCfg = $tp.Value
    if (-not (($testCfg -is [bool] -and -not $testCfg) -or (Test-JsonObject $testCfg))) {
      Stop-Fail ".agents/config.json: key 'test' must be false or a map of language to entry"
    }
  }
}
if ($haveCfg -and $testCfg -is [bool]) { if ($WhatIf) { ConvertTo-Json -InputObject @() } else { Write-Output 'skipped: disabled' }; exit 0 }

# --- languages
$languages = @()
if ($Language) { $languages = @($Language) }
else {
  $pkg = Test-Path -LiteralPath (Join-Path $root 'package.json') -PathType Leaf
  if ($pkg) { $languages += $(if (Test-Path -LiteralPath (Join-Path $root 'angular.json') -PathType Leaf) { 'ng' } else { 'ts' }) }
  if ((Test-RootFile 'pyproject.toml') -or (Test-RootFile 'pytest.ini') -or (Test-RootFile 'setup.py')) { $languages += 'py' }
  if (Test-RootFile 'Cargo.toml') { $languages += 'rust' }
  if (Test-RootFile 'go.mod') { $languages += 'go' }
  if ((Test-RootFile '*.sln') -or (Test-RootFile '*.slnx') -or (Test-RootFile '*.csproj')) { $languages += 'dotnet' }
  if ($languages.Count -eq 0) {
    if ($WhatIf) { ConvertTo-Json -InputObject @() } else { Write-Output 'skipped: no test command (no language detected)' }
    exit 0
  }
}

# --- package.json facts for ts and ng
$pkgJson = $null
$pkgFile = Join-Path $root 'package.json'
if (Test-Path -LiteralPath $pkgFile -PathType Leaf) {
  try { $pkgJson = Get-Content -LiteralPath $pkgFile -Raw | ConvertFrom-Json -ErrorAction Stop } catch { $pkgJson = $null }
}
function Test-Dependency([string]$Name) {
  if (-not $pkgJson) { return $false }
  foreach ($k in 'dependencies', 'devDependencies') {
    $p = $pkgJson.PSObject.Properties[$k]
    if ($p -and (Test-JsonObject $p.Value) -and $p.Value.PSObject.Properties[$Name]) { return $true }
  }
  $false
}
function Get-JsTool {
  if ((Test-Dependency 'vitest') -or (Test-RootFile 'vitest.config.*')) { return 'vitest' }
  if ((Test-Dependency 'jest') -or (Test-RootFile 'jest.config.*')) { return 'jest' }
  $null
}

# tool defaults: full command and targeted command ({files})
$toolDefaults = @{
  vitest = @{ Full = 'npx vitest run'; Targeted = 'npx vitest run {files}' }
  jest   = @{ Full = 'npx jest'; Targeted = 'npx jest {files}' }
  pytest = @{ Full = 'uv run pytest'; Targeted = 'uv run pytest {files}' }
  cargo  = @{ Full = 'cargo test'; Targeted = $null }
  go     = @{ Full = 'go test ./...'; Targeted = $null }
  dotnet = @{ Full = 'dotnet test'; Targeted = $null }
}

$fileList = @($Files | ForEach-Object { $_ -split ',' } | Where-Object { $_ -and $_.Trim() } | ForEach-Object { $_.Trim() })

function Resolve-Plan([string]$Lang) {
  $full = $null; $targeted = $null; $source = $null
  $entry = if ($haveCfg) { $testCfg.PSObject.Properties[$Lang] } else { $null }
  if ($entry) {
    $ev = $entry.Value
    $cp = if (Test-JsonObject $ev) { $ev.PSObject.Properties['command'] } else { $null }
    if (-not $cp) { Stop-Fail ".agents/config.json: test entry '$Lang' has no 'command'" }
    $full = @($cp.Value | Where-Object { $_ -is [string] -and $_.Trim() })
    if ($full.Count -eq 0) { Stop-Fail ".agents/config.json: test entry '$Lang' has an empty 'command'" }
    $tp = $ev.PSObject.Properties['targeted']
    if ($tp -and $tp.Value -is [string] -and $tp.Value.Trim()) { $targeted = $tp.Value }
    $source = 'config'
  }
  else {
    $tool = $null
    switch ($Lang) {
      { $_ -in 'ts', 'ng' } {
        $tool = Get-JsTool
        $scripts = if ($pkgJson) { $pkgJson.PSObject.Properties['scripts'] } else { $null }
        $hasScript = $scripts -and (Test-JsonObject $scripts.Value) -and $scripts.Value.PSObject.Properties['test']
        if ($hasScript) { $full = @('npm test'); $source = 'package.json' }
        elseif ($tool) { $full = @($toolDefaults[$tool].Full); $source = 'default' }
        if ($tool) { $targeted = $toolDefaults[$tool].Targeted }
      }
      'py' { $full = @($toolDefaults.pytest.Full); $targeted = $toolDefaults.pytest.Targeted; $source = 'default' }
      'rust' { $full = @($toolDefaults.cargo.Full); $source = 'default' }
      'go' { $full = @($toolDefaults.go.Full); $source = 'default' }
      'dotnet' { $full = @($toolDefaults.dotnet.Full); $source = 'default' }
      default { Stop-Fail "unknown language '$Lang' (ts, ng, py, dotnet, rust, go)" }
    }
    if (-not $full) { Stop-Fail "no test command for '$Lang': set test.$Lang in .agents/config.json, add scripts.test to package.json or install a test tool (vitest, jest)" }
  }
  $commands = @($full)
  if ($Targeted -and $fileList.Count -gt 0 -and $targeted) { $commands = @($targeted.Replace('{files}', ($fileList -join ' '))) }
  [ordered]@{ language = $Lang; source = $source; commands = $commands }
}

$plan = @($languages | ForEach-Object { Resolve-Plan $_ })
if ($WhatIf) { ConvertTo-Json -Depth 4 -InputObject @($plan); exit 0 }

$code = 0; $ran = 0
Push-Location -LiteralPath $root
try {
  foreach ($p in $plan) {
    foreach ($c in $p.commands) {
      $t = @($c.Trim() -split '\s+' | Where-Object { $_ })
      if (-not (Get-Command $t[0] -ErrorAction SilentlyContinue)) { Stop-Fail "$($t[0]) not found (test command for $($p.language), source $($p.source))" }
      $al = @($t | Select-Object -Skip 1)
      & $t[0] @al
      $ran++
      if ($LASTEXITCODE -ne 0) { $code = $LASTEXITCODE; break }
    }
    if ($code -ne 0) { break }
  }
} finally { Pop-Location }
if ($code -ne 0) { exit $code }
Write-Output "tested: $ran command(s)"
