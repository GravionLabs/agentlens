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

## Context hygiene

- `claude_code/clear_at_start.jsonl`: one session (`clear-1`) opened by a `/clear`, laid out as Claude Code writes it: a `file-history-snapshot` line, the meta caveat message, the `/clear` command message and its empty `local-command-stdout`, then six requests (1,000 to 6,000 tokens) with two compactions. Part of the output-contract home.
- `claude_code/two_compactions.jsonl`: the same six requests and two compactions without a `/clear` (`compact-2`). Part of the output-contract home.
- `claude_code/near_limit.jsonl`: three requests of `claude-test-1` growing from 10,000 to 190,000 tokens (95% of the documented 200,000 window) without a compaction (`near-1`). Part of the output-contract home.

## Delegation

- `claude_code/large_context_no_subagents.jsonl`: one session (`deleg-1`, whose id the test sees as the file name) with 20 main-agent requests of `claude-test-1`, each with more than 200,000 tokens of input, and no subagent request. Not part of the output-contract home; the Delegation tests lay it out themselves.
