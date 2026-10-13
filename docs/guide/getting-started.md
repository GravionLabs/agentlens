# Getting started

## Install

[Download](download.md) the command line (and the MCP server) or the desktop application for your platform, unpack the archive and put `agentlens` on your `PATH`.

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
| `--rounded`        | Token counts in text tables as `1.55 M`, `2.11 k` or `3.20 B` instead of the full number; JSON and CSV stay exact. |

## Reading the numbers

A leading `~` marks an estimate; a value without it was reported by the client; `n/a` means the client does not record it. [Measured and estimated](measured-and-estimated.md) explains the rule, and the [analyses](analyses.md) explain the columns.
