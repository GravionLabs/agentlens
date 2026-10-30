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

  What ties a Claude Code run to its caller is the `agent-<id>.meta.json` next to each subagent transcript. AgentLens reads only its `agentType`, `name` (the skill of a forked skill), `description`, `model`, `parentAgentId`, `spawnDepth`, `toolUseId` and `workflowPhase`, never the worktree path or a prompt. A run with `spawnDepth` 1 and no parent was called by the main session; an older meta file without those keys counts as depth 1 when the transcript sits directly in `subagents/`. A workflow run (`subagents/workflows/<run>/`) is a run of its own at depth 1, and its subagents are at depth 2 under it. A missing or unreadable meta file gives a warning and a run whose caller is unknown, never a guess.

- **OpenCode** reports tokens per request and the cost it computed itself (shown as is).

  What ties an OpenCode run to its caller is `parent_id` in the `session` table: a session with a parent is a run of that parent. Its depth is the length of the `parent_id` chain (1 when the main session called it), and its start and end are the `time_created` and `time_updated` of the session. The requests of that session belong to the run. The agent type is the `agent` of the first user message of the session, when it has one. A parent chain that loops yields no run and no depth.

- **Copilot CLI** reports tokens and premium requests per request from `session-store.db`; a session without rows in the store is read from the `session.usage_record` events of newer versions (one per request, measured), and failing that from the session totals, which cannot be split per request. When both the store and the records exist, the store is used and the records are not counted again. The records carry no subagent marker as far as seen, so their requests count as main-agent requests. It also records hooks and permission prompts.

  What ties a Copilot CLI run to its caller is the `subagent.started`, `subagent.completed` and `subagent.failed` events: one run per `toolCallId`, with `agentName` as the agent type, `agentDisplayName` as the description and the event times as start and end (a run that never completes has no end). Without a named caller the main session started it (depth 1); a `parentToolCallId` that names another run nests it there, and one that names no run leaves the caller unknown. In the store, a row with a `parent_tool_call_id` belongs to the run with that id.

- **OpenTelemetry** exports of the Copilot CLI tie a run to its caller through the span tree: an `invoke_agent` span nested in another one is a run, its depth is the number of agent spans above it, its parent is the nearest agent span above that is a run itself (none when the main agent called it), and the `gen_ai.tool.call.id` of a tool span directly above it is the calling tool call. Requests (`chat` spans) inside it belong to the run.

- **VS Code Copilot** reports prompt and completion tokens per request without a cache split, plus the share of system instructions and tool definitions in the prompt, which `agentlens context` uses as reported token counts.

A value a client does not record is `n/a`, never 0.

## Adding an OpenTelemetry export

An [export](../telemetry.md) adds what the local files do not hold, such as the cost Claude Code computed per request and the skill each request is attributed to. A session that is in the local data and in an export is counted once.
