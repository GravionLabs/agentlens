<#
.SYNOPSIS
  Compares the version of a release tag with the version GitVersion expects from the commits; prints one JSON object.
.DESCRIPTION
  Read-only. The previous tag is the highest v<x.y.z> below -Tag. Commits are those in <previous>..<Tag>.
  Bump: `<type>!:`, `BREAKING CHANGE:` or `+semver: major` -> major; `feat:` or `+semver: minor` -> minor;
  anything else -> patch. Fields: tag, previous, expected, match, bump, deciding[] (the commits that set the bump).
  No previous tag: previous null, expected = the tag version, match true.
.PARAMETER Tag
  Release tag, v<x.y.z>.
#>
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Tag)
$ErrorActionPreference = 'Stop'
if ($Tag -notmatch '^v\d+\.\d+\.\d+$') { [Console]::Error.WriteLine("invalid tag '$Tag', expected v<x.y.z>"); exit 1 }
$cur = [version]$Tag.Substring(1)
$prevTag = git tag --list 'v*' | Where-Object { $_ -match '^v\d+\.\d+\.\d+$' -and ([version]$_.Substring(1)) -lt $cur } |
  Sort-Object { [version]$_.Substring(1) } | Select-Object -Last 1
if (-not $prevTag) {
  [ordered]@{ tag = $Tag; previous = $null; expected = $Tag.Substring(1); match = $true; bump = $null; deciding = @() } | ConvertTo-Json -Depth 4
  exit 0
}
$us = [string][char]0x1f
$rs = [string][char]0x1e
$raw = git log "$prevTag..$Tag" --no-merges "--format=%h$us%B$rs"
if ($LASTEXITCODE -ne 0) { [Console]::Error.WriteLine("git log $prevTag..$Tag failed"); exit 1 }
$rank = @{ major = 3; minor = 2; patch = 1 }
$commits = @(foreach ($c in ($raw -join "`n") -split $rs) {
    $c = $c.Trim(); if (-not $c) { continue }
    $sha, $msg = $c -split $us, 2
    $subject = ($msg -split "`n")[0]
    $bump = if ($subject -match '^\w+(\([^)]*\))?!:' -or $msg -match '(?m)^BREAKING[ -]CHANGE:' -or $msg -match '\+semver:\s*(major|breaking)') { 'major' }
    elseif ($subject -match '^feat(\([^)]*\))?:' -or $msg -match '\+semver:\s*(minor|feature)') { 'minor' } else { 'patch' }
    [pscustomobject]@{ sha = $sha; subject = $subject; bump = $bump }
  })
$top = 1
foreach ($c in $commits) { if ($rank[$c.bump] -gt $top) { $top = $rank[$c.bump] } }
$bump = ($rank.GetEnumerator() | Where-Object Value -eq $top).Key
$p = [version]$prevTag.Substring(1)
$exp = switch ($bump) { 'major' { "$($p.Major + 1).0.0" } 'minor' { "$($p.Major).$($p.Minor + 1).0" } default { "$($p.Major).$($p.Minor).$($p.Build + 1)" } }
[ordered]@{
  tag = $Tag; previous = $prevTag; expected = $exp; match = ($exp -eq $Tag.Substring(1)); bump = $bump
  deciding = @($commits | Where-Object bump -eq $bump | ForEach-Object { "$($_.sha) $($_.subject)" })
} | ConvertTo-Json -Depth 4
