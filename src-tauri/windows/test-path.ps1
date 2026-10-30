<#
.SYNOPSIS
  Tests the NSIS installer and uninstaller against a synthetic user PATH (HKCU\Environment\Path).

.DESCRIPTION
  For each case: set the user PATH, install silently, check, uninstall silently, check again.
  The runner's own PATH value is restored after every case. Synthetic paths only.
  Windows only; also runnable by hand (it changes your user PATH temporarily and installs
  and uninstalls the application, so use a test machine).

  The installer leaves PATH alone (ADR 0008); the uninstaller removes the install directory
  when PATH has it (the application's *Install command line tools* or an earlier installer
  added it) and leaves PATH unchanged otherwise.

  Cases:
    short    a few synthetic entries, REG_EXPAND_SZ
    long     more than 1100 characters
    expand   entries with %USERPROFILE%, REG_EXPAND_SZ (must stay unexpanded)
    missing  no Path value at all
    entry    synthetic entries with the install directory in the middle, REG_EXPAND_SZ

  After install: the value (bytes and type) is unchanged, or still missing, and no backup
  under HKCU\Software\GravionLabs\AgentLens\PathBackup was written.
  After uninstall: without the install directory the value is unchanged (missing: still
  missing); with it, the install directory is gone and every other entry, their order and
  the type are kept.

  Exit code: 0 when every case passed, otherwise the code of the first failure:
    10 installer failed      11 uninstaller failed     12 install directory not found
    22 PATH changed by the installer                   24 backup written by the installer
    30 PATH wrong after uninstall                      31 value type changed after uninstall
    40 bad arguments

.PARAMETER Installer
  Path of the NSIS setup .exe.
.PARAMETER Case
  short, long, expand, missing, entry or all (default).
.PARAMETER InstallDir
  Install directory of the per-user install (default: %LOCALAPPDATA%\AgentLens).
#>
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [ValidateSet('short', 'long', 'expand', 'missing', 'entry', 'all')][string]$Case = 'all',
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'AgentLens')
)
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Installer)) { Write-Error "installer not found: $Installer"; exit 40 }
$Installer = (Resolve-Path -LiteralPath $Installer).Path

$BackupSubKey = 'Software\GravionLabs\AgentLens\PathBackup'
$Hkcu = [Microsoft.Win32.Registry]::CurrentUser

class CaseFailure : System.Exception {
  [int]$Code
  CaseFailure([int]$code, [string]$message) : base($message) { $this.Code = $code }
}

function Get-UserPath {
  # Raw value (variables unexpanded) and kind; $null when there is no Path value.
  $key = $Hkcu.OpenSubKey('Environment')
  try {
    if ($key.GetValueNames() -notcontains 'Path') { return $null }
    [pscustomobject]@{
      Value = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
      Kind  = $key.GetValueKind('Path')
    }
  } finally { $key.Close() }
}

function Set-UserPath($state) {
  $key = $Hkcu.OpenSubKey('Environment', $true)
  try {
    if ($null -eq $state) {
      if ($key.GetValueNames() -contains 'Path') { $key.DeleteValue('Path') }
    } else {
      $key.SetValue('Path', $state.Value, $state.Kind)
    }
  } finally { $key.Close() }
}

function Get-Backup {
  $key = $Hkcu.OpenSubKey($BackupSubKey)
  if ($null -eq $key) { return $null }
  try {
    if ($key.GetValueNames() -notcontains 'Path') { return $null }
    [pscustomobject]@{
      Value = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
      Kind  = $key.GetValueKind('Path')
    }
  } finally { $key.Close() }
}

function Invoke-Silent([string]$exe, [string[]]$arguments, [int]$code, [string]$what) {
  $p = Start-Process -FilePath $exe -ArgumentList $arguments -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw [CaseFailure]::new($code, "$what exited with $($p.ExitCode)") }
}

function Remove-Leftovers {
  $uninstaller = Join-Path $InstallDir 'uninstall.exe'
  if (Test-Path -LiteralPath $uninstaller) {
    # _?= keeps the uninstaller in place and makes the call synchronous
    Start-Process -FilePath $uninstaller -ArgumentList @('/S', "_?=$InstallDir") -Wait | Out-Null
    Remove-Item -LiteralPath $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
  }
  $Hkcu.DeleteSubKeyTree($BackupSubKey, $false)
}

function New-LongPath {
  $entries = 1..40 | ForEach-Object { 'C:\Synthetic\LongPath\Entry{0:D3}\bin' -f $_ }
  $value = $entries -join ';'
  if ($value.Length -le 1100) { throw 'the synthetic long PATH is too short' }
  $value
}

function Get-CaseSetup([string]$name) {
  # State: the user PATH before install; Expected: after uninstall ($null: no value).
  switch ($name) {
    'short' {
      $state = [pscustomobject]@{ Value = 'C:\Synthetic\One;C:\Synthetic\Two;C:\Synthetic\Three'; Kind = 'ExpandString' }
      @{ State = $state; Expected = $state }
    }
    'long' {
      $state = [pscustomobject]@{ Value = (New-LongPath); Kind = 'ExpandString' }
      @{ State = $state; Expected = $state }
    }
    'expand' {
      $state = [pscustomobject]@{
        Value = '%USERPROFILE%\AppData\Local\Synthetic\bin;C:\Synthetic\Tools;%USERPROFILE%\.synthetic\bin'
        Kind  = 'ExpandString' }
      @{ State = $state; Expected = $state }
    }
    'missing' { @{ State = $null; Expected = $null } }
    'entry' {
      @{ State    = [pscustomobject]@{ Value = "C:\Synthetic\One;$InstallDir;%USERPROFILE%\.synthetic\bin"; Kind = 'ExpandString' }
         Expected = [pscustomobject]@{ Value = 'C:\Synthetic\One;%USERPROFILE%\.synthetic\bin'; Kind = 'ExpandString' } }
    }
  }
}

function Format-State($state) {
  if ($null -eq $state) { '<no value>' } else { "$($state.Kind) '$($state.Value)'" }
}

function Test-Same($a, $b) {
  if ($null -eq $a -or $null -eq $b) { return ($null -eq $a) -and ($null -eq $b) }
  ($a.Value -ceq $b.Value) -and ($a.Kind -eq $b.Kind)
}

function Test-Case([string]$name) {
  $setup = Get-CaseSetup $name
  $old = $setup.State

  Set-UserPath $old
  $Hkcu.DeleteSubKeyTree($BackupSubKey, $false)

  Invoke-Silent $Installer @('/S') 10 'installer'
  if (-not (Test-Path -LiteralPath $InstallDir)) {
    throw [CaseFailure]::new(12, "install directory not found: $InstallDir")
  }

  $after = Get-UserPath
  if (-not (Test-Same $after $old)) {
    throw [CaseFailure]::new(22, "PATH changed by the installer: $(Format-State $old) -> $(Format-State $after)")
  }
  if ($null -ne (Get-Backup)) {
    throw [CaseFailure]::new(24, "the installer wrote a backup under HKCU\$BackupSubKey")
  }

  Invoke-Silent (Join-Path $InstallDir 'uninstall.exe') @('/S', "_?=$InstallDir") 11 'uninstaller'

  $restored = Get-UserPath
  $expected = $setup.Expected
  if (-not (Test-Same $restored $expected)) {
    $code = if ($null -ne $restored -and $null -ne $expected -and $restored.Value -ceq $expected.Value) { 31 } else { 30 }
    throw [CaseFailure]::new($code, "PATH after uninstall: expected $(Format-State $expected), got $(Format-State $restored)")
  }
}

$names = if ($Case -eq 'all') { @('short', 'long', 'expand', 'missing', 'entry') } else { @($Case) }
$runnerPath = Get-UserPath
$firstFailure = 0
try {
  foreach ($name in $names) {
    Write-Host "::group::case $name"
    try {
      Test-Case $name
      Write-Host "PASS $name"
    } catch {
      $code = if ($_.Exception -is [CaseFailure]) { $_.Exception.Code } else { 1 }
      Write-Host "FAIL $name (exit code $code): $($_.Exception.Message)"
      if ($firstFailure -eq 0) { $firstFailure = $code }
    } finally {
      Remove-Leftovers
      Set-UserPath $runnerPath
      Write-Host '::endgroup::'
    }
  }
} finally {
  Set-UserPath $runnerPath
}
exit $firstFailure
