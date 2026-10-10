# AgentLens

Analyze the local session data of coding agents — Claude Code, OpenCode and GitHub Copilot — and answer questions such as:

- Which installed skills are actually used, and how many tokens do they cost?
- How much context do tool definitions, MCP servers and instruction files add to every request?
- What do sessions cost per project, branch or issue, and how do the clients compare?

AgentLens only reads files that the clients already write. Nothing is sent anywhere.

## Status

Early development. The plan lives in the GitHub issues and the [AgentLens project](https://github.com/orgs/GravionLabs/projects/13); milestone M0 delivers `agentlens scan` and `agentlens skills` for Claude Code.

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
