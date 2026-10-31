# herdr progress display

Only when `printenv HERDR_ENV` prints `1` (the session runs in a herdr pane). Without herdr nothing here runs.

When `scripts/Invoke-DeliverLoop.ps1` started this session it owns the pane display: it runs the harness with `HERDR_ENV` empty (so `printenv HERDR_ENV` prints nothing and this file is skipped), reports the same tokens per progress line with the loop position in the step (`deliver_step=3/7 review: clean`) and clears them at the end (`references/loop.md`).

## Start

`printenv HERDR_PANE_ID` once and keep the pane id. Every later call carries the id as a literal: Claude Code denies any command with `$HERDR_PANE_ID` in it, quoted or not (checked headless with `--permission-mode default`, 2026-10-29).

## Per step

After the progress line of a step, report it as pane tokens:

```powershell
herdr pane report-metadata <pane> --source deliver --token deliver=#12 --token "deliver_step=review: 1 blocking" --token deliver_pr=#55 --ttl-ms 3600000
```

- `deliver=#<nr>`: the PBI or Bug of the progress line.
- `deliver_step=<step>[: <result>]`: the step and its result from the progress line, at most 80 characters (cut longer ones).
- `deliver_pr=#<pr>`: only once the PR exists (from the `pr` step on).

The tokens never replace the agent name: the sidebar keeps showing the harness (`claude`, `opencode`). herdr keeps them per source for the TTL (one hour), so a crashed run does not leave them forever. Token names are 1–32 ASCII letters, digits, `_` or `-`; at most 16 per report.

## End

After the report:

```powershell
herdr pane report-metadata <pane> --source deliver --clear-token deliver --clear-token deliver_step --clear-token deliver_pr
```

## Sidebar layout

The Agent sidebar shows a token only where one of its rows names it as `$<name>`. A layout with the tokens in their own row, for the user's herdr config (propose it, never write it):

```toml
[ui.sidebar.agents]
rows = [["state_icon", "agent", "state_text"], [{ token = "$deliver", bold = true }, { token = "$deliver_step", rules = [{ contains = "blocked", fg = "#f55" }, { contains = "merged", fg = "#5f5" }] }, "$deliver_pr"], ["workspace", "tab"]]
```

A herdr plugin can filter panes on the `deliver` token of source `deliver`.

## Log pane toggle

`scripts/Switch-LogPane.ps1` toggles the `deliver-log` pane of the active tab. herdr has no hide, so it closes the pane, and opens it again below the active pane (last 40 lines of `.agents/deliver-loop.ansi.log` under the pane's cwd, else `.agents/deliver-loop.log`). Never two in one tab; no log or outside herdr: no pane, exit 0. A keybinding for the user's herdr config (propose it, never write it; `<skill dir>` is the installed `deliver` skill):

```toml
[[keys.command]]
key = "prefix+l"
type = "shell"
command = "pwsh -NoProfile -File <skill dir>/scripts/Switch-LogPane.ps1"
description = "toggle deliver log"
```

`type = "shell"` runs detached from the focused pane's cwd with `HERDR_ACTIVE_TAB_ID`, `HERDR_ACTIVE_PANE_ID` and `HERDR_ACTIVE_PANE_CWD` set; these count as in herdr even without `HERDR_ENV`. With a herdr plugin, `type = "plugin_action"` takes its place and the script reads `HERDR_TAB_ID` and `HERDR_PANE_ID` (with `HERDR_ENV=1`).

The allowlist entries (`printenv HERDR_ENV`, `printenv HERDR_PANE_ID`, `herdr pane report-metadata`) are in `references/permissions.md`.
