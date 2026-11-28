<#
.SYNOPSIS
  Creates the release-test issue of a milestone when .agents/config.json has release.test (called by Get-Milestones.ps1 create and insert).
.DESCRIPTION
  Reads release.test (platforms[], template) of .agents/config.json in the current directory. Without it nothing happens and nothing is printed.
  Creates `[PBI] test: release candidate test of <milestone> on <platforms>` with the labels pbi, release-test and interactive, the milestone
  and the body of the template ({milestone}, {platforms} replaced) plus a `Tier: interactive` line. A release-test issue (open or closed)
  already in the milestone is not duplicated. Prints one JSON object { created, number, url } when release.test is set.
.PARAMETER Milestone
  Title of the milestone (e.g. M4).
.PARAMETER MilestoneNumber
  REST number of the milestone.
.PARAMETER Repo
  owner/name; default: the repository of the current directory.
.PARAMETER ConfigPath
  Path of the project config; default .agents/config.json.
#>
param(
  [Parameter(Mandatory)][string]$Milestone,
  [Parameter(Mandatory)][int]$MilestoneNumber,
  [string]$Repo = '{owner}/{repo}',
  [string]$ConfigPath = '.agents/config.json'
)
$ErrorActionPreference = 'Stop'
function Fail([string]$m) { Write-Error $m -ErrorAction Continue; exit 1 }

if (-not (Test-Path $ConfigPath)) { exit 0 }
$cfg = Get-Content -Raw $ConfigPath | ConvertFrom-Json
$rt = $cfg.release.test
if (-not $rt -or -not $rt.platforms -or -not $rt.template) { exit 0 }
$platforms = (@($rt.platforms) -join ', ')
if (-not (Test-Path $rt.template)) { Fail "release.test.template '$($rt.template)' not found" }

$raw = & gh api "repos/$Repo/issues?milestone=$MilestoneNumber&state=all&labels=release-test&per_page=100" --paginate --slurp 2>$null
if ($LASTEXITCODE -ne 0) { Fail 'listing the release-test issues failed' }
$flat = [System.Collections.Generic.List[object]]::new()
function Add-Leaf($x) {
  if ($x -is [System.Collections.IList]) { foreach ($e in $x) { Add-Leaf $e } }
  elseif ($null -ne $x) { $flat.Add($x) }
}
Add-Leaf ($raw | Out-String | ConvertFrom-Json -NoEnumerate)
$found = $flat | Where-Object { $null -eq $_.pull_request } | Select-Object -First 1
if ($found) {
  [ordered]@{ created = $false; number = $found.number; url = $found.html_url } | ConvertTo-Json
  exit 0
}

$text = (Get-Content -Raw $rt.template).TrimEnd().Replace('{milestone}', $Milestone).Replace('{platforms}', $platforms)
$body = "$text`n`nTier: interactive`n"
$title = "[PBI] test: release candidate test of $Milestone on $platforms"
$out = & gh api -X POST "repos/$Repo/issues" -f "title=$title" -f "body=$body" -f 'labels[]=pbi' -f 'labels[]=release-test' -f 'labels[]=interactive' -F "milestone=$MilestoneNumber" 2>$null
if ($LASTEXITCODE -ne 0) { Fail "creating the release-test issue of $Milestone failed" }
$i = $out | ConvertFrom-Json
[ordered]@{ created = $true; number = $i.number; url = $i.html_url } | ConvertTo-Json
