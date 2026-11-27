# Lint changed files

`scripts/Format-Changed.ps1 -Lint` runs the linter on the changed files. It is the `-Lint` mode of the format runner: same file selection (`-Base`, extension grouping without `-Language`), same `{files}` handling, same `-WhatIf`. Lint is read-only and never combined with `-Check`.

```powershell
pwsh scripts/Format-Changed.ps1 -Lint                  # detect the languages from the changed files
pwsh scripts/Format-Changed.ps1 -Lint -Language py
pwsh scripts/Format-Changed.ps1 -Lint -Language ts -Base origin/main
```

Defaults (no `lint` key): ts eslint (`npx biome lint --no-errors-on-unmatched {files}` in a Biome project), ng eslint; with `.oxlintrc.json` at the git toplevel ts `npx --no-install oxlint --no-error-on-unmatched-pattern {files}`, ng the same plus `npx --no-install eslint {files}` on `.html` only; py `uv run ruff check {files}`, dotnet `dotnet format analyzers --verify-no-changes --include {files}`, rust `cargo clippy --all-targets -- -D warnings` (whole crate, no `{files}`), go `golangci-lint run {files}`. Once `stylelint.config.*` (or `.stylelintrc*`) is at the git toplevel, ts and ng also lint changed `.css`/`.scss` with `npx --no-install stylelint {files}` (ADR 0008). A tool that is missing or whose probe fails is `skipped: <tool> not configured`.

Override (`.agents/config.json`, key `lint`, schema `schema/config.schema.json`):

```json
{
  "lint": {
    "dotnet": {
      "extensions": [".cs"],
      "command": ["jb inspectcode MySolution.sln --no-build --include={files} -o=obj/inspect.sarif"]
    }
  }
}
```

- `lint: false` disables: `skipped: disabled`, exit 0.
- `command` is a string or an array run in order, the first non-zero exit stops the run and is the exit code. `extensions` absent: those of the built-in default of the language.
- In lint mode a command without `{files}` runs as it is; no files are appended.
- A configured tool that is missing is an error. A missing `command` key is an error.
- Success prints `linted: <n> files`.

`jb inspectcode` writes SARIF (`-o`, `--no-build`); whether it exits non-zero on findings is not verified here: when it does not, a second command in the array must fail on the report, or `code-review` reads the SARIF.
