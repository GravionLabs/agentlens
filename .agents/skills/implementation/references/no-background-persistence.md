# No background persistence

GravionLabs apps never install background persistence: nothing that runs or starts when the app is not running, and nothing that survives uninstall. Work that must repeat runs only while the app runs, in its own process (timer, worker, loop of the app itself).

## Forbidden

- Windows scheduled tasks: `schtasks`, `Register-ScheduledTask`, Task Scheduler COM
- Windows services: `New-Service`, `sc create`
- `Run` and `RunOnce` registry keys
- `tauri-plugin-autostart` and other autostart or auto-launch libraries
- macOS LaunchAgents and LaunchDaemons
- systemd `--user` units and timers
- cron entries
- startup-folder shortcuts

Also forbidden: an installer that spawns PowerShell or another script host. Registry changes in an installer use native NSIS commands.

## Exception

Only with an accepted ADR (`adr`) and the explicit approval of the owner. Then:

- opt-in, never on by default
- removed by the uninstaller (NSIS `NSIS_HOOK_PREUNINSTALL`, or package removal) and covered by a test
- signed binaries

Without both, do not add the mechanism; ask the owner.
