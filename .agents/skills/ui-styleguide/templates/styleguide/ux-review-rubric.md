# UX review rubric

Read when reviewing a UI change. Walk the screen with the questions, then report in the fixed output format below.

## Questions to ask of a screen

- What is the one thing the user came to do, and is it the most prominent element?
- Can the user tell where they are, what just happened and what comes next?
- Does every action give feedback, and can a destructive one be undone or confirmed?
- Does the screen use existing components and tokens, or does it invent new ones?
- Are labels, errors and empty states plain words that say what to do next?
- Can the whole flow be done with the keyboard, in a sensible tab order, with visible focus?
- What happens while data loads, when there is none, when it fails and when only part arrives?

## Output format

Use exactly these headings, in this order. Write "None" under a heading with nothing to report.

### Top fixes

The changes that must happen before merge, at most five, most important first. Each names the element and the fix.

### Friction notes

Where the user slows down, hesitates or can err. Each note names the step and why it hurts; no fix required.

### Suggested changes

Optional improvements, each with the concrete change and the benefit.

### Keyboard and speed check

- Keyboard: every action reachable, tab order, shortcuts, focus visible.
- Speed: clicks or keystrokes for the main task; feedback within 100 ms.

### Follow-up states (loading, empty, error, partial)

One line per state: present and fine, present with a problem, or missing.
