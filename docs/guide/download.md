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

Unpack the archive and put `agentlens` and `agentlens-mcp` (the [MCP server](mcp-server.md)) on your `PATH`. Check the download against [`SHA256SUMS`](https://github.com/GravionLabs/agentlens/releases/latest/download/SHA256SUMS) from the same release:

```sh
sha256sum -c --ignore-missing SHA256SUMS
```

## From source

Install the Rust toolchain and run `cargo install --path crates/agentlens-cli` (and `--path crates/agentlens-mcp` for the MCP server).

## Next

[Getting started](getting-started.md) shows the first commands.
