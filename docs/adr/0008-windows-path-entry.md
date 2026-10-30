# 0008: Command line tools on the Windows PATH from the application

- Status: Accepted (decided by the owner on 2026-10-30)
- Context: Bug #443 (the installer overwrote the user `PATH`), Feature #460 and PBI #461; extends ADR 0006 to Windows

## Context

On Windows the installer used to add the folder of the command line tools (`agentlens`, `agentlens-mcp`) to the user `PATH`. The NSIS hook wrote the value with NSIS string functions, which cannot read a value longer than 1024 characters, and it overwrote the user `PATH` (#443). A developer, the main audience of AgentLens, often has a longer `PATH`, so the installer either lost entries or did nothing for that user.

The installer also ran on every update, so an entry the user had removed came back. Some users do not want the command line tools on `PATH` at all. ADR 0006 keeps the same question on macOS and in an AppImage in the application and does not decide Windows.

Rust edits the registry without a length limit, its string logic can be unit-tested on every platform, and the application runs only when the user starts it.

## Options

1. **The installer edits `PATH`** (status quo, fixed by #443). The hook reads and writes the value without truncation and keeps a backup. Still on every install and update, and the user cannot choose.
2. **The installer asks.** A page with a checkbox in a custom NSIS template, or a message box, or a feature tree in an MSI. A custom template is more maintenance than the installer is worth, and the updater runs the installer again without the page.
3. **An opt-in from the application** (*Install command line tools* and *Remove command line tools*, as on macOS and in an AppImage). Written by Rust, on the user's click.
4. **Copy the programs into `%LOCALAPPDATA%\Microsoft\WindowsApps`.** Rejected: that is a Windows-owned directory for execution aliases, and a copy does not follow an update.

## Decision

- **Option 3**, accepted. The status bar offers *Install command line tools* and *Remove command line tools* on Windows too. The installer no longer edits `PATH` on install or update; it keeps only the removal of the entry on uninstall.
- **What is written:** only `HKCU\Environment\Path`, and only the entry equal to the install directory of the running application (`bin_dir`). The comparison is case-insensitive and ignores a trailing backslash. The value type stays `REG_EXPAND_SZ` (or `REG_SZ` if it was), and all other entries keep their order.
- **Backup first:** the old value is saved to `HKCU\Software\GravionLabs\AgentLens\PathBackup` before any change (the key of #443). After the change, `WM_SETTINGCHANGE` is broadcast so that new processes see the new `PATH`.
- **Only on a click.** Looking at the state reads the registry and writes nothing.
- **Ours and foreign entries:** the exact entry of `bin_dir` is ours. An entry of another AgentLens install directory is reported and never removed by *Install*. *Remove* removes only the `bin_dir` entry.
- **Uninstall:** the NSIS hook removes the entry, because the user may never start the application again.

## Consequences

- After installing, the CLI is not on `PATH` until the user clicks *Install command line tools*. The first-run hint and the guide say so.
- Users who got an entry from an earlier version keep it, and the application shows it as installed.
- The installer no longer depends on the length of `PATH`, and an update no longer adds an entry the user removed.
- The registry logic is unit-tested on every platform; the broadcast and the real registry are checked on Windows.
- The MSI is not part of this decision (dropped by #458).
