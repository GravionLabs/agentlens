# Sources

AgentLens reads the session files that the coding agents write themselves. It never writes to them (SQLite files are opened read-only), and a record it does not understand produces a warning and is skipped, never a failure.

| Source ID        | Agent                      | Where it reads                                                                                                                                    |
| ---------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `claude-code`    | Claude Code                | the session files under `~/.claude/projects` (or `$CLAUDE_CONFIG_DIR`)                                                                            |
| `opencode`       | OpenCode                   | the SQLite database under `~/.local/share/opencode`, and the JSON files of versions before 1.2.0 under `storage/`; a session in both is counted once |
| `copilot-cli`    | GitHub Copilot CLI         | `~/.copilot/session-state` and the `session-store.db` next to it (or `$COPILOT_HOME`)                                                             |
| `vscode-copilot` | GitHub Copilot Chat in VS Code and VS Code Insiders | the `chatSessions` files in the user directory, including the `.jsonl` operation log                                              |

`agentlens scan` shows what was found and what it holds, with a reason where a source was not found.

## What each source reports

The clients differ in what they record, and AgentLens shows the difference instead of filling gaps:

- **Claude Code** reports tokens per request with the cache split, the static context the client adds to every request, tool calls with their outcome, subagents, compactions and the git branch. It is the only source that reports the static context, so `agentlens context` and the idle cost of `unused` depend on it.
- **OpenCode** reports tokens per request and the cost it computed itself (shown as is).
- **Copilot CLI** reports tokens and premium requests per request from `session-store.db`; a session without rows in the store is read from the `session.usage_record` events of newer versions (one per request, measured), and failing that from the session totals, which cannot be split per request. When both the store and the records exist, the store is used and the records are not counted again. The records carry no subagent marker as far as seen, so their requests count as main-agent requests. It also records hooks and permission prompts.
- **VS Code Copilot** reports prompt and completion tokens per request without a cache split, plus the share of system instructions and tool definitions in the prompt, which `agentlens context` uses as reported token counts.

A value a client does not record is `n/a`, never 0.

## Adding an OpenTelemetry export

An [export](../telemetry.md) adds what the local files do not hold, such as the cost Claude Code computed per request and the skill each request is attributed to. A session that is in the local data and in an export is counted once.
