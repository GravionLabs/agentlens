# UX review of a UI diff

Loaded by step 8 of `SKILL.md` only when the diff has a UI file: `.html`, `.css`, `.scss`, `.tsx`, `.jsx`, `.vue`, `.svelte` or `*.component.ts`. Without one, do not load it.

## Rubric

Read the first that exists:

1. `docs/styleguide/ux-review-rubric.md` of the repository;
2. the base guide's rubric, when `docs/STYLEGUIDE.md` starts with `Extends: <path>` (relative to the repository root): `ux-review-rubric.md` beside `<path>` or in its `styleguide/` folder;
3. the default rubric of the `ui-styleguide` skill, `templates/styleguide/ux-review-rubric.md` (`.claude/skills/ui-styleguide/`, `~/.claude/skills/ui-styleguide/` or `.agents/skills/ui-styleguide/`).

An `Extends:` path that does not exist (e.g. dependencies not installed): note `base guide not found: <path>`, never guess another file, and go on with the next source.

None exists: the review behaves as before, without a note and without UX findings.

## Output

Walk the changed screens with the questions of the rubric. Put the result under `## Notes`, under the five headings of the rubric in its order: `### Top fixes`, `### Friction notes`, `### Suggested changes`, `### Keyboard and speed check`, `### Follow-up states (loading, empty, error, partial)`. "None" under a heading with nothing to report. In the JSON result of `reviewer-contract.md` the same text goes into `notes`; the schema does not change.

## Severity

A deviation from `docs/STYLEGUIDE.md`, or from its base guide unless `## Deviations` lists it, is a note. It is blocking only when an acceptance criterion names it. A UX finding without such a criterion never makes the review blocking.
