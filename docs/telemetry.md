# Telemetry export for AgentLens

AgentLens reads the local session files of each client directly. OpenTelemetry (OTel) is an optional second source: it adds what the local files do not hold, such as the cost Claude Code computed per request and the skill each request is attributed to.

## What AgentLens imports

`agentlens import-otel FILE` reports what an export holds, and `--otel FILE` (repeatable, works with every command) adds it to an analysis.

| Client | Imported | Notes |
|---|---|---|
| Claude Code | Log events `claude_code.user_prompt`, `claude_code.api_request`, `claude_code.skill_activated`, `claude_code.tool_result` | Measured tokens and cost per request; `skill.name` of a request gives measured per-skill attribution (the `attributed` column of `agentlens skills`). |
| Copilot CLI, VS Code Copilot | Not yet | Their trace file formats are not documented. AgentLens reads their local session files instead. See the follow-up issue for traces. |

Not imported: metrics (`claude_code.token.usage` and others) are aggregates without a request or timestamp, and traces.

A session that is in the local Claude Code data and in an export (same `session.id`) is counted once, and the local copy is used.

## Claude Code

Claude Code has no file exporter. It sends OTLP to a collector, and the collector writes the file.

1. Enable telemetry for the logs signal:

   ```sh
   export CLAUDE_CODE_ENABLE_TELEMETRY=1
   export OTEL_LOGS_EXPORTER=otlp
   export OTEL_EXPORTER_OTLP_PROTOCOL=grpc
   export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
   export OTEL_LOGS_EXPORT_INTERVAL=5000
   ```

2. Optional: `export OTEL_LOG_TOOL_DETAILS=1` makes skill names appear verbatim. Without it, user-defined and third-party plugin skills are reported as `custom_skill` or `third-party`, and AgentLens attributes usage to those placeholder names.

3. Run an OpenTelemetry Collector (the contrib distribution contains the `file` exporter) with this configuration:

   ```yaml
   receivers:
     otlp:
       protocols:
         grpc:
           endpoint: 127.0.0.1:4317
   exporters:
     file/claude:
       path: ./claude-code-logs.jsonl
   service:
     pipelines:
       logs:
         receivers: [otlp]
         exporters: [file/claude]
   ```

   The file exporter writes one OTLP JSON export per line, which is what AgentLens reads.

4. Analyze:

   ```sh
   agentlens import-otel claude-code-logs.jsonl
   agentlens skills --otel claude-code-logs.jsonl --since 30d
   ```

## Privacy

- Prompts are redacted by Claude Code unless `OTEL_LOG_USER_PROMPTS=1` is set. AgentLens never stores prompt text, tool parameters or commands, even if the export contains them; it keeps counts, sizes, names and timings only.
- With `OTEL_LOG_TOOL_DETAILS=1` the export contains commands and tool input. Keep such files private.
- The collector configuration above listens on localhost only.

## Sources

The attribute names come from the Claude Code monitoring documentation (https://code.claude.com/docs/en/monitoring-usage).
