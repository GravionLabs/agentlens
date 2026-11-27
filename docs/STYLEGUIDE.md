Extends: node_modules/@gravionlabs/helix-ui/styleguide/STYLEGUIDE.md

# UI style guide

Binding for every UI change of the application (`src`). Read the base guide named above first (installed by `pnpm install`; [helix-ui 22.10.0](https://github.com/GravionLabs/helix/blob/v22.10.0/docs/styleguide/STYLEGUIDE.md)), then this file. This file holds only the rules of AgentLens and its deviations; where it says nothing, the base applies. A deviation not listed here needs a reason in the PR. The standalone report (`crates/agentlens-core/assets/report.html`) is not covered.

## Color

- A chart series takes its color from `--c-baseline`, `--c-load`, `--c-carry` or `--c-follow`; a series that appears in several charts or legends keeps the same one.
- Palette, decided in [#557](https://github.com/GravionLabs/agentlens/issues/557): the app uses the Helix default palette and components read Helix tokens directly; `src/helix-theme.css` overrides shape and type only. Only the `--c-*` series are literals, in `src/styles/chart-colors.css`.
- A status color in the app's own markup is `--h-tag-warn-color` (estimates, warnings), `--h-tag-danger-color` (errors, a change for the worse) or `--h-tag-success-color` (a change for the better): Helix has no semantic status token, and these are readable as text in light and dark.

## Measured and estimated values

The data rule is in [AGENTS.md](../AGENTS.md#rules) (*Measured vs. estimated*); these are its rules for the screen.

- Format tokens, money, shares and durations with `src/app/core/format.ts`, which writes the tilde of an estimate and `n/a` for a missing value. A value the client does not record is never shown as `0`.
- An estimate in a view also has the class `est` (the warning color), next to the tilde and never instead of it. An `n/a` value is not marked.
- A chart draws estimates hatched and measured values solid, and the text above it says so.
- A view that shows estimates explains the tilde in the details of its page header.

## Data density

- Rows of data with columns are an `hx-data-table` with an `ariaLabel` and an `emptyMessage`; numeric columns are right-aligned.
- A table with a column that can be `n/a` and is sortable sorts with `nullsLast` (`src/app/ui/sort.ts`).
- A table that can grow long paginates only above one page (`[paginator]="rows.length > pageSize"`).
- A cell stays one line: a path shows its last two segments (`shortPath`, `pathLabel`) with the full path as tooltip, and a rounded token count carries the exact one (`exactTokens`).

## Filter bar and page header

- The filters that apply to every view (period, sources, project, roles, comparison) are in the filter bar only, which is a `topbarActions` entry of the shell (the page header). A setting of one view stays in that view.
- The filter bar is one row: it shrinks and scrolls, it never wraps or changes the height of the header. Its errors show below the header.
- The nav rail only navigates.
- An analysis view starts with `app-page-header`: title, one-sentence summary, the explanation behind *Explain*, and the export menu when the view can be exported.

## View toggles and filters

- Switching what one table shows is an `hx-select-button` with an `ariaLabel` and `[allowEmpty]="false"` (Context). Separate panels are `hx-tabs` (Flow).
- A boolean filter of a view is an `hx-switch` in an `hx-field` above the table it filters (Skills, Unused).

## Forms

- The *What if* form of the cost view (`src/app/features/cost/cost.html`) is the reference form: copy its structure for a new form. It is a single narrow column of `hx-field`s with a text *Clear* next to the submit button, and an entry that the core rejects shows its error below the field.

## Deviations

| Base rule | This project | Reason |
| --- | --- | --- |
| STYLEGUIDE.md, Tokens: no literal colors in app CSS | The chart series `--c-*` are literals | Helix has no chart tokens ([#557](https://github.com/GravionLabs/agentlens/issues/557)) |
| color.md, Status: app CSS shows a status with a Helix component | Text and timeline markers of the app read the `--h-tag-*-color` tokens | An estimate or a trend is a value in a table or card, not a tag; it keeps its tilde or sign next to the color |
| color.md, Primary: `--h-primary-color` colors the text of a link only | It also colors the load progress bar, the timeline series, the model switch marker and the highlighted borders (page header details, glossary entry, help link) | They carried the former accent; the app has no other highlight color |
| STYLEGUIDE.md, Components: use the Helix component before you build one (`hx-chart`) | The stacked bars and the timeline are hand-written SVG; the other hand-written parts are listed in the ADR | Owner decision in [ADR 0007](adr/0007-helix-ui.md) |
| STYLEGUIDE.md, Components: a form control sits in an `hx-field` with a `label` | The filter bar has `<label for>` elements that are hidden visually; the comparison switch has a visible label | The page header has no room to show the labels |
| STYLEGUIDE.md, Tokens: change a look by a token, never the CSS of a Helix component | `src/styles.css` has rules for the shell's top bar, status bar and narrow-window nav rail | The filter bar needs the free width, the command line tools panel would be clipped, and helix-shell 22.10.0 leaves a strip of the closed rail visible |
| STYLEGUIDE.md, Tokens: light and dark are set through `provideHxTheme` and `HxTheme` | `data-theme` and `prefers-color-scheme` drive the mode; `HelixDark` (`src/app/ui/helix-dark.ts`) passes it to Helix | The application follows the system and has no theme setting ([ADR 0007](adr/0007-helix-ui.md)) |
