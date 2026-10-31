# Loop: a fresh session per PBI

```powershell
pwsh <skill-dir>/scripts/Invoke-DeliverLoop.ps1 -Milestone 0.13 -Max 5
pwsh <skill-dir>/scripts/Invoke-DeliverLoop.ps1 -Target 860,870 -Model opus
```

Run it from the repository root. It starts `claude -p "/deliver <targets> --unattended --max 1" --model <model> --permission-mode auto --output-format stream-json --verbose` once per PBI (`-Model` default `sonnet`, `-Harness claude` is the only harness). The allowlist is `references/permissions.md`; the script checks nothing more.

## Loop or one session

- One `/deliver` session: a few PBIs, you watch, interactive questions are welcome.
- The loop: a whole release or a Feature overnight. Each PBI starts with an empty context, a dead session (sleep, crash, rate limit) costs one PBI, and `deliver` keeps all state in GitHub, so there is no queue file.

## Stop rules

Checked from GitHub (not from the output) before and after each iteration, on the open PBIs of the targets without `needs-decision` / `needs-refinement` / `interactive` (unattended, `deliver` skips `interactive` PBIs; they count as held):

| Result      | When                                                                                | Exit |
| :---------- | :---------------------------------------------------------------------------------- | :--- |
| done        | no open PBI left                                                                    | 0    |
| max reached | `-Max` iterations ran (delivered, blocked or paused PBIs in total)                  | 0    |
| blocked     | open PBIs remain, all wait on an open blocker or a label                            | 2    |
| no progress | the set did not change after an iteration; prints the last log lines, never retries | 1    |

## Observability

- `.agents/deliver-loop/<run>/<nr>.jsonl`: every stream record as it arrives (git-ignored). `<nr>` is the first ready PBI at the start of the iteration.
- `.agents/deliver-loop.log` and stdout: `HH:MM:SS <tool> <description or file>`, short assistant text, tool errors with `✗`; subagent steps carry the agent description as prefix (`[Review PBI #401 diff] Bash …`).
- `.agents/deliver-loop.ansi.log`: the same lines with ANSI colors (time dim, tool name cyan, `✗` and tool errors red; paused, blocked, CI red, no progress yellow; PR opened, merged, done green). `.agents/deliver-loop.log` stays plain text. `-NoColor` or a non-empty `NO_COLOR` writes no `.ansi.log`.
- Log pane (herdr, `HERDR_ENV=1`, stdout a terminal): before the first iteration the loop splits a pane labeled `deliver-log` below the session (`herdr pane split --current --direction down --ratio 0.7 --no-focus`) that runs `pwsh -NoProfile -Command "Get-Content -Tail 40 -Wait <log>"` on the `.ansi.log` (on the plain log with `-NoColor`/`NO_COLOR`). A `deliver-log` pane already in the tab is reused, any herdr failure is ignored, the loop never closes the pane, so the last lines stay readable after the run. `-NoPane` opens none. The function is `Open-LogPane` in `scripts/LogPane.ps1`. A herdr key toggles the pane (`scripts/Switch-LogPane.ps1`, `references/herdr.md` "Log pane toggle").
- Notifications (macOS `osascript`, Windows toast, else none): PR opened, review fix round, CI red, paused, blocked, merged, and the loop result.
- herdr (`HERDR_ENV=1`): the loop reports the pane tokens of `references/herdr.md` for the PBI it runs (`deliver=#401`, `deliver_step=3/7 review: clean` with the loop position, `deliver_pr=#55` once the PR exists, TTL one hour) and clears them on exit, Ctrl+C or error. The harness it starts runs with `HERDR_ENV` empty and reports nothing itself.
