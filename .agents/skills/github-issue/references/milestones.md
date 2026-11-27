# Milestones and the issue levels

A milestone is a release and holds the work items that are delivered. The rule is written here once; `planner`, `roadmap`, `deliver` and `github-milestone` follow and check it. Its title is `M<nnn>` (three digits, `M001` to `M999`; the lookup accepts `M11` for `M011`), see `github-milestone`.

| Level     | Milestone                                                          |
| :-------- | :----------------------------------------------------------------- |
| PBI       | Always: the milestone it is delivered in.                          |
| Bug       | Always: the milestone it is delivered in.                          |
| Feature   | Only when all its PBIs are in that same milestone, otherwise none. |
| Epic      | Never.                                                             |
| Objective | Never (it spans repos and releases).                               |

Why: `deliver --milestone` takes the open PBIs and Bugs of the milestone plus the work items below the Features in it. A Feature in the milestone with PBIs outside of it pulls those in, and they then do not count in the milestone progress. `deliver` therefore skips such an item (`#<nr> select: outside M<nnn>, skipped`); `/deliver <feature>` without `--milestone` delivers all its PBIs as before.

## Setting a milestone

Only `github-milestone assign M<nnn>|none #<nr> …` sets or clears the milestone of issues (`scripts/Get-Milestones.ps1 -Mode assign`; `-DryRun` writes nothing). Never set one by hand: the milestone flags of the `gh` issue-edit call (`--milestone`, `--remove-milestone`) are not used in any skill, because they skip the checks below and the report. `assign` takes an open milestone only (a missing `M<nnn>` is not created: `create` first), refuses an Epic or an Objective (except `none`) and a Feature whose open sub-issues would not all be in the milestone afterwards, and reports each change as `set` or `unchanged`. A PBI or Bug created by `github-issue` `references/create-from-plan.md` gets its milestone this way.

## Release test

One `release-test` per milestone (label `pbi` + `release-test`, usually `interactive`): the manual platform test on the release candidate. Rule: one `release-test` per milestone, always last, never a gate unless `release.test.gate` is true (then the RC test blocks the release; see `release-review.md`, `project-config.md`). `deliver` never selects it as work: step 9 cuts the RC once no other `pbi`/`bug` of the milestone is open and posts it on the release-test issue, which stays open for the human (`select: release test, left for step 9`; with open items before, `release test, waits for <n> open items`); `interactive` stays on it.

RC of a desktop app (Tauri): every RC and beta build has its own identifier `<identifier>.rc`, product name `<Name> RC`, its own runtime, config and data paths and no auto-update, so it installs side by side with the stable build. The release test installs that RC next to the stable app and never runs on a `tauri dev` build; update checks run on the stable app only.

## Closing

An issue closes when its PR is merged into `release/next` (comment "ships with <milestone>", `release-branch.yml` of GravionLabs/ci), also its PBI or Feature parent once all sub-issues are closed; an Epic is never closed this way, an Objective is closed by a human only (`objectives.md`). `close-release-issues` (GravionLabs/ci) is the safety net when the release lands on `main`. "Released" means a closed milestone (`github-milestone close`), not a label. A reverted PR: reopen its issues by hand (`github-release` `references/closing.md`).

## Checks

- `github-milestone assign` refuses an Epic (except `none`) and a Feature whose open sub-issues would not all be in the milestone, before the first write; `warnings` names a PBI or Bug left without a milestone (`no-milestone`).
- `github-milestone` `current` and `<name>` list a `warnings` group: an Epic in the milestone; a Feature in it with open sub-issues outside of it (with their milestone or `none`).
- `roadmap` (`Set-IssueMilestones.ps1`) refuses an Epic and a Feature whose open PBIs would not all be in its milestone; nothing is written then.
- GravionLabs/ci: `project-needs.yml` (nightly, mode `project`) and `issue-events.yml` (on milestoned and demilestoned events) run `Sync-ProjectNeeds.ps1`, which checks the Current and Milestones fields of the project against the issues' milestones; it does not check the milestone rule itself.
- Fix a warning by moving the PBIs into the milestone or clearing the milestone of the Feature or Epic.
