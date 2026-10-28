# AgentLens

Analyze the local session data of coding agents (Claude Code, OpenCode, GitHub Copilot CLI and Copilot Chat in VS Code) and answer questions such as:

- Which installed skills are actually used, and how many tokens do they cost?
- How much context do tool definitions, MCP servers and instruction files add to every request?
- What do sessions cost per project, branch or issue, and how do the clients compare?

AgentLens only reads files that the clients already write, and it shows for every number whether the client measured it or AgentLens estimated it (`~`). Nothing is sent anywhere unless you ask for it ([Privacy](docs/guide/privacy.md)). It comes as a command line (`agentlens`), a desktop application and an MCP server (`agentlens-mcp`) that lets an agent ask for the results itself.

## Documentation

The guide is a website at <https://gravionlabs.github.io/agentlens/> (with the application as a live demo on sample data) and lives in [`docs/guide`](docs/guide/README.md):

| Page                                                          | About                                                                      |
| ------------------------------------------------------------- | -------------------------------------------------------------------------- |
| [Download](docs/guide/download.md) | the application, the command line and the MCP server |
| [Getting started](docs/guide/getting-started.md)              | install, first commands, filters                                           |
| [Sources](docs/guide/sources.md)                              | which agents are read, where their files are, what each one reports        |
| [Measured and estimated](docs/guide/measured-and-estimated.md) | how to read a number                                                       |
| [The analyses](docs/guide/analyses.md)                        | skills, context, unused, flow, cost, efficiency, compare                   |
| [Reports and export](docs/guide/reports-and-export.md)        | JSON, CSV, the offline HTML report                                         |
| [The desktop application](docs/guide/application.md)          | views, filters, export, updates                                            |
| [The MCP server](docs/guide/mcp-server.md)                    | tools, and `agentlens mcp-setup` to register it in your agent              |
| [Exact token counts](docs/guide/exact-tokens.md)              | the opt-in `--exact` mode                                                  |
| [Glossary](docs/guide/glossary.md)                            | overhead, baseline, carry and the other terms                              |

## Installation

Download the archive for your platform (and the installer of the application) from the [latest release](https://github.com/GravionLabs/agentlens/releases/latest), unpack it and put `agentlens` and `agentlens-mcp` on your `PATH`. The [Download](docs/guide/download.md) page has direct links, how to verify them and what to do about the unsigned installers. To build from source: `cargo install --path crates/agentlens-cli` (and `--path crates/agentlens-mcp`).

## Agent skills

Run `apm install` before working in the repository to install the stack skills declared in `apm.yml`. The skill folders (`.claude/skills`, `.agents/skills`) are git-ignored, so a fresh clone has none. This needs [apm](https://github.com/microsoft/apm).

## Usage

```sh
agentlens scan                       # which sources exist, how many sessions
agentlens skills --since 30d         # skill usage and token cost
agentlens unused --days 30           # installed skills, agents and MCP servers nobody used
agentlens cost --since 30d           # cost per model, measured vs. computed
agentlens report --html report.html  # one self-contained page that works offline
agentlens mcp-setup                  # how to register the MCP server in your agent
```

Every command takes `--since`, `--until` (closes the period, for example `--since 2026-10-01 --until 2026-11-01`), `--source`, `--project` and `--format table|json|csv`; `agentlens --help` lists them all.

## Layout

| Path                    | Purpose                                                        |
| ----------------------- | -------------------------------------------------------------- |
| `crates/agentlens-core` | normalized event model, adapters, analyzers, the shared API    |
| `crates/agentlens-cli`  | the `agentlens` binary                                         |
| `crates/agentlens-mcp`  | the `agentlens-mcp` binary, the analyses as MCP tools          |
| `src-tauri`             | the desktop application (Tauri 2), a thin shell over the core  |
| `src`                   | the application frontend (Angular 22)                          |
| `site`, `docs`          | the documentation site (VitePress) and its Markdown            |

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --exclude agentlens-app --all-targets -- -D warnings
cargo test --workspace --exclude agentlens-app
pnpm lint && pnpm format:check && pnpm test && pnpm build   # application and site
pnpm tauri dev                                              # the application with live reload
pnpm docs:dev                                               # the documentation site
```

See [AGENTS.md](AGENTS.md) for the architecture and the working conventions.

## License

MIT, see [LICENSE](LICENSE).
