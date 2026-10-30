# Fixtures

Test data for the adapters, one directory per source (`claude_code/`, `opencode/`, `copilot_cli/`).

Rules:

- **Synthetic only.** Never commit real sessions: no real prompts, file contents, project paths, user names or secrets. Hand-write the records or scrub a real one completely.
- **No binary blobs.** Build SQLite fixtures in the test from SQL.
- Keep fixtures small; one file per scenario, named after what it shows (`skill_by_slash_command.jsonl`).
- Use `tests/support::TempHome` to lay a fixture out below a temp home directory.

## Retry loops

- `claude_code/retry_loop.jsonl`: one session (`loop-1`) in which `Bash` runs `cargo build --release` three times in a row: the first two calls fail, the third succeeds. Four requests with distinct token counts: the second and the third issued the retries (the first issued the first attempt, the fourth came after the loop). It is part of the output-contract home. Claude Code records no hook runs.
- `copilot_cli/retry_loop.jsonl`: one session (`cp-loop`) in which `bash` runs `npm run lint` three times and fails each time, with a failing `preToolUse` hook run between the first and the second call. The requests come from `session.usage_record` events (no store).
