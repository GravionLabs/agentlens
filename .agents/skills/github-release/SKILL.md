---
name: github-release
description: 'Cut the rolling release/next branch or finish it with the release PR and changelog. Use for "cut a release", "finish the release" or when no release branch exists.'
context: fork
---

Owns the lifecycle of the rolling `release/next` branch (explicit `release/<x.y>` only in parallel lines). The model and the base-branch resolver are in the `github-issue` skill's `references/release-branches.md`. No dialogue, tool calls only; on Claude Code this runs in a forked context without the session history.

Prerequisites: `gh` authenticated, repo root, clean `git status`. Mode: `cut [--feature <nr>]` or `finish [major] [release/next|release/<x.y>]`.

## Check the state first

```powershell
pwsh scripts/Get-ReleaseState.ps1 -Mode cut            # or: -Mode finish [-Branch release/next]
```

The script is read-only and prints JSON: `default`, `branch`, `state`, `openPrs`, `localAhead`, `staleLines`, `deleteAllowed`, `action`, `hasSemverCommit`, `hasMajorCommit`, `releaseWorkflow`, `releasePr`. Exit 1 with a message on stderr (invalid branch name, no release branch) means stop and report. Do what `action` says; `stop: ...` means report the text and change nothing.

| `state`                     | cut                                                             | finish                                                                                                 |
| :-------------------------- | :-------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------- |
| `absent`                    | `create`                                                        | stop                                                                                                   |
| `fresh` (equals default)    | no-op                                                           | stop: nothing to release                                                                               |
| `unreleased`                | no-op                                                           | continue below                                                                                         |
| `behind` (default moved on) | no-op                                                           | stop, merge default in first                                                                           |
| `merged`, `released`        | `recreate` when `deleteAllowed`, else stop and name the blocker | `merged` with `deleteAllowed`: `delete-branch`; else stop "already released; cut a new release branch" |
| `released-continued`        | no-op                                                           | stop "already released; cut a new release branch"                                                      |

## cut

1. `action: create`: create `release/next` from the default branch and push it with upstream.
2. `action: recreate`: delete `release/next` on origin (`git push origin --delete release/next`; the guardrail allows exactly this), then `git checkout -B release/next origin/<default>` and push with upstream. No force push, no `+semver` commit: `finish` adds it.
3. Each `staleLines` entry was released and is never reused. Report it with `git push origin --delete release/<x.y>`; the owner runs it (the guardrail blocks it in a session). Continue with the cut.
4. `releaseWorkflow` false: tell the user the repo does not label merged issues and offer to copy `templates/release-branch.yml` to `.github/workflows/release-branch.yml` (never silently, one PR per repo, `references/merged-label.md`). A GitHub Project mirrors the label as status `In release` via `github-project sync`.
5. With `--feature <nr>`: insert `Release branch: release/next` as the second line of that issue's body (`gh issue view <nr> --json body`, then `gh issue edit <nr> --body-file -`), the `Parent ...` line stays first.

## finish

1. `state` `unreleased`, `openPrs` empty: continue. Otherwise `action` already says stop; list `openPrs` when they block.
2. Bump: minor by default, `major` only with the argument `major`; never ask in a fork. The version is determined on the default branch after the merge, where no branch config of `release/*` applies, so every release branch needs the commit: `git commit --allow-empty -m "chore: release minor|major" -m "+semver: minor|major"` and a push. Minor only when `hasSemverCommit` is false, major only when `hasMajorCommit` is false.
3. `releasePr` set: CI keeps it current, report it. None: open it with `github-pull-request` in release mode (`branch`, `base_branch: <default>`, `title: release: <branch>`, `merged_prs` per its default, `feature_number` only when recorded, `auto_merge: false`). Never use a direct `gh pr create`; when `github-pull-request` stops, stop and report. Do not touch `CHANGELOG.md`: the release job on `main` regenerates it with git-cliff, tags, publishes and closes the issues the release's PRs reference.
4. Report the PR URL, `gh pr merge <nr> --merge` (merge commit, never squash) and the cleanup `git push origin --delete <branch>`, then stop; never wait for the merge. The cleanup after the merge is a later `finish` on the merged branch: `Get-ReleaseState.ps1` returns `delete-branch` and that run deletes `release/next`.

## Gotchas

- Never merge the release PR yourself; the push to `main` builds, regenerates the changelog and closes the delivered issues.
- The `merged` label lifecycle: `references/merged-label.md`. A repo without `cliff.toml` gets no changelog from CI: `references/cliff-setup.md`.
- Hotfixes bypass this skill (`fix/<nr>-<slug>` from `main`); afterwards merge `main` into `release/next`.
- A release branch is never continued after its merge. The CI release PR title may show the branch name, not the version (`release-branches.md`).

Report: mode, branch, PR URL (finish) or the Feature issue updated (cut).
