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
| Windows x86_64         | `.msi` or setup `.exe`             |

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
| Windows `.msi` or setup `.exe`  | the install directory of the application                                        |
| Linux `.deb` or `.rpm`          | `/usr/bin`, which is on your `PATH` already                                     |
| macOS `.dmg` and Linux AppImage | inside the application; the action *Install command line tools* makes them available (see below) |

The Windows installers add their install directory to your `PATH` (next section). The Linux and macOS installers do not.

### Install command line tools (macOS and AppImage)

On macOS and in an AppImage, click **Install command line tools** in the status bar of the application. It creates `agentlens` and `agentlens-mcp` in `~/.local/bin` (and the folder, if it is missing), and only when you click:

- **macOS:** two symbolic links to the programs inside the application, so they follow every update of the application. Move `AgentLens.app` to `/Applications` first: started from the disk image (`/Volumes/...`) or from where it was downloaded (macOS then runs it from a temporary App Translocation path), the application refuses to install, because the links would break.
- **AppImage:** two copies of the programs, because the path inside a mounted AppImage changes on every run and a link would break. A copy does not follow an update: after you update the AppImage, click **Install command line tools** again and it replaces the old copies. AgentLens recognizes its own copies by a marker in the programs (it reads the file and never runs it), so an old copy is replaced, not reported as foreign.
- **Foreign files:** the action never overwrites or removes a file it did not create (your own script, or a link of yours to somewhere else). It names the file and skips it.
- **Safe replacement:** a new link or copy is written under a temporary name and then renamed over the old one, so a failed install leaves the working one in place.
- **Remove:** **Remove command line tools** removes only those links and copies (old ones too) and nothing else.
- **`PATH`:** if `~/.local/bin` is not on your `PATH`, the application shows the line to add for zsh and bash (`export PATH="$HOME/.local/bin:$PATH"`). It never edits a shell profile; you add the line yourself. The check uses the `PATH` the application was started with, which can differ from your shell's: an application started from the macOS Finder or Dock does not read your shell profile, so it may report the folder as missing although your terminal has it. Check with `echo $PATH` in a terminal.

The action is not offered for the `.deb` and `.rpm` packages, which install into `/usr/bin` already, and not on Windows, where the installer adds the program folder to `PATH`. The demo in the browser does not have it.

### Windows: the `PATH` entry

Both Windows installers (setup `.exe` and `.msi`) add their install directory to your user `PATH`, so `agentlens` and `agentlens-mcp` work without any further step:

- **Setup `.exe`:** it installs for your account only and adds the directory to your user `PATH`. A setup `.exe` built for a per-machine installation adds it to the machine `PATH` instead, but only when it runs elevated and the install directory is under `Program Files` (a directory that only administrators can write to); with any other directory it leaves the machine `PATH` untouched.
- **`.msi`:** it installs for all users into `Program Files`, but adds the directory to the user `PATH` of the account that runs the installation only. Other users of the machine find the programs in the install directory and run them by their full path or add the directory to their own `PATH`. The entry is removed when that same account uninstalls the application.

Programs that are already running keep the old `PATH`: open a **new** terminal (or restart your editor) before you use the commands.

The uninstaller removes exactly that entry and leaves every other entry as it is. To remove it by hand (for example after deleting the files without the uninstaller):

- Open **Edit environment variables for your account** from the Start menu, select `Path` under **User variables**, click **Edit**, select the AgentLens install directory, click **Delete** and confirm with **OK**.
- Or run this in PowerShell, replacing the directory with your install directory (it edits the user `PATH` only, and a new terminal sees the change):

```powershell
$dir = 'C:\Path\To\AgentLens'
$key = Get-Item 'HKCU:\Environment'
$old = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$new = ($old -split ';' | Where-Object { $_ -and $_ -ne $dir }) -join ';'
Set-ItemProperty 'HKCU:\Environment' -Name Path -Value $new -Type ExpandString
```

The setup `.exe` leaves `PATH` untouched in these cases and says why in the details of the installation:

- your existing `PATH` is very long (1023 characters or more), because the installer cannot read and write such a value safely without cutting off entries, or the new entry would not fit;
- the install directory contains `;` or `%`, which `PATH` would split or expand.

The installation still succeeds, but you must add the install directory by hand, with the dialog above (choose **New** instead of **Delete**). The uninstaller then has nothing to remove.

## From source

Install the Rust toolchain and run `cargo install --path crates/agentlens-cli` (and `--path crates/agentlens-mcp` for the MCP server).

## Next

[Getting started](getting-started.md) shows the first commands.
