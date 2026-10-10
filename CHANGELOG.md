# Changelog

## 0.3.0

First tagged release. It covers the milestones M0 to M3.

### Sources

- Claude Code (JSONL transcripts), OpenCode (SQLite), GitHub Copilot CLI (`events.jsonl` and `session-store.db`), GitHub Copilot Chat in VS Code and VS Code Insiders (`chatSessions`, including the `.jsonl` operation log).
- OpenTelemetry exports of Claude Code logs (`--otel FILE`, `agentlens import-otel`).

### Commands

- `scan`: which sources exist and how many sessions they hold.
- `skills`: skill usage and token cost (baseline, load, carry, follow-on; measured per-skill attribution from OpenTelemetry).
- `context`: static context per request (tools, MCP, agents, skills, instruction files).
- `unused`: installed skills, agents and MCP servers without use, with their idle cost.
- `flow`: context growth and compactions, tool errors and retry loops, hooks and permission prompts.
- `cost`: cost per model, project, branch or issue, with what-if prices.
- `efficiency`: cache hit rate and subagent share.
- `compare`: the clients side by side.
- `report --html`: a self-contained HTML report.

### Notes

- Every number is marked measured or estimated. Token counts of static content are estimated as characters / 4 (ADR 0001).
- Prices are bundled for Anthropic models only and can be extended (ADR 0002).
- Release binaries: Linux x86_64 and aarch64, macOS Apple silicon, Windows x86_64.

### Not included

- Exact token counts through the Anthropic `count_tokens` API (opt-in, planned).
- The legacy OpenCode JSON storage and OpenTelemetry traces of Copilot CLI and VS Code Copilot.
