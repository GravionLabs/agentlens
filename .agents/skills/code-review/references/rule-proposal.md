# Rule proposal

A finding that keeps coming back should become a structural ban, see the `ast-grep-rules` skill. The review only proposes; it never writes a rule, a rule test or `sgconfig.yml`.

## When

Only when `ast-grep-rules` is installed (`.claude/skills/ast-grep-rules/`, `~/.claude/skills/ast-grep-rules/` or `.agents/skills/ast-grep-rules/`). Otherwise the review behaves as before, without a note.

A **blocking** finding gets the note when one of these holds:

- it matches an approved lesson of `.agents/lessons/` (step 3 of `SKILL.md`): the lesson exists because the finding repeated;
- the same finding class (same defect, same pattern) is in two reviews of this repository, e.g. in the lessons or in earlier review comments of the pull request.

A lesson that already carries `rule: <rule id>` is not proposed again; if the finding still appears, the rule has a gap and the note says so.

Only a pattern that a syntax tree can match qualifies. A semantic or design finding gets no proposal.

## Note

One note per finding class, in `notes`, as plain text:

```text
propose ast-grep rule: <file>:<line> — Why: <the reason>. Instead: <the replacement>. See #<issue>.
```

The text after the colon is the draft message per `ast-grep-rules` (reason, replacement, causing issue). Without an issue number write `See #<issue>` and leave the number to the author. A matching lesson is named by its `id`.
