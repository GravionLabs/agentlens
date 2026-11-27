# Components

Read when you add, change or compose a component. Paths and names are the project's own.

## Inventory

| Component | Use it for | Path |
| :--- | :--- | :--- |
| `<Button>` | actions | `<path>` |
| `<Input>` | single-line text | `<path>` |
| `<Dialog>` | blocking questions | `<path>` |
| `<FormField>` | label, control, hint and error of one field | `<path>` |
| `<InputGroup>` | an input with its picker button or affix | `<path>` |
| `<DialogFooter>` | the actions of a dialog | `<path>` |

## Rules

- A new component is added only when no inventory entry fits; extend a variant first.
- Props describe roles (`variant="danger"`), not looks (`color="red"`).
- Every component lists its states: default, hover, focus, active, disabled, loading, error.
- Spacing between siblings comes from the spacing scale in `<token file>`, set by the parent, not the child.
- Icons: one set (`<icon set>`), one size per context (`<sizes>`); an icon-only button has an accessible name.

## Button hierarchy

One variant per role; a view or dialog footer has at most one primary button.

| Variant | Use it for |
| :--- | :--- |
| primary | the one main action |
| secondary | other actions beside the primary, such as Cancel |
| quiet / toolbar | frequent actions in toolbars and rows |
| icon-only | a quiet action with a familiar icon; it has an accessible name |
| link | navigation, not an action that changes data |
| danger | an action that destroys data; never Cancel |
