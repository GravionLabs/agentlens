# The MCP server

`agentlens-mcp` lets a coding agent ask AgentLens itself, for example "which skills cost the most and which are never used", instead of you copying CLI output into a chat. It is a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio: the client starts the process, nothing listens on a port and nothing leaves the machine. It only reads the same client files as the CLI and writes nothing.

## Tools

| Tool                   | What it returns                                                                                   |
| ---------------------- | ------------------------------------------------------------------------------------------------- |
| `sources`              | which agents left sessions on this machine, and how many                                          |
| `skills`               | uses, load, carry, baseline and overhead tokens per skill                                         |
| `advice`               | what to change to save tokens: unused skills, costly descriptions, large contexts, with the numbers behind each finding |
| `context`              | static context: tool listings, MCP instructions, instruction files, agent and skill listings     |
| `unused`               | installed skills, agents and MCP servers nobody used, and what they cost while idle               |
| `flow`                 | context growth and compactions, tool errors and retry loops, hooks and permission waits           |
| `cost`                 | USD per model, project, branch, issue or session, measured and estimated apart; `what_if` compares a model |
| `efficiency`           | prompt cache hit rate and the subagents' share                                                    |
| `compare`              | the agents side by side over the same sessions                                                    |
| `sessions`, `timeline` | the sessions in a period, and the context size of every request of one                            |

Every tool takes the filter of the CLI (`since`, `until`, `sources`, `project`, `session`). `cost`, `efficiency`, `flow` and `compare` also take `role`, a list of roles (keys of the `roles` table of `efficiency`: forked skill name, else workflow phase, else agent type): only the sessions that started a run of one of them, with all their requests; a session without subagent runs never matches. All tools return `{ schema_version, data, warnings, errors }` as the application shows it. All tools are marked read-only. `since` and `until` take a duration counted back from now (`30d`, `12h`, `4w`), a date (midnight UTC) or an RFC 3339 instant; the period is `[since, until)`, so `until` must be later than `since`. Estimates stay marked: a count that can be an estimate has a `measured` flag ([Measured and estimated](measured-and-estimated.md)), and what the tools, the prompt and the fields promise is on the [Compatibility](compatibility.md) page. The results contain counts, names and paths, never prompts, responses, tool arguments or file contents ([Privacy](privacy.md)).

## A prompt and a glossary

Besides the tools the server offers two things that make an analysis easier for an agent:

- **The prompt `optimize-skills`** tells the agent how to analyze a skill setup: call `advice` first, read the glossary where a field is unclear, look closer with the other tools, trust measured numbers and say when a conclusion rests on an estimate, call a skill unused only if it was neither loaded nor used through its files, and change nothing without asking. It takes an optional `since` (default `30d`). Clients that list prompts show it as a command, in Claude Code as `/mcp__agentlens__optimize-skills`.
- **The resource `agentlens://glossary`** is the [glossary](glossary.md) in Markdown, made from the same source as the application's Glossary view, so a field name in a result (`load`, `carry`, `baseline`, `follow_on`, `file_uses`) can be looked up where the data is instead of guessed. The tool descriptions point to it.

## Registering it in a client

```sh
agentlens mcp-setup                 # prints what to add for each client, changes nothing
agentlens mcp-setup --write         # registers it in the clients found on this machine
agentlens mcp-setup --remove        # takes the entry out again
```

`--write` asks for each client first (`--yes` skips the question), makes a backup copy of a configuration file before it changes it (`<file>.agentlens-backup-<time>`), adds only its own entry and keeps everything else, and does nothing when the entry is already there. A file it cannot read as JSON (VS Code's `mcp.json` may contain comments) is left untouched with a warning. Claude Code is registered with its own `claude mcp add`, which `--write` runs for you. **This is the only command of AgentLens that writes into files of the clients.** `--client claude-code|vscode|copilot-cli|opencode` chooses clients, and `--binary FILE` names the server if it is neither next to `agentlens` nor on `PATH`. `agentlens mcp-setup` looks for `agentlens-mcp` in the folder of the `agentlens` executable first and then on `PATH`, so it finds the server in an archive folder, in `/usr/bin` after a `.deb` or `.rpm`, next to `agentlens` in an installer's files, and in `~/.local/bin` after the application's *Install command line tools* action, when that folder is on `PATH` ([Download](download.md)).

To do it by hand, register the server in your client with the full path to the binary:

| Client      | Registration                                                                                                                                      |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `claude mcp add agentlens -- /path/to/agentlens-mcp` (add `--scope user` for all projects)                                                         |
| VS Code     | `.vscode/mcp.json` or the user `mcp.json`: `{ "servers": { "agentlens": { "type": "stdio", "command": "/path/to/agentlens-mcp" } } }`             |
| Copilot CLI | `~/.copilot/mcp-config.json`: `{ "mcpServers": { "agentlens": { "type": "local", "command": "/path/to/agentlens-mcp", "args": [], "tools": ["*"] } } }` |
| OpenCode    | `opencode.json`: `{ "mcp": { "agentlens": { "type": "local", "command": ["/path/to/agentlens-mcp"], "enabled": true } } }`                           |

The formats are the ones the clients document at the time of writing; if your version differs, the client's own documentation wins. `agentlens-mcp --version` prints the version.
