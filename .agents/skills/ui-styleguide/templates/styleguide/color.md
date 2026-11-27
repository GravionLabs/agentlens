# Color

Read when you add a color role, a theme, or a status color. Roles only: values live in `<token file>`.

## Roles

| Role | Used for | Token |
| :--- | :--- | :--- |
| background | page behind everything | `<token>` |
| surface | cards, panels, dialogs | `<token>` |
| text / muted text | content / secondary content | `<token>` |
| primary action | the one main action of a view | `<token>` |
| danger | destructive action, error | `<token>` |

## Rules

- A view has at most one primary action.
- Every role is defined for each theme (`<light, dark>`); a missing pair is a bug.
- Status colors (success, warning, danger) always come with an icon or label.
- Contrast: text 4.5:1, large text and icons 3:1, checked for each theme.
- A new role is added only when no existing role fits; name it by purpose, not by hue.
