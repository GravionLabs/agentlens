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

| Flag                | Meaning                                                                                                            |
| ------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `--since <WHEN>`    | Only sessions active since a point in time: a duration (`12h`, `30d`, `4w`) or a date (`2026-10-01`).              |
| `--until <WHEN>`    | Only sessions that ended before this point, same forms as `--since`; with `--since` the period is closed.          |
| `--source <ID>`     | Only this source (repeatable): `claude-code`, `opencode`, `copilot-cli`, `vscode-copilot`.                         |
| `--project <DIR>`   | Only sessions that ran in this project directory or below it, including its git worktrees (see below).             |
| `--format <FORMAT>` | `table` (default), `json` or `csv`; see [Reports and export](reports-and-export.md).                               |
| `--otel <FILE>`     | Add an [OpenTelemetry export](../telemetry.md) to the analysis (repeatable).                                       |
| `--rounded`         | Token counts in text tables as `1.55 M`, `2.11 k` or `3.20 B` instead of the full number; JSON and CSV stay exact. |

A period is `[since, until)`: a session counts when it ended at or after `--since` and before `--until`. A duration counts back from now, a date is midnight UTC, and an RFC 3339 instant is taken as given. `--until` must be later than `--since`. For example, `agentlens cost --since 2026-10-01 --until 2026-11-01` is the cost of October.

### Git worktrees

A session that ran in a git worktree (for example `<repo>/.worktrees/<name>`) belongs to the main repository: AgentLens reads the worktree's `.git` file and its `commondir`, read-only, and uses the repository as the project. `--project <repo>` therefore finds the sessions of its worktrees, and `cost --by project` has one row per repository. If the worktree is already removed, the path is cut at its `.worktrees` segment; any other path stays as it is.

## Reading the numbers

A leading `~` marks an estimate; a value without it was reported by the client; `n/a` means the client does not record it. [Measured and estimated](measured-and-estimated.md) explains the rule, and the [analyses](analyses.md) explain the columns.
