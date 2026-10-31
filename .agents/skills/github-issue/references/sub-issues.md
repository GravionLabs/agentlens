# Native GitHub Sub-Issues

Parent/child links use GitHub's **native sub-issues**, not markdown checklists.

A checklist is a second source of truth that drifts: it has to be hand-edited on every close,
nothing validates it, and a missed edit is invisible. Native sub-issues drive the parent's progress
bar and completion % automatically, so **closing a child is the only action needed** — there is no
checklist to tick.

Keep a human-readable parent line as the **first line of the child's body** (`Parent Epic: #12`,
`Parent Feature: #34`) so the relationship is legible in plain text and in
search. That line is documentation; the native link is the mechanism. Both must agree.

## Legacy Task issues

PBIs and Bugs have no sub-issues. `[Task]` issues (label `task`, first line `Parent PBI: #N` or `Parent Bug: #N`) of older plans are read, never created: they are part of their PBI, never a separate branch or PR. When the PBI is closed or delivered, close its open Task sub-issues with it:

```powershell
gh api repos/{owner}/{repo}/issues/<pbi>/sub_issues --jq '.[] | select(.state == "open") | select(any(.labels[]; .name == "task")) | .number' |
  ForEach-Object { gh issue close $_ --comment "Closed with #<pbi>." }
```

## Read the children of an issue

```powershell
gh api repos/{owner}/{repo}/issues/<parent-nr>/sub_issues --jq '.[] | "\(.number) \(.state) \(.title)"'
```

Returns children in display order. Use this wherever a workflow needs the list of Features under an
Epic or PBIs under a Feature (legacy: Tasks under a PBI). An issue with no children returns `[]`.

## Link a child to a parent

Linking is a GraphQL mutation over **node IDs**, not issue numbers:

```powershell
$parentId = gh issue view <parent-nr> --json id --jq .id
$childId  = gh issue view <child-nr> --json id --jq .id

$query = 'mutation($parentId: ID!, $childId: ID!) { addSubIssue(input: { issueId: $parentId, subIssueId: $childId }) { issue { number } } }'
gh api graphql -f query=$query -f parentId=$parentId -f childId=$childId
```

Re-running on an already-linked pair is harmless, which makes batch scripts safe to resume.

## Move a child to a different parent

A child has at most one parent — unlink before re-linking:

```powershell
$query = 'mutation($parentId: ID!, $childId: ID!) { removeSubIssue(input: { issueId: $parentId, subIssueId: $childId }) { issue { number } } }'
gh api graphql -f query=$query -f parentId=$oldParentId -f childId=$childId
```

Then `addSubIssue` against the new parent, and update the child's `Parent …: #N` body line to match.

## Blocked-by links

"<nr> is blocked by <blocker>" is a native REST dependency. The calls take the blocker's **database
id** (`.id` of the REST issue, an integer), not the GraphQL node id and not the issue number:

```powershell
$blockerId = gh api repos/{owner}/{repo}/issues/<blocker-nr> --jq .id

# add
gh api -X POST repos/{owner}/{repo}/issues/<nr>/dependencies/blocked_by -F issue_id=$blockerId
# read
gh api repos/{owner}/{repo}/issues/<nr>/dependencies/blocked_by --jq '.[] | "\(.number) \(.state)"'
# remove
gh api -X DELETE repos/{owner}/{repo}/issues/<nr>/dependencies/blocked_by/$blockerId
```

`-F` (capital) sends `issue_id` as the integer the API expects. Adding an existing link
fails with HTTP 422 "Target issue has already been taken": read the links first or treat that error
as done, so a resumed run stays safe. The `Depends on` / `Blocked by #N` body line documents the link.

## Verify

After any batch that creates or moves issues, confirm the two sources agree — the native link and
the body line — and that every non-Epic issue has exactly one parent:

```powershell
gh api repos/{owner}/{repo}/issues/<parent-nr>/sub_issues --jq 'length'
gh issue view <child-nr> --json body --jq '.body | split("\n")[0]'
gh api repos/{owner}/{repo}/issues/<child-nr>/dependencies/blocked_by --jq '[.[].number]'
```

An issue that was created but never linked looks completely normal in `gh issue list`. Silent
orphans are the characteristic failure of this workflow, so verify explicitly rather than assuming
the create step linked anything.

## gh CLI gotchas

- `gh api graphql -f query=@file.graphql` **silently fails to read the file**. Use `-F query=@file.graphql`
  (capital `F`). Inline `-f query='...'` works fine.
- `-f` sends every value as a String. Typed variables need `-F` (e.g. `-F number=42` for an `Int!`).
- `gh issue view` / `gh issue edit` need `--repo <owner>/<repo>` when run outside a git worktree
  (for example from a scratch directory).
- `gh issue create` prints the new issue URL; take the number from its last path segment.

The plan returned by `planner` (`references/plan-schema.md`) is the input for creating and linking the sub-issues: `create-from-plan.md`.
