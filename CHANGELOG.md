# Changelog

## 0.5.0

### Application

- New layout: a sidebar on the left holds navigation, filters, refresh and the update status; tables scroll on their own and keep their header; stat cards and rows are more compact.
- Every page has a one-line summary and an "Explain" link; a Glossary page explains the terms (overhead, baseline, carry and more), and table headers and explanations link to it.
- Faster loading on real data, refresh, readable tables, and checking for updates and installing them from the application.

### Exact token counts

- `--exact` (with `--exact-model`) counts text through the Anthropic `count_tokens` API instead of estimating it. It needs `ANTHROPIC_API_KEY`, caches results on disk and falls back to estimates with a warning. Instruction files and tool definitions stay estimated; the application always estimates.

### Sources

- OpenCode: sessions of the legacy JSON storage (before 1.2.0) are read next to the SQLite database, without counting a session twice.
- Copilot CLI and VS Code Copilot: `import-otel` reads the OpenTelemetry file exporter (measured tokens per request, premium requests, tool calls, user turns). The export replaces the local usage of the same moment, within 250 ms.

### Fixes

- A Claude Code session that appears under two project directories is counted once.

## 0.4.0

### Desktop application

- A Tauri 2 + Angular 22 application on `agentlens-core` (`pnpm tauri dev`, see the README): sources, skills, context, unused, flow (growth, errors, hooks), cost (with grouping and what-if prices), efficiency, compare and a timeline of single sessions, with shared filters (period, sources, project).
- The same numbers and marking as the CLI (`~` estimated, `n/a` not recorded); loading indicator, light and dark theme, app icon.
- CI checks the frontend and the application crate in a separate workflow; the CLI workflow needs no system libraries.

### Other changes

- An OpenTelemetry export now enriches the local copy of a session instead of losing against it: its measured requests (cost, skill attribution) replace the local usage in the range the export covers.
- MIT license.
- ADR 0003 records the decision for the desktop application.

### Not included

- Installers for the application (release binaries remain CLI only).
- Exact token counts through `count_tokens`, the legacy OpenCode JSON storage and OpenTelemetry traces of Copilot CLI and VS Code Copilot (milestone M5 Backlog).

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
