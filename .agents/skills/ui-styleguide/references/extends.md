# Create a project guide that extends a base guide

A project on a design system keeps only its own rules and deviations. The base guide stays where it is.

## Input

`/ui-styleguide extends <path>`, or "add a style guide on <design system>". `<path>` is the core file of the base guide, relative to the repository root, usually inside a package, e.g. `node_modules/<package>/<guide path>/STYLEGUIDE.md`.

- No path given: take it from the user or from a document of the design system that names it. Never build one from a package name.
- The path does not exist: report `base guide not found: <path>` (say when the dependencies are not installed), write nothing and stop. Never take another file instead.
- Without a user to ask and without a path, say that the path is needed and stop.

## Steps

1. Read the base core file and the deep dives it links.
2. `docs/STYLEGUIDE.md` exists: never overwrite it. With an `Extends:` line, use [sync.md](sync.md). Without one, propose a diff that adds the line and removes what repeats the base, and write it only after approval (`## Approval` of `sync.md`).
3. Else copy [STYLEGUIDE.extends.md](../templates/STYLEGUIDE.extends.md) to `docs/STYLEGUIDE.md`. Its first line is `Extends: <path>`.
4. Add a section only for a topic where the project changes the base, under the heading the base uses. Fill it from the code as [fill.md](fill.md) does: nothing invented, an unsettled point is an `Open: <question>` line.
5. A project rule that is stricter than the base goes into its section. A rule that relaxes or contradicts the base goes under `## Deviations` with the base rule and the reason. A reason the code does not show is `Open: why <deviation>?`, never invented.
6. Copy nothing of the base: no section, rule, deep dive or rubric. Add a file in `docs/styleguide/` only for rules of the project itself.
7. Add the extends paragraph of [agents-styleguide.md](../templates/agents-styleguide.md) to `AGENTS.md`, with the base path filled in; it replaces a plain style guide paragraph.

## Check

- Every path of `Extends:` and of `AGENTS.md` exists.
- Every entry of `## Deviations` has a rule and a reason or an `Open:` line.
- No line repeats the base, no `<placeholder>` is left, the file has at most 100 lines.
