# Audit views against the review rubric

Review existing views from screenshots, without a diff. No browser is driven, no file of the repository is written, and nothing is created in the tracker without approval.

## Input

`/ui-styleguide audit [<names>…]`. Take the first source that applies:

1. A screenshot folder the user names: its image files.
2. The screenshot command of the project, from `AGENTS.md`, else from the scripts of `package.json` (e.g. `pnpm screenshots`): run it and read the images it writes.

Neither exists: say so, ask for a folder or a command, and stop. Do not take screenshots another way.

- `<names>` keeps the views whose file name or path contains one of them (case-insensitive). A name that matches nothing is reported with the list of views found.
- A view is one screen; images of its states or sizes (e.g. empty, error, dark, mobile) belong to the same view.
- A failed command or an unreadable image is reported; the view is listed as `not checked: <reason>`, never judged from its name.

## Rubric

Read the first that exists:

1. `docs/styleguide/ux-review-rubric.md` of the repository;
2. the base guide's rubric, when `docs/STYLEGUIDE.md` starts with `Extends: <path>`: `ux-review-rubric.md` beside `<path>` or in its `styleguide/` folder;
3. the template of this skill, [ux-review-rubric.md](../templates/styleguide/ux-review-rubric.md).

An `Extends:` path that does not exist is reported as `base guide not found: <path>`, never guessed; go on with the next source.

Read `docs/STYLEGUIDE.md` and its deep dives when they exist, and with `Extends:` the base guide and its deep dives too: its rules apply unless `## Deviations` replaces them. Without them, say once that no guide rule can be named.

## Per view

Walk each view with the questions of the rubric. Report it as `## <view>` with the screenshots used, then the five headings of the rubric in its order: `### Top fixes`, `### Friction notes`, `### Suggested changes`, `### Keyboard and speed check`, `### Follow-up states (loading, empty, error, partial)`. Write "None" under a heading with nothing to report.

- A finding that breaks a guide rule names the file and the section, e.g. `docs/styleguide/ux-rules.md`, `### Dialog anatomy`. A finding no rule covers names none; never invent a rule.
- A screenshot shows no keyboard order, focus or speed. Judge keyboard and focus only from the markup of the view, and say under `### Keyboard and speed check` what was read from which file and what was not checked; never guess.
- A state without a screenshot or markup is `not checked`, not "missing".

## Ranking

End with `## Top fixes across views`, ranked, most important first:

1. a blocked or wrong main task, or data loss;
2. then a broken guide rule;
3. then, within the same class, the fix that touches more views.

A problem several views share is one entry that lists them. Each entry names the views, the element and the fix. No finding: say so and offer no drafts.

## Drafts

Only on request ("create PBIs for 1 and 3"):

1. Write one PBI draft per requested fix in the `github-issue` format: title `feat|fix(scope): …` with the `[PBI]` prefix of that skill (`fix` for behavior that is wrong, `feat` for something missing), then current behavior, desired behavior and acceptance criteria. Current behavior names the views and the guide rule.
2. Show every draft in full. Create nothing before the user approves it; silence or a question is no approval.
3. Create only the approved drafts, as shown, with `github-issue`. A requested change gives a new draft, shown again.

Without a user to ask, return the report and stop.
