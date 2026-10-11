# Telemetry export for AgentLens

AgentLens reads the local session files of each client directly. OpenTelemetry (OTel) is an optional second source: it adds what the local files do not hold, such as the cost Claude Code computed per request and the skill each request is attributed to.

## What AgentLens imports

`agentlens import-otel FILE` reports what an export holds, and `--otel FILE` (repeatable, works with every command) adds it to an analysis.

| Client | Imported | Notes |
|---|---|---|
| Claude Code | Log events `claude_code.user_prompt`, `claude_code.api_request`, `claude_code.skill_activated`, `claude_code.tool_result` | Measured tokens and cost per request; `skill.name` of a request gives measured per-skill attribution (the `attributed` column of `agentlens skills`). |
| Copilot CLI | Spans of the file exporter: `chat` (one model request with its tokens), `execute_tool`, `invoke_agent` (user turns) | Measured tokens per request, premium requests (`github.copilot.cost`) and subagent nesting. See [Copilot CLI](#copilot-cli). |
| VS Code Copilot | The same spans as Copilot CLI | VS Code uses the same agent host, so the file has the same schema. See [VS Code](#vs-code). |

Not imported: metrics (`claude_code.token.usage`, the `gen_ai.client.*` metrics of Copilot and others) are aggregates without a request or timestamp. Prompts, responses and tool arguments are never read.

A session that is in the local Claude Code or Copilot CLI data and in an export (same `session.id`, for Copilot CLI `gen_ai.conversation.id`) is counted once. The export's requests (with the cost and skill attribution Claude Code measured) replace the local usage inside the time range the export covers; local requests outside that range stay, so a partial export does not undercount. Tool calls, skills, compactions, context items, project and branch come from the local copy.

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

## Copilot CLI

Copilot CLI writes JSON lines itself, no collector is needed:

```sh
mkdir -p /tmp/copilot-otel
COPILOT_OTEL_FILE_EXPORTER_PATH=/tmp/copilot-otel/trace.jsonl copilot
agentlens import-otel /tmp/copilot-otel/trace.jsonl
agentlens --otel /tmp/copilot-otel/trace.jsonl --source copilot-cli compare
```

Setting the path switches OpenTelemetry on and selects the file exporter. `copilot help monitoring` lists all variables. Message content (`OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT`) is off by default and not needed.

Each line is a `span` or a `metric`. AgentLens reads the spans:

| Span | Becomes |
|---|---|
| `chat <model>` | A request with measured tokens. `gen_ai.usage.input_tokens` includes the cache read and write tokens, so the uncached input is the difference. `github.copilot.cost` is the number of premium requests. |
| `execute_tool <name>` | A tool call. A span status of 0 means unset, so the call is `unknown`, never a success; status 2 is an error. |
| `invoke_agent` | The `github.copilot.user.message` events become user turns. An `invoke_agent` nested inside another one marks its requests as subagent requests (inferred from the nesting; not yet seen in a real export). |

Recent Copilot CLI versions also keep these numbers in their local `session-store.db`, and AgentLens reads them from there. The export then changes nothing: the request of the export replaces the local one of the same moment (the two differ by a few milliseconds, so the match allows 250 ms). It adds requests for sessions the store holds no rows for, and sessions that are only in the export.


## VS Code

The Copilot agent in VS Code writes the same span schema as Copilot CLI. Switch the file exporter on in the settings (or start VS Code with the `COPILOT_OTEL_*` variables from a terminal), reload the window and chat:

```json
{
  "github.copilot.chat.otel.enabled": true,
  "github.copilot.chat.otel.exporterType": "file",
  "github.copilot.chat.otel.outfile": "/tmp/copilot-otel/vscode.jsonl"
}
```

```sh
agentlens import-otel /tmp/copilot-otel/vscode.jsonl
```

The exporter buffers: the file stays empty until a few spans are flushed, so close the window if nothing shows up. Next to the `chat`, `execute_tool` and `invoke_agent` spans the file holds many host spans (`github.copilot.session.timing.*`, `vscode.agent_host.*`) and lines without a `type`. They carry no tokens and are reported as "other spans" and "unknown line", which is expected. The sessions are `copilot-cli` sessions, the same as the CLI's.
