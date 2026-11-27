# UI style guide

Binding for every UI change. Read the whole file first; read a deep-dive file only when its condition applies. A deviation needs a reason in the PR.

## Tokens

- All values live in `<path/to/tokens file>` (CSS variables or theme). This guide names roles, never values.
- Never hard-code a color, size, radius or shadow in a component; use a token.
- A new token needs a role name and an entry in the token file, not in this guide.

## Color

- Roles: background, surface, border, text, muted text, primary action, danger, focus.
- Text on a surface has a contrast ratio of at least 4.5:1 (3:1 for large text and icons).
- Color is never the only signal; pair it with an icon or text.
- Read [styleguide/color.md](styleguide/color.md) when you add a role, a theme, or a status color.

## Typography

- One font family for the UI, one for code: `<families>`.
- Sizes come from the type scale in `<token file>`; at most `<n>` sizes per screen.
- Body text is at least `<n>`px; line length is at most 80 characters.

## Radius and elevation

- Radius comes from the scale `<none, small, medium, full>`; nested elements use a smaller radius than their parent.
- Elevation has `<n>` levels (flat, raised, overlay); only overlays (menus, dialogs) cast a shadow.

## Components

- Reuse the component in `<path/to/components>` before you build a new one.
- Interactive targets are at least `<n>`px high and wide.
- Every interactive component has hover, focus, active and disabled states.
- Read [styleguide/components.md](styleguide/components.md) when you add, change or compose a component.

## UX rules

- Every action gives feedback within 100 ms; slower work shows progress.
- A destructive action asks for confirmation or can be undone.
- An empty, loading and error state exists for every data view.
- Read [styleguide/ux-rules.md](styleguide/ux-rules.md) when you add a flow, a form, a dialog or an error message.
- Read [styleguide/ux-review-rubric.md](styleguide/ux-review-rubric.md) when reviewing a UI change.

## Cross-platform

- Supported targets: `<browsers, OS, window sizes>`.
- Keyboard: every action is reachable without a mouse; use `<Cmd/Ctrl>` per platform, never a hard-coded one.
- Layout works from `<min width>`px up; no horizontal scroll.

## When this guide is silent

1. Follow the nearest existing component or screen that solves the same problem.
2. If none exists, choose the simplest option that uses tokens only.
3. Write the choice and the reason in the PR; if it will recur, add a rule here or in a deep-dive file.
