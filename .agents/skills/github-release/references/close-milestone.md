# Closing the milestone after the release (finish step 5)

The release job of GravionLabs/ci (`close-milestones`) normally closes the milestone after the tag; this is the repair path for a milestone it left open. Runs in the later `finish` on the merged branch, after `fast-forward`. Needs `github-milestone` installed (`../github-milestone/scripts/Get-Milestones.ps1`); otherwise stop after the fast-forward as before.

1. Tag: the newest `v<x.y.z>` on the default branch (`git tag --list 'v*' --sort=-v:refname`). No tag yet: report "release job still running" and stop.
2. Version check: `pwsh scripts/Test-ReleaseVersion.ps1 -Tag v<x.y.z>` (read-only JSON: `expected`, `match`, `bump`, `deciding[]`). `match` false: report tag, `expected` and the `deciding` commits (for example a `feat:` that should have made a minor release), then stop. Nothing is renamed or closed with a wrong version.
3. Milestone: `Get-Milestones.ps1 -Mode list`; the lowest open `M<nnn>` or, legacy, the open `x.y.z` with `tagged: true`. None: skip to the report.
4. Leftovers: `Get-Milestones.ps1 -Mode issues -Name <milestone>` lists the open issues of the milestone. PBIs and Bugs among them whose PR is merged into the released branch (missing `Closes`) are the unclosed ones.
5. Confirmation: one question that lists the leftovers to close, the issues `--move-to` moves and the next milestone name. With `--yes` ask nothing. A fork without `--yes` never asks: it reports the plan and the commands and does steps 6 to 8 only with `--yes`.
6. `Get-Milestones.ps1 -Mode create` for the next `M<nnn+1>` (skip when it exists); legacy: the next `x.y.z`.
7. Close the unclosed PBIs/Bugs before the move: `gh issue close <nr> --comment "Released in v<x.y.z>"`.
8. `Get-Milestones.ps1 -Mode close -Name <milestone> -Version <x.y.z> -MoveTo <next>` (a plain close without open issues).
9. Report: the closed milestone with its new title, issues closed and moved, the next milestone with the count of its ready issues (`-Mode issues`, group ready) and the hint to cut `release/next` (`/github-release cut`).
