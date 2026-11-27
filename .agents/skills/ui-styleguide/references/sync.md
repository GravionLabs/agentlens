# Sync an existing guide with the code

Compare the guide with the code and the template, then propose the changes. Read-only until the user approves.

## Input

Read `docs/STYLEGUIDE.md` and every `docs/styleguide/*.md`. Without `docs/STYLEGUIDE.md` there is nothing to sync: say so and offer the steps of `SKILL.md`.

## Base guide

A guide that starts with `Extends: <path>` extends a base guide ([extends.md](extends.md)). Read the base core file and its deep dives; `<path>` is relative to the repository root. The path does not exist: report `base guide not found: <path>`, never guess another file, run the checks that need no base and list the base comparison as `not checked`. A proposed diff never changes the base.

With a base, add two checks:

- **Repeats the base.** A project rule that says the same as a base rule is flagged for removal, with both files and lines. A stricter rule is not a repeat.
- **Deviation without a reason.** An entry of `## Deviations` with an empty, placeholder or `Open:` reason is flagged. The diff never invents a reason: it asks for one.

Check 3 is then `not needed: extends a base`: parts the base has are not missing. Checks 2 and 4 count what neither the guide nor the base covers, and apply the base rules unless `## Deviations` replaces them.

## Checks

Run all four with grep and file reads. Every finding names the file (and line) and the evidence. A check without a finding is reported as `none`; a check you could not run is reported as `not checked: <reason>`.

1. **Stale references.** Search the code for every path, token name and component name the guide names. Report each one that no longer exists, with the guide line and the search that found nothing. A likely rename (same value or same job under a new name) is named with its file.
2. **Missing components.** Find the shared components by the rule of [fill.md](fill.md) step 4: components that appear in more than one screen. Report each one the inventory of the guide lacks, with its path and two screens that use it.
3. **Missing template parts.** Compare headings and files with `templates/STYLEGUIDE.md` and `templates/styleguide/*.md`. Report each section, subsection or deep dive the guide lacks. `fill.md` step 8 drops parts with nothing to say, so propose a part only when the code has real content for it; else list it as `not needed: <reason>`.
4. **Broken rules.** For each measurable rule of the guide, search the code it applies to, e.g. a hex color or a raw font size in component styles when the guide forbids it. Leave out the token file, generated files, dependencies and tests. Report per rule the count and up to five examples (`file:line` and the matching text). A rule no search can decide is listed as `not checked`.

## Output

1. **List**: the findings grouped by the checks, each with file and evidence.
2. **Proposed diff**: one unified diff of the guide files only.
   - Follow `fill.md`: nothing is invented, roles not values, an unsettled point is an `Open: <question>` line.
   - A broken rule stays in the guide: the code is what deviates. Change or drop the rule only when the user says the code is right.
   - The core file stays at ≤ 100 lines; detail goes into a deep-dive file.
   - Code is never part of the diff.

No difference: report that the guide is in sync and propose no diff.

## Approval

- Write nothing before the user approves the diff. Silence or a question is no approval.
- Apply an approved diff as shown. A partial approval or a requested change gives a new diff, shown again before it is written.
- A guide file that changed since the diff was made: make and show the diff again.
- Without a user to ask, return the list and the diff and stop.
