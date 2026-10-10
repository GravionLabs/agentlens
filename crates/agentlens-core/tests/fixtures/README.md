# Fixtures

Test data for the adapters, one directory per source (`claude_code/`, `opencode/`, `copilot_cli/`).

Rules:

- **Synthetic only.** Never commit real sessions: no real prompts, file contents, project paths, user names or secrets. Hand-write the records or scrub a real one completely.
- **No binary blobs.** Build SQLite fixtures in the test from SQL.
- Keep fixtures small; one file per scenario, named after what it shows (`skill_by_slash_command.jsonl`).
- Use `tests/support::TempHome` to lay a fixture out below a temp home directory.
