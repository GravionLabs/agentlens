# `## Summary` as the smallest visual

Write `## Summary` as the smallest visual that makes the change clear, plus one or two short lines (what and why). Prose alone is the fallback, not the default.

Adapted from mattpocock `pr` (credits: `show-me` by Dex Horthy, HumanLayer). Merge danger belongs in `## Risk`, evidence in `## Verification` and `## Visual proof`; this file covers only the summary form.

## Forms

| Form                   | Fits                                                              |
| ---------------------- | ----------------------------------------------------------------- |
| Pseudocode             | a changed algorithm or rule: show the new logic in a few lines    |
| Call tree              | a changed flow: who calls whom, with the new or moved node marked |
| Component or file tree | new or moved files, modules or components                         |
| `diff` sketch          | one of the above with `+`/`-` lines for what changed              |
| Mermaid                | a flow, state machine or sequence a tree cannot show              |

Pick the first form that fits; Mermaid only when a tree or pseudocode does not.

## Limits

- At most one or two visuals, each short enough to read at a glance.
- A one-line change keeps a one-line summary, with no visual.
- Release mode is unchanged: its body follows `release-mode.md`.
- The lines name the change and the reason; no restating of the visual, no task numbers.
