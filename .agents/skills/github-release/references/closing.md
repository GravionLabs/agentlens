# Closing issues at merge

`Closes #n` fires only on the default branch, so an issue whose PR merges into `release/*` would stay open until the release. The rule instead: an issue closes when its PR is merged into `release/next` (or `release/<x.y>`). "Released" is a closed issue in a closed milestone (`github-milestone close`), not an open issue with a label.

## Lifecycle

1. A PR merges into a `release/*` branch. The reusable workflow `GravionLabs/ci/.github/workflows/release-branch.yml@v1` (`release-branch.yml` template) closes every issue the PR closes or references (`Closes`/`Fixes`/`Refs #n` in the PR body or its commits) with the comment "ships with <milestone>" (`none` without milestone), and closes the parent PBI and Feature once all their children are closed. Only merged PRs whose base is the pushed branch count.
2. `close-release-issues` (`gh-release`) is the safety net: when the release lands on `main` it closes what is still open (issues with `Closes #n`, their open sub-issues, parents whose sub-issues are all closed). An Epic is never closed this way.
3. `github-release finish` after the tag closes the milestone itself (`references/close-milestone.md`).

## A reverted PR

The workflow never reopens. If a merged PR is reverted, reopen its issues by hand (`gh issue reopen <nr>`) and comment why.

## One-time catch-up

Issues labelled `merged` from the old label lifecycle are still open. Run the workflow once with `workflow_dispatch` (Actions, "Release branch", branch `release/next`); set `dry-run` first to see which issues it would close. Afterwards the label is no longer used.

## Rollout to existing repos

One PR per repo: copy `templates/release-branch.yml` to `.github/workflows/release-branch.yml` (replace an inline copy of the workflow; set the input project-url) and open the PR with `github-pull-request`. `cut` offers this when no workflow in `.github/workflows/` calls `release-branch.yml` of GravionLabs/ci (`GravionLabs/ci/.github/workflows/release-branch.yml@v1`, or `./.github/workflows/release-branch.yml` inside ci itself).

## Permissions

- Token requirements are in the README of GravionLabs/ci (`release-branch.yml`): the GitHub App `gravionlabs-ci` (secrets `CI_APP_ID` and `CI_APP_PRIVATE_KEY`); the `GH_PAT` fallback was removed in v1.
- The workflow needs `contents: read`, `issues: write`, `pull-requests: write` (the calling job grants them to the reusable workflow, `permissions: {}` at the top).
- Secrets are passed explicitly (`CI_APP_ID` and `CI_APP_PRIVATE_KEY` required, `PROJECT_TOKEN` optional), not `secrets: inherit`. Without the app secrets the workflow fails; the app token makes PRs created by CI trigger the required checks.
- The project Needs sync needs a classic PAT with scope `project` in secret `PROJECT_TOKEN`. A fine-grained token does not work for a user-owned project (only an organization-owned project accepts one). Failures per issue are warnings and never fail the job.

## Known limit

The release PR step of the reusable `release-branch.yml` (`versioning/release-pr`) accepts only `release/<x.y>` and ignores `release/next` (GravionLabs/ci#55). Closing is a separate step and is not affected. Checked 2026-10-26: GravionLabs/ci#55 is OPEN (current state: `gh issue view 55 -R GravionLabs/ci --json state`).
