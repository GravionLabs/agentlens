# Run the tests

`scripts/Invoke-Tests.ps1` runs the tests with the command the project names. It never guesses a test tool (seen 2026-11-09: `npx jest` in a vitest repository).

```powershell
pwsh scripts/Invoke-Tests.ps1                                   # every language found in the repository root
pwsh scripts/Invoke-Tests.ps1 -Language ts                      # the full command
pwsh scripts/Invoke-Tests.ps1 -Language ts -Targeted -Files a.test.ts,b.test.ts
pwsh scripts/Invoke-Tests.ps1 -WhatIf                           # the resolved plan as JSON, runs nothing
```

Precedence per language: the `test.<lang>` entry of `.agents/config.json`, then `scripts.test` of `package.json` (ts, ng: `npm test`), then the default of the detected tool: vitest `npx vitest run`, jest `npx jest` (the tool comes from `package.json` dependencies or `vitest.config.*` / `jest.config.*`; the `tools` of `detect-techstack` report the same), py `uv run pytest`, rust `cargo test`, go `go test ./...`, dotnet `dotnet test`. Nothing configured and no tool found: exit 1 with the hint to set the key; no guess.

`-Targeted` with `-Files` runs the `targeted` command (`{files}` replaced); the tool defaults have one for vitest, jest and pytest. Without `-Files` or without such a command the full command runs, so use the `<lang>-test` skill's targeted mapping for the rest.

Override (`.agents/config.json`, key `test`, schema `schema/config.schema.json`): `false` disables the runner (`skipped: disabled`), or a map keyed by language with `command` (string, or array run in order; the first non-zero exit stops and is the exit code) and optional `targeted`.

```json
{ "test": { "ts": { "command": "npx vitest run", "targeted": "npx vitest run {files}" } } }
```

`/project-setup` offers the key per detected language and writes only non-default choices.
