# AgentLens

Analyze the local session data of coding agents — Claude Code, OpenCode and GitHub Copilot — and answer questions such as:

- Which installed skills are actually used, and how many tokens do they cost?
- How much context do tool definitions, MCP servers and instruction files add to every request?
- What do sessions cost per project, branch or issue, and how do the clients compare?

AgentLens only reads files that the clients already write. Nothing is sent anywhere.

## Status

Early development. The plan lives in the GitHub issues and the [AgentLens project](https://github.com/orgs/GravionLabs/projects/13); milestone M0 delivers `agentlens scan` and `agentlens skills` for Claude Code.

## Usage

```sh
cargo run -p agentlens-cli -- scan                       # which sources exist, how many sessions
cargo run -p agentlens-cli -- skills --since 30d         # skill usage and token cost
cargo run -p agentlens-cli -- skills --format json       # also: csv
```

Global flags: `--since <12h|30d|4w|2026-10-01>`, `--source <id>` (repeatable), `--project <dir>`, `--format table|json|csv`.

Token values with a leading `~` are estimated (see [ADR 0001](docs/adr/0001-token-estimation.md)); values without it were reported by the client. In the skills table:

| Column | Meaning |
|---|---|
| load | tokens the skill content added to the context when it was loaded |
| carry | the loaded content sent again with every later request until the context is compacted |
| baseline | name and description of the skill, sent with every request of the clients that list it, used or not |
| overhead | baseline + load + carry |

Supported sources today: Claude Code, OpenCode (SQLite database) and GitHub Copilot CLI. Copilot CLI sessions without per-request rows in `session-store.db` fall back to the session totals from `session.shutdown`, which cannot be split per request.

## Layout

| Path | Purpose |
|---|---|
| `crates/agentlens-core` | normalized event model, adapters, analyzers |
| `crates/agentlens-cli` | the `agentlens` binary |

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

See [AGENTS.md](AGENTS.md) for the architecture and the working conventions.
