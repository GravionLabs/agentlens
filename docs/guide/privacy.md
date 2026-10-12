# Privacy

AgentLens works on files that already exist on your machine and is built so that you can check what it does.

## What it reads

- The session files of the [sources](sources.md), the installed skills and agents, the instruction files and the names of configured MCP servers. MCP configuration files are read for server names only. The files of agents, commands and skills are searched for the names of unused skills; only the kind and name of the agent, command or skill that mentions one is kept (files over 1 MiB and symlinked folders are skipped).
- It reads the content of prompts and responses only as far as the analyses need their size or their metadata (token usage, tool names, outcomes, timestamps), and **no report contains a prompt, a response, a tool argument, a tool result or the content of a file**. A test of the MCP server runs a session full of marker texts through every tool and checks that none of them comes out.
- Paths of projects and instruction files, names of skills, tools and models do appear in reports.

## What it writes

- **Nothing in the client files.** Adapters are read-only, and SQLite databases are opened read-only.
- The files you ask for: a report (`agentlens report --html`), an [export](reports-and-export.md) from the application to the place you choose, and `--format json|csv` output on the standard output.
- A cache of token counts, only with [`--exact`](exact-tokens.md): `$XDG_CACHE_HOME/agentlens/count_tokens/`.
- The one exception, and only when you ask for it: `agentlens mcp-setup --write` registers the [MCP server](mcp-server.md) in a client's configuration. It asks first, makes a backup, changes only its own entry and can undo it.

## What leaves the machine

Nothing, with these opt-in exceptions:

| When                                   | What is sent, to whom                                                                                       |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `agentlens --exact …`                  | the text of your installed skills, to the Anthropic `count_tokens` API ([details](exact-tokens.md))          |
| **Check for updates** in the application | a request to the GitHub release page to ask for the latest version; no other call ([details](../updater.md)) |

There is no analytics, no background update check and no network use of the MCP server.

## Where your data stays

HTML reports and exports contain the numbers of your sessions, including project paths. They are files like any other: share them as far as you would share the numbers.
