<#
.SYNOPSIS
  Tests the NSIS installer against a synthetic user PATH (HKCU\Environment\Path).

.DESCRIPTION
  For each case: set the user PATH, install silently, check, uninstall silently, check again.
  The runner's own PATH value is restored after every case. Synthetic paths only.
  Windows only; also runnable by hand (it changes your user PATH temporarily and installs
  and uninstalls the application, so use a test machine).

  Cases:
    short    a few synthetic entries, REG_EXPAND_SZ
    long     more than 1100 characters; the installer must leave PATH untouched
    expand   entries with %USERPROFILE%, REG_EXPAND_SZ (must stay unexpanded)
    missing  no Path value at all

  Checks after install: every old entry present in the same order, the install directory
  added exactly once (long: PATH unchanged), value type unchanged, and, when the installer
  wrote PATH, HKCU\Software\GravionLabs\AgentLens\PathBackup holds the old value.
  After uninstall: the old value is back and the type is unchanged (missing: no entries).

  Exit code: 0 when every case passed, otherwise the code of the first failure:
    10 installer failed      11 uninstaller failed     12 install directory not found
    20 old entries lost or reordered after install     21 install directory not added exactly once
    22 PATH changed although it must stay untouched    23 value type changed after install
    24 backup missing or different                     30 old value not restored after uninstall
    31 value type changed after uninstall              40 bad arguments

.PARAMETER Installer
  Path of the NSIS setup .exe.
.PARAMETER Case
  short, long, expand, missing or all (default).
.PARAMETER InstallDir
  Install directory of the per-user install (default: %LOCALAPPDATA%\AgentLens).
#>
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [ValidateSet('short', 'long', 'expand', 'missing', 'all')][string]$Case = 'all',
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

function Split-Entries([string]$value) {
  @($value -split ';' | Where-Object { $_ -ne '' })
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
  switch ($name) {
    'short' {
      @{ State = [pscustomobject]@{ Value = 'C:\Synthetic\One;C:\Synthetic\Two;C:\Synthetic\Three'; Kind = 'ExpandString' }
         Untouched = $false }
    }
    'long' {
      @{ State = [pscustomobject]@{ Value = (New-LongPath); Kind = 'ExpandString' }
         Untouched = $true }
    }
    'expand' {
      @{ State = [pscustomobject]@{
           Value = '%USERPROFILE%\AppData\Local\Synthetic\bin;C:\Synthetic\Tools;%USERPROFILE%\.synthetic\bin'
           Kind = 'ExpandString' }
         Untouched = $false }
    }
    'missing' { @{ State = $null; Untouched = $false } }
  }
}

function Test-Case([string]$name) {
  $setup = Get-CaseSetup $name
  $old = $setup.State
  $oldEntries = if ($null -eq $old) { @() } else { Split-Entries $old.Value }

  Set-UserPath $old
  $Hkcu.DeleteSubKeyTree($BackupSubKey, $false)

  Invoke-Silent $Installer @('/S') 10 'installer'
  if (-not (Test-Path -LiteralPath $InstallDir)) {
    throw [CaseFailure]::new(12, "install directory not found: $InstallDir")
  }

  $after = Get-UserPath
  if ($setup.Untouched) {
    if ($null -eq $after -or $after.Value -cne $old.Value) {
      $len = if ($null -eq $after) { 0 } else { $after.Value.Length }
      throw [CaseFailure]::new(22, "PATH must stay untouched but changed ($($old.Value.Length) -> $len characters)")
    }
  } else {
    if ($null -eq $after) { throw [CaseFailure]::new(21, 'PATH value is missing after install') }
    $entries = Split-Entries $after.Value
    $ours = @($entries | Where-Object { $_ -ieq $InstallDir })
    if ($ours.Count -ne 1) {
      throw [CaseFailure]::new(21, "install directory present $($ours.Count) times, expected once")
    }
    $others = @($entries | Where-Object { $_ -ine $InstallDir })
    if (($others -join ';') -cne ($oldEntries -join ';')) {
      throw [CaseFailure]::new(20, "old entries changed: '$($oldEntries -join ';')' -> '$($others -join ';')'")
    }
    # a missing value is created as REG_EXPAND_SZ by the installer
    $expectedKind = if ($null -eq $old) { 'ExpandString' } else { $old.Kind }
    if ($after.Kind -ne $expectedKind) {
      throw [CaseFailure]::new(23, "value type changed: $expectedKind -> $($after.Kind)")
    }
    if ($null -ne $old) {
      $backup = Get-Backup
      if ($null -eq $backup -or $backup.Value -cne $old.Value -or $backup.Kind -ne 'ExpandString') {
        throw [CaseFailure]::new(24, "backup under HKCU\$BackupSubKey missing or different")
      }
    }
  }

  Invoke-Silent (Join-Path $InstallDir 'uninstall.exe') @('/S', "_?=$InstallDir") 11 'uninstaller'

  $restored = Get-UserPath
  if ($null -eq $old) {
    $left = if ($null -eq $restored) { @() } else { Split-Entries $restored.Value }
    if ($left.Count -ne 0) { throw [CaseFailure]::new(30, "entries left after uninstall: '$($left -join ';')'") }
  } else {
    if ($null -eq $restored -or $restored.Value -cne $old.Value) {
      $got = if ($null -eq $restored) { '<no value>' } else { $restored.Value }
      throw [CaseFailure]::new(30, "old value not restored: '$got'")
    }
    if ($restored.Kind -ne $old.Kind) {
      throw [CaseFailure]::new(31, "value type changed after uninstall: $($old.Kind) -> $($restored.Kind)")
    }
  }
}

$names = if ($Case -eq 'all') { @('short', 'long', 'expand', 'missing') } else { @($Case) }
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
