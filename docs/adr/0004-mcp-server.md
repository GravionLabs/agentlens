# 0004: MCP server

- Status: Accepted (decided by the owner on 2026-10-11)
- Context: Epic "MCP server" (#202), PBIs #204 to #209 and #211

## Context

AgentLens answers questions about the token use of coding agents (which skills cost what, where the context grows, what is unused). Today a person runs the CLI or opens the application and carries the output to an agent by hand. An agent that could ask AgentLens directly could analyze the results itself and propose changes (remove unused skills, shrink instruction files), with the measured/estimated marking intact.

The analyses are already plain functions over normalized events (`agentlens-core`), and the desktop application wraps them as commands (`src-tauri/src/api.rs`). A server is a third thin shell over the same functions.

## Options

1. **A subcommand `agentlens mcp`.** One binary, but every user of the CLI carries the server code and its dependencies.
2. **A separate binary `agentlens-mcp` in its own crate.** Its own dependency set (an async runtime, the MCP SDK), packaged next to the CLI.
3. **An HTTP server.** Useful for remote clients, but opens a port and needs authentication; nothing asks for it.

## Decision

- **Option 2**, decided by the owner. The crate `agentlens-mcp` joins the workspace; the CLI and the core stay free of the async runtime and the MCP SDK.
- **Transport: stdio.** The client starts the process; nothing listens on a port and no network is used.
- **Read-only.** One tool per analysis (`skills`, `context`, `unused`, `flow`, `cost`, `efficiency`, `compare`, `sessions`, `timeline`, `sources`), each taking the filter of the CLI (period, sources, project) and the options of its view, and returning the report type of the core as JSON with the same `measured` flags and warnings the CLI prints. Tools never write, never delete and never call out. `--exact` (the only network feature) is not offered.
- **Shared entry points.** The functions that `src-tauri/src/api.rs` holds move into `agentlens-core` (PBI #205), so the application, the server and the CLI call the same code; a change in an analysis shows up everywhere.
- **Privacy.** Reports contain counts, names of skills, tools and projects, and paths, never prompts, responses or file contents. A test keeps it that way (PBI #207). Project paths are what the CLI shows already.
- **SDK.** The official Rust SDK of the Model Context Protocol (`rmcp`), checked for its current version and maturity when #206 starts; if it is unsuitable, the tools are implemented against the JSON-RPC protocol directly.
- **Setup help (PBI #211).** `agentlens mcp-setup` prints, for each installed client (Claude Code, VS Code, Copilot CLI, OpenCode), the exact configuration or command to register the server. Printing is the default. `--write` registers it in the client's MCP configuration: it asks per client, backs the file up, merges instead of overwriting, is idempotent and has `--remove`. **This is the only place where AgentLens writes into client files**, an exception to "read-only" that is opt-in, visible and reversible; a malformed configuration is left untouched with a warning.
- **Release.** The binary is built and packaged with the CLI in the release workflow (PBI #208). Planned for release 0.7.

## Consequences

- Agents can answer "what should I remove to save tokens" from measured data, without a person copying output.
- The refactor of #205 touches the application's API layer; its tests (`src-tauri/tests/api.rs`) move with it.
- A new dependency family (async runtime, SDK) lives only in one crate and is packaged as a second binary, which the release workflow and the installers' documentation must mention.
- Tool results are data for the agent, not instructions; the descriptions of the tools say so and stay free of text that reads like instructions.
- The exception for `mcp-setup --write` is documented here and in AGENTS.md.
