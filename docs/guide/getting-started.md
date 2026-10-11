# Getting started

## Install

Every push to `main` publishes a release. Download the archive for your platform from the [latest release](https://github.com/GravionLabs/agentlens/releases/latest), unpack it and put `agentlens` and `agentlens-mcp` (the [MCP server](mcp-server.md)) on your `PATH`:

| Platform               | Archive                                                                                    |
| ---------------------- | ------------------------------------------------------------------------------------------ |
| Linux x86_64 / aarch64 | `agentlens-x86_64-unknown-linux-gnu.tar.gz` / `agentlens-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple silicon    | `agentlens-aarch64-apple-darwin.tar.gz`                                                    |
| Windows x86_64         | `agentlens-x86_64-pc-windows-msvc.zip`                                                     |

Verify the download against `SHA256SUMS` from the same release (`sha256sum -c --ignore-missing SHA256SUMS`).

The [desktop application](application.md) comes as installers in the same release: a `.deb`, `.rpm` or AppImage for Linux x86_64, a `.dmg` for macOS on Apple silicon and an `.msi` or setup `.exe` for Windows x86_64. They are not signed yet, so the systems warn on first start: on macOS open the app with a right click and choose Open (or allow it in System Settings, Privacy and Security), on Windows choose More info and Run anyway in SmartScreen, and make an AppImage executable (`chmod +x`) before running it.

To build from source, install the Rust toolchain and run `cargo install --path crates/agentlens-cli` (and `--path crates/agentlens-mcp` for the MCP server).

## First commands

```sh
agentlens scan                       # which sources exist, how many sessions
agentlens skills --since 30d         # skill usage and token cost
agentlens unused --days 30           # installed skills, agents and MCP servers nobody used
agentlens cost --since 30d           # cost per model, measured vs. computed
agentlens report --html report.html  # one self-contained page that works offline
```

`scan` comes first: it tells you which of the four [sources](sources.md) AgentLens found and how many sessions it read. Everything else works on those sessions.

## Narrowing down

Every command takes the same filters:

| Flag               | Meaning                                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------------------------ |
| `--since <WHEN>`   | Only sessions active since a point in time: a duration (`12h`, `30d`, `4w`) or a date (`2026-10-01`).        |
| `--source <ID>`    | Only this source (repeatable): `claude-code`, `opencode`, `copilot-cli`, `vscode-copilot`.                   |
| `--project <DIR>`  | Only sessions that ran in this project directory or below it.                                                |
| `--format <FORMAT>` | `table` (default), `json` or `csv`; see [Reports and export](reports-and-export.md).                        |
| `--otel <FILE>`    | Add an [OpenTelemetry export](../telemetry.md) to the analysis (repeatable).                                 |

## Reading the numbers

A leading `~` marks an estimate; a value without it was reported by the client; `n/a` means the client does not record it. [Measured and estimated](measured-and-estimated.md) explains the rule, and the [analyses](analyses.md) explain the columns.
