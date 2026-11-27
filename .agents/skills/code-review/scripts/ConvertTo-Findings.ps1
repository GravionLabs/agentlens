<#
.SYNOPSIS
  Maps a `review.schema.json` result to the findings format of poimen (`findings.json`, schema `findings.schema.json` in the docs of GravionLabs/poimen).
.DESCRIPTION
  Single place of the mapping for `code-review` (form `findings`, stdout) and `deliver` (file). `blocking[]` becomes severity `blocking`
  with `file`, `line`, `title`, `why` (and `rule`); `notes[]` (strings) become severity `note` without location (title and why are the note);
  `unmet[]` (strings) become severity `blocking` without location. Prints the JSON, or with -Worktree writes
  `<Worktree>/.agents/state/findings.json` and makes sure `.agents/state/` is ignored (appended to `<git-common-dir>/info/exclude` when not).
.PARAMETER Review
  Path of a JSON file with the review result; an optional top-level `unmet` (array of strings) is read too (the subagent result of deliver).
.PARAMETER Sha
  The reviewed commit (7 to 64 hex characters).
.PARAMETER Source
  Who made the review (default deliver).
.PARAMETER Worktree
  Write the file into this worktree instead of printing.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Review,
  [Parameter(Mandatory)][string]$Sha,
  [string]$Source = 'deliver',
  [string]$Worktree
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($Sha -notmatch '^[0-9a-fA-F]{7,64}$') { throw "Sha '$Sha' is not a commit id (7 to 64 hex characters)." }
$doc = Get-Content -LiteralPath $Review -Raw | ConvertFrom-Json
function Get-Items($o, [string]$n) { if ($o.PSObject.Properties[$n]) { @($o.$n) } else { @() } }

$findings = [System.Collections.Generic.List[object]]::new()
foreach ($b in (Get-Items $doc 'blocking')) {
  $f = [ordered]@{ file = $b.file; line = [int]$b.line; severity = 'blocking'; title = $b.title; why = $b.why }
  if ($b.PSObject.Properties['rule']) { $f.rule = $b.rule }
  $findings.Add($f)
}
foreach ($u in (Get-Items $doc 'unmet')) { $findings.Add([ordered]@{ severity = 'blocking'; title = "Unmet criterion: $u"; why = "Acceptance criterion not met: $u" }) }
foreach ($n in (Get-Items $doc 'notes')) { $findings.Add([ordered]@{ severity = 'note'; title = $n; why = $n }) }

$json = [ordered]@{ version = 1; sha = $Sha; source = $Source; findings = @($findings) } | ConvertTo-Json -Depth 6
if (-not $Worktree) { $json; return }

$dir = Join-Path $Worktree '.agents/state'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Set-Content -LiteralPath (Join-Path $dir 'findings.json') -Value $json -Encoding utf8
& git -C $Worktree check-ignore -q '.agents/state/findings.json' 2>$null
if ($LASTEXITCODE -ne 0) {
  $common = (& git -C $Worktree rev-parse --path-format=absolute --git-common-dir).Trim()
  $exclude = Join-Path $common 'info/exclude'
  New-Item -ItemType Directory -Force -Path (Split-Path $exclude) | Out-Null
  Add-Content -LiteralPath $exclude -Value '.agents/state/'
}
