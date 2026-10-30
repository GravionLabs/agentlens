# Download

Every push to `main` publishes a release, so the newest one is always at <https://github.com/GravionLabs/agentlens/releases/latest>. It has three kinds of files:

- **The desktop application:** an installer per platform.
- **The command line and the MCP server:** one archive per platform that holds both `agentlens` and `agentlens-mcp`.
- **`SHA256SUMS`:** the checksums of the archives.

## The desktop application

The [desktop application](application.md) comes as an installer. Their names carry the version, so pick the one for your system on the [release page](https://github.com/GravionLabs/agentlens/releases/latest):

| Platform               | Installer                          |
| ---------------------- | ---------------------------------- |
| Linux x86_64           | `.deb`, `.rpm` or AppImage         |
| macOS Apple silicon    | `.dmg`                             |
| Windows x86_64         | setup `.exe`                       |

The installers are not signed yet, so the systems warn on first start: on macOS open the app with a right click and choose Open (or allow it in System Settings, Privacy and Security), on Windows choose More info and Run anyway in SmartScreen, and make an AppImage executable (`chmod +x`) before running it. Once installed, the application [updates itself](../updater.md) when you click **Check for updates**.

## The command line and the MCP server

The archives have a name without a version, so these links always lead to the newest release:

| Platform               | Archive                                                                                                                                                                                                                                                                                                                                                |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Linux x86_64           | [`agentlens-x86_64-unknown-linux-gnu.tar.gz`](https://github.com/GravionLabs/agentlens/releases/latest/download/agentlens-x86_64-unknown-linux-gnu.tar.gz)   |
| Linux aarch64          | [`agentlens-aarch64-unknown-linux-gnu.tar.gz`](https://github.com/GravionLabs/agentlens/releases/latest/download/agentlens-aarch64-unknown-linux-gnu.tar.gz) |
| macOS Apple silicon    | [`agentlens-aarch64-apple-darwin.tar.gz`](https://github.com/GravionLabs/agentlens/releases/latest/download/agentlens-aarch64-apple-darwin.tar.gz)         |
| Windows x86_64         | [`agentlens-x86_64-pc-windows-msvc.zip`](https://github.com/GravionLabs/agentlens/releases/latest/download/agentlens-x86_64-pc-windows-msvc.zip)           |

Check the download against [`SHA256SUMS`](https://github.com/GravionLabs/agentlens/releases/latest/download/SHA256SUMS) from the same release:

```sh
sha256sum -c --ignore-missing SHA256SUMS
```

Unpack the archive and put `agentlens` and `agentlens-mcp` (the [MCP server](mcp-server.md)) on your `PATH`. The desktop installers contain both programs as well, so with an installer you do not need the archive:

| Installer                       | Where `agentlens` and `agentlens-mcp` are                                       |
| ------------------------------- | ------------------------------------------------------------------------------- |
| Windows setup `.exe`            | the install directory of the application; the action *Install command line tools* adds it to your `PATH` (see below) |
| Linux `.deb` or `.rpm`          | `/usr/bin`, which is on your `PATH` already                                     |
| macOS `.dmg` and Linux AppImage | inside the application; the action *Install command line tools* makes them available (see below) |

No installer changes your `PATH`: on Windows, use **Install command line tools** in the application ([Windows: the `PATH` entry](#windows-the-path-entry)).

### Install command line tools (macOS and AppImage)

On macOS and in an AppImage, click **Install command line tools** in the status bar of the application. It creates `agentlens` and `agentlens-mcp` in `~/.local/bin` (and the folder, if it is missing), and only when you click:

- **macOS:** two symbolic links to the programs inside the application, so they follow every update of the application. Move `AgentLens.app` to `/Applications` first: started from the disk image (`/Volumes/...`) or from where it was downloaded (macOS then runs it from a temporary App Translocation path), the application refuses to install, because the links would break.
- **AppImage:** two copies of the programs, because the path inside a mounted AppImage changes on every run and a link would break. A copy does not follow an update: after you update the AppImage, click **Install command line tools** again and it replaces the old copies. AgentLens recognizes its own copies by a marker in the programs (it reads the file and never runs it), so an old copy is replaced, not reported as foreign.
- **Foreign files:** the action never overwrites or removes a file it did not create (your own script, or a link of yours to somewhere else). It names the file and skips it.
- **Safe replacement:** a new link or copy is written under a temporary name and then renamed over the old one, so a failed install leaves the working one in place.
- **Remove:** **Remove command line tools** removes only those links and copies (old ones too) and nothing else.
- **`PATH`:** if `~/.local/bin` is not on your `PATH`, the application shows the line to add for zsh and bash (`export PATH="$HOME/.local/bin:$PATH"`). It never edits a shell profile; you add the line yourself. The check uses the `PATH` the application was started with, which can differ from your shell's: an application started from the macOS Finder or Dock does not read your shell profile, so it may report the folder as missing although your terminal has it. Check with `echo $PATH` in a terminal.

The action is not offered for the `.deb` and `.rpm` packages, which install into `/usr/bin` already. On Windows it adds a `PATH` entry instead (next section). The demo in the browser does not have it.

### Windows: the `PATH` entry

**From the application.** Click **Command line tools** in the status bar, then **Install command line tools**. On Windows nothing is copied or linked: the application adds its install directory (the folder of `agentlens.exe` and `agentlens-mcp.exe`) to your user `PATH` (`HKEY_CURRENT_USER\Environment\Path`), and only when you click:

- It adds the directory once, at the end, and keeps every other entry, their order and the type of the value (entries such as `%USERPROFILE%\...` stay unexpanded). Directories compare without regard to case or a trailing backslash, so an entry an earlier installer added counts as installed and is not added a second time.
- Before it changes the value, it saves the old one to `HKEY_CURRENT_USER\Software\GravionLabs\AgentLens\PathBackup` (value `Path`, see [Restore `PATH` on Windows](#restore-path-on-windows)). If it cannot read your `PATH` (for example a value of another type), it changes nothing and says so.
- **Remove command line tools** removes exactly that entry and nothing else. Clicking either button a second time changes nothing.
- Another folder on your `PATH` that holds `agentlens.exe` (an older installation, a `cargo install`) is named but never changed.
- Programs that are already running keep the old `PATH`: open a **new** terminal (or restart your editor) after you click.

**Not from the installer.** The setup `.exe` does not change your `PATH`, neither when you install nor when the application [updates itself](../updater.md) (the updater runs the setup `.exe` again): after installing, `agentlens` and `agentlens-mcp` are on your `PATH` only once you click **Install command line tools**. An update keeps an entry you added from the application, and an entry you removed stays removed.

**Upgrading from an earlier version:** versions before this change added the install directory to your `PATH` during setup. That entry stays when you update, and the application shows the command line tools as installed. **Remove command line tools** removes it if you do not want it.

If you installed an earlier version with the `.msi`, uninstall it (Settings, Apps) before you run the setup `.exe`; the uninstall removes the `PATH` entry of the `.msi`. The `.msi` is no longer built, and the updater would otherwise install the setup `.exe` next to it.

**Uninstall.** The uninstaller removes the entry of its install directory from your user `PATH` (a per-machine installation: from the machine `PATH`, when the uninstaller runs elevated and the install directory is under `Program Files`), whether the application or an earlier installer added it, and leaves every other entry as it is. Without that entry it leaves `PATH` unchanged. It also leaves `PATH` untouched, and says why in the details of the uninstallation, when:

- your `PATH` is very long (the uninstaller cannot read a value of 1023 characters or more, and a write could cut off entries) or not a text value;
- the install directory contains `;` or `%`, which `PATH` would split or expand.

To remove the entry by hand in these cases (or after deleting the files without the uninstaller):

- Open **Edit environment variables for your account** from the Start menu, select `Path` under **User variables**, click **Edit**, select the AgentLens install directory, click **Delete** and confirm with **OK**.
- Or run this in PowerShell, replacing the directory with your install directory (it edits the user `PATH` only, and a new terminal sees the change):

```powershell
$dir = 'C:\Path\To\AgentLens'
$key = Get-Item 'HKCU:\Environment'
$old = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$new = ($old -split ';' | Where-Object { $_ -and $_ -ne $dir }) -join ';'
Set-ItemProperty 'HKCU:\Environment' -Name Path -Value $new -Type ExpandString
```

### Restore `PATH` on Windows

Before **Install command line tools** or **Remove command line tools** changes your user `PATH`, the application saves the old value in the registry under `HKEY_CURRENT_USER\Software\GravionLabs\AgentLens\PathBackup` (value `Path`). Earlier versions of the setup `.exe` saved it under the same key when they added the entry (a per-machine installation under `HKEY_LOCAL_MACHINE`). The uninstaller does not write or remove the backup. To put the old value back, run this in PowerShell, then sign out and in again or open a new terminal:

```powershell
$old = (Get-Item 'HKCU:\Software\GravionLabs\AgentLens\PathBackup').GetValue('Path', '', 'DoNotExpandEnvironmentNames'); Set-ItemProperty 'HKCU:\Environment' -Name Path -Value $old -Type ExpandString
```

`ExpandString` keeps entries such as `%USERPROFILE%\...` unexpanded, as Windows stores them. To only add the install directory by hand, open **Edit environment variables for your account**, select `Path`, click **Edit**, click **New** and enter the install directory.

**If an earlier version cut your `PATH` short:** before this fix, the installer replaced a `PATH` of 1023 characters or more with the install directory alone, and no backup exists. Rebuild the list in **Edit environment variables for your account**: select `Path` (or click **New** if it is gone) and add the entries your tools need. Windows itself puts this one there on every account:

```text
%USERPROFILE%\AppData\Local\Microsoft\WindowsApps
```

Add the folders of the other programs you use (for example Git, Node.js or Python) again too, or reinstall them, which restores their entries. The system `PATH` was never changed by this.

## From source

Install the Rust toolchain and run `cargo install --path crates/agentlens-cli` (and `--path crates/agentlens-mcp` for the MCP server).

## Next

[Getting started](getting-started.md) shows the first commands.
