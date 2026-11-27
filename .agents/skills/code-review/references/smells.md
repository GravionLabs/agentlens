# Smell baseline

Loaded by step 5 of `SKILL.md` for every non-test file of the diff. Adapted from the smell baseline of `code-review` in mattpocock/skills (MIT) and Fowler, _Refactoring_, ch. 3. It applies when the repository documents nothing on the point.

## Rules

- A smell is a judgment call and always a **note**, never blocking, named as `possible <smell>` (`possible Duplicated Code`), with file and line.
- Suppressed when a `## Review rules` entry of the language skill or an approved lesson endorses the pattern.
- Skipped when a linter of the repository enforces it (the lint step reports it).
- Report only smells the diff introduces or worsens, not those of untouched code.
- A fix must not add an abstraction without a concrete need (step 10). Speculative Generality is the other side of that rule: both say "no structure without a use today". Where the fix of a smell (extract, introduce a class) would be such an abstraction, say "possible <smell>" and name the duplication or coupling that exists now, not a future one.

## Smells

- **Mysterious Name** — a name that does not say what the thing is or does. → Rename it; when no good name comes, the design is unclear: simplify first.
- **Duplicated Code** — the same logic in two or more places of the diff. → Extract the shared part (function, method) when the copies change together; leave it when they only look alike.
- **Feature Envy** — a function uses the data of another module more than its own. → Move it (or the envious part) next to the data.
- **Data Clumps** — the same group of values travels together (parameters, fields). → Group them in one object or type.
- **Primitive Obsession** — a domain concept (money, range, id, status) as a bare string or number. → Introduce a small type for it.
- **Repeated Switches** — the same `switch` or `if` chain on one condition in several places. → Replace by polymorphism or a lookup in one place.
- **Shotgun Surgery** — one change needs small edits in many places. → Gather the pieces in one module.
- **Divergent Change** — one module changes for several unrelated reasons. → Split it by reason to change.
- **Speculative Generality** — hooks, parameters, base classes or options for a case that does not exist. → Remove them; add them when the second case arrives.
- **Message Chains** — `a.b().c().d()` that makes the caller know the structure. → Hide the chain behind a method of the first object.
- **Middle Man** — a class or function that only forwards to another. → Remove it and call the target; keep it when it is a deliberate boundary.
- **Refused Bequest** — a subclass that ignores or overrides away what it inherits. → Replace inheritance by composition, or move the unused part down.
