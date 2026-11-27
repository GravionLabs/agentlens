# Gate: `check:changed`

One command checks the change of a branch before the pull request. Its name is `check:changed` in every repository; the `gate` key of `.agents/config.json` records the command line (string) or `false` (no gate).

## What it runs

In this order, on the files changed against the base only:

1. Format check (no write).
2. Lint.
3. Typecheck or build.
4. The tests that import the change (not the full suite).
5. The ratchets of the `checks` key of `.agents/config.json`.

## Output rules

- A failure is printed in full (tool output, not a summary), per step.
- Success prints nothing.
- Exit code: 0 all steps passed, non-zero otherwise. The first failing step does not hide the later ones when they are cheap; the exit code is non-zero if any failed.

## Pass cache

A pass is cached by the hash of the changed files plus the base ref. An unchanged tree is not checked again. Any edit, or another base, invalidates it.

## Options and limits

- `--base=<ref>`: the ref the changed files are computed against. Default: the base of the pull request; stacked work passes the parent branch.
- Worker limit: `min(4, cores / 2)` parallel processes; `GATE_MAX_WORKERS` overrides it.

## Use

- Run it once when the work is done (not after every edit; the fast `checks` cover the loop).
- `scripts/Invoke-Gate.ps1` reads the `gate` key, runs the command in the repository root, prints its output and returns its exit code; without `gate` it prints `skipped: no gate configured` and exits 0; `-WhatIf` prints the command. Skills call it instead of parsing the config.
- Never skip it and never commit with `--no-verify`.
- Run it in the foreground with a timeout; a timeout is a failure, not a pass.

## Form per stack

- TypeScript: copy `templates/check-changed.mts` to the repository's own scripts folder (Node >= 22.18), wired as `pnpm check:changed`. It runs the steps above with oxfmt/biome/prettier, oxlint/biome/eslint, `tsc --noEmit`, `vitest related` (jest `--findRelatedTests`) with `--maxWorkers`, then the `checks` entries; success is one line, a failed step prints its full output only. A pass is cached under `<git-common-dir>/check-changed/` (key: content of the changed files, base commit, step list; a hit prints `cached pass`); `--base=<ref>` sets the base (default: `base` of `.agents/config.json`, else `origin/HEAD`), `--no-cache` ignores the cache.
- Other stacks get no script from these skills; wire the commands below as a repository script, a `just` recipe or a `cargo` alias named `check:changed`.

| Language | Format                              | Lint                                                                                                     | Typecheck / build               | Tests                    |
| :------- | :---------------------------------- | :------------------------------------------------------------------------------------------------------- | :------------------------------ | :----------------------- |
| .NET     | `dotnet format --verify-no-changes` | `dotnet format analyzers --verify-no-changes`                                                            | `dotnet build --no-restore`     | `dotnet test --no-build` |
| Rust     | `cargo fmt --check`                 | `cargo clippy --all-targets --all-features -- -D warnings`, `cargo deny check` (when `deny.toml` exists) | `cargo check` (clippy compiles) | `cargo test`             |
| Python   | `ruff format --check`               | `ruff check`                                                                                             | `pyright` or `mypy`             | `pytest`                 |
| Go       | `gofmt -l .`                        | `golangci-lint run`                                                                                      | `go build ./...`                | `go test ./...`          |

## Command per stack

| Stack               | Gate command          |
| :------------------ | :-------------------- |
| TypeScript, Angular | `pnpm check:changed`  |
| .NET, Python, Go    | `just check-changed`  |
| Rust                | `cargo check-changed` |

The language implementation skills name these.

Rust: after the migration of `allow` to `expect`, every suppression is an `#[expect(` and the lint-suppressions ratchet (`check-lint-suppressions-ratchet.mts`) counts them as it counts `#[allow(`.

## Claude Code Stop hook

`templates/claude-gate-hook.json` is a `Stop` hook that runs the gate (through `scripts/Invoke-Gate.ps1`) when the agent stops. It replaces a husky pre-commit hook for agent sessions. `Stop`, not `PostToolUse`: the gate runs once when the work is done, not after every edit.

- A red gate exits with code 2 and prints the gate output to stderr; Claude Code feeds it back to the agent, which fixes it.
- The hook input carries `stop_hook_active`; when it is `true` the stop was already blocked once, so the hook exits 0 and does not block again (no endless loop).
- Without a `gate` key `Invoke-Gate.ps1` skips and the hook passes.

Merge it by hand into `.claude/settings.json`: add the `Stop` entry to the `hooks` object, keep existing `Stop` entries, and adjust the script path when the skill is installed elsewhere (global: `~/.claude/skills/implementation/scripts/Invoke-Gate.ps1`). `.claude/` is a protected path (`deliver` does not merge changes to it), so the merge is a manual step, never done by a skill or a delivery session.

Copilot CLI and OpenCode have no equivalent hook; there the agent runs the gate itself, as `AGENTS.md` tells it (`templates/agents-checks.md`).
