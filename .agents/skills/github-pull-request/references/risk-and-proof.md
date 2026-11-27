# Risk and visual proof

A reviewer of an agent-written pull request needs the risk and the visible result first. Both sections are always in the body of a normal PR. Release mode (`release-mode.md`) has neither.

## Risk

Input `risk`: `level` (low|medium|high), `what` could break and for whom, `backout` how to undo it. Body: `Level: <level>. <what>. Backout: <backout>.` Without the input: `Risk: not assessed`.

| Level  | Changes                                                                                                           |
| ------ | ----------------------------------------------------------------------------------------------------------------- |
| low    | docs, tests, internal refactor with tests                                                                         |
| medium | behavior change inside one module                                                                                 |
| high   | data or persistence, authentication or authorization, security, a public interface, a migration, a protected path |

When changes of several levels meet, the highest level wins.

## Visual proof

For a UI or behavior change that a person can see. Lines of the input `visual_proof`:

- `before: <path or text>`
- `after: <path or text>`
- `N/A: <reason>` for a non-UI change, e.g. `N/A: no UI change, skill text only`

Without the input: `N/A: not stated`. Never invent a screenshot path.
