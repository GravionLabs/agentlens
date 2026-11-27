<#
.SYNOPSIS
  Creates origin/release/next from the default branch when it is missing (the `cut` of `github-release` for "branch missing"); prints one JSON object.
.DESCRIPTION
  Reads `git ls-remote --heads origin release/next`. Present: nothing to do. Missing: the default branch
  (`gh api repos/{owner}/{repo} --jq .default_branch`) is fetched and pushed to `refs/heads/release/next`.
  Output: {status, default, detail}. `status`: present, created (`detail` = `release/next created from <default>`),
  refused (the push or the default-branch lookup failed; `detail` carries the git or gh error) or unreachable
  (origin cannot be read; nothing is created). Exit 0 in all four cases, 1 outside a git repository.
.PARAMETER ProjectRoot
  Repository (default: current directory).
#>
param([string]$ProjectRoot = (Get-Location).Path)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
& git -C $root rev-parse --show-toplevel *> $null
if ($LASTEXITCODE -ne 0) { Write-Error 'not inside a git repository' -ErrorAction Continue; exit 1 }
function Out-State([string]$status, [string]$default, [string]$detail) {
  [ordered]@{ status = $status; default = $default; detail = $detail } | ConvertTo-Json -Compress
  exit 0
}
$ls = & git -C $root ls-remote --heads origin release/next 2>$null
if ($LASTEXITCODE -ne 0) { Out-State 'unreachable' $null 'origin not reachable' }
if (@($ls | Where-Object { $_ }).Count) { Out-State 'present' $null 'origin/release/next present' }
Push-Location $root
try {
  $default = ((& gh api 'repos/{owner}/{repo}' --jq .default_branch 2>&1) -join "`n").Trim()
  if ($LASTEXITCODE -ne 0 -or -not $default) { Out-State 'refused' $null "default branch unknown: $default" }
  $err = (& git fetch -q origin $default 2>&1) -join ' '
  if ($LASTEXITCODE -ne 0) { Out-State 'refused' $default "fetch of $default failed: $err" }
  $err = (& git push -q origin "refs/remotes/origin/${default}:refs/heads/release/next" 2>&1) -join ' '
  if ($LASTEXITCODE -ne 0) { Out-State 'refused' $default "push to release/next refused: $($err.Trim())" }
  Out-State 'created' $default "release/next created from $default"
} finally { Pop-Location }
