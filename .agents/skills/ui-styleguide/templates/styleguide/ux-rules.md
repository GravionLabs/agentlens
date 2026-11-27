# UX rules

Read when you add a flow, a form, a dialog or an error message.

## Feedback

- Feedback within 100 ms; work over 1 s shows progress; work over 10 s can be cancelled.
- Success is confirmed where the user looks (inline), not only in a toast.

## Forms

- Validate on blur and on submit; the error names the field and the fix.
- Submit stays enabled; an invalid submit moves focus to the first error.

### Field anatomy

- The visible label sits above the field.
- A hint sits below the field in the muted text role.
- An error sits below the field; the field sets `aria-invalid`, and `aria-describedby` names the error and the hint.
- A placeholder is a hint, never the label and never an example value that reads as filled in.
- A path or value picker is one input group: the input and its button share one label.

## Dialogs and destructive actions

- A destructive action names what is lost ("Delete 3 files"), or offers undo.

### Dialog anatomy

- The footer has exactly one primary action.
- Footer order: `<platform order>`.
- Cancel never uses the danger role.
- Default focus is on the first field, or on the primary action when there is no field.
- Enter submits; Escape closes; a close button is the visible way out.
- The size follows the content.

## Messages

- Error text says what happened and what to do next, in plain words, without codes alone.
- Empty states say how to fill the view.
