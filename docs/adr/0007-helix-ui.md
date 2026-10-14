# 0007: UI components and charts from helix-ui

- Status: Accepted (decided by the owner on 2026-10-14)
- Context: PBI #348, Feature #335, spikes #349 and #350

## Context

The application's Angular frontend is hand-written: its own buttons, tables, forms, layout and theme, and SVG charts (the stacked bars of the skills view, the timeline). The owner maintains an in-house Angular library, helix (`GravionLabs/helix`), with components (`@gravionlabs/helix-ui`), an application shell (`@gravionlabs/helix-shell`) and a chart component (`HxChart`, built on chart.js). Feature #335 asks whether the application should use it. Two spikes answered the technical questions:

- **#349** adopted helix-ui in a branch with a custom helix theme built from the AgentLens tokens (`src/helix-theme.scss`), including dark mode.
- **#350** rebuilt two charts on `HxChart` next to the SVG versions ([spike note](../spikes/helix-chart-spike.md)).

The open issue #229 ("decide on a chart library (ECharts)") asked the chart question separately. This ADR decides it together with the component question, because `HxChart` ships with helix-ui.

## Options

Components:

1. **Keep the hand-written components.** No new dependency, but every control, its states and its accessibility stay our work.
2. **helix-ui and helix-shell** (peer dependency `@angular/cdk`). Shared look and behavior with the owner's other applications; a dependency on a library the owner also maintains.

Charts:

1. **Keep SVG.** No dependency and no canvas; hatching, native tooltips and text come for free. Interaction (zoom, brushing, legend toggles) would be our work.
2. **ECharts** (the option of #229). Rich, but a large bundle and a second chart stack next to helix.
3. **chart.js used directly.** The engine of `HxChart` without the helix integration (theme, resize, fallback table).
4. **D3.** Maximum control, but every chart is drawn by hand, like the SVG today.
5. **chart.js through `HxChart`.** Same engine as 3, integrated in the component library the application adopts anyway.

## Decision

- **Components: helix-ui and helix-shell**, with `@angular/cdk` as peer dependency. The packages are pinned to the published release 22.2.7. The theme is the custom helix theme in `src/helix-theme.scss`, derived from the AgentLens tokens, so the look stays that of AgentLens.
- **Charts: chart.js through `HxChart`** is the chart stack for charts that are built on a library. This replaces the ECharts option of #229 (to be closed once this ADR is merged).
- **The choice per chart is not decided here.** The spike recommends keeping the SVG stacked bars (simple and static, the SVG already gives hatching, native tooltips and text, and `HxChart` adds 204 kB raw of lazy chart.js code plus a canvas fallback table for no visible gain) and migrating the timeline only if interaction is wanted (400 points with all markers draw in about 20 ms; theme and resize redraws work without extra code). Migrating both for consistency is feasible with the spike's hatch helper and builders. Issue #351 decides per chart, with this as input.
- **What stays hand-written:** the SVG charts (stacked bars and timeline) until #351 decides; the estimate hatching (`src/app/ui/hatch.ts`) and the number formatting with the tilde, which `HxChart` does not know and which the measured/estimated rule requires. Domain components of AgentLens (views, the glossary, the measured/estimated marking) remain ours; helix supplies the generic parts.
- Dependencies added: `@gravionlabs/helix-ui`, `@gravionlabs/helix-shell`, `@angular/cdk` (peer) and `chart.js` (peer of helix-ui, bar and line elements only, loaded lazily).

## Bundle size

Production `pnpm build`, gzip is `gzip -9` of the `main-*.js` chunk, initial transfer is the "Estimated transfer size" of the initial total that the Angular build prints. All rows were measured on 2026-10-14 in clean git worktrees with `pnpm install --frozen-lockfile`.

| State | Commit | Main chunk raw | Main chunk gzip | Initial raw | Initial transfer | Lazy chart chunk |
| --- | --- | --- | --- | --- | --- | --- |
| Today (release/0.12, no helix) | 1ffdf584 | 266.86 kB | 81,564 B | 270.38 kB | 73.88 kB | none |
| helix-ui components only (#349, before chart.js) | 4123adb4 | 484.13 kB | 136,950 B | 890.37 kB | 159.00 kB | none |
| With `HxChart` (both spike charts, #350) | 2fad0f35 | 488.94 kB | 138,700 B | 895.18 kB | 160.49 kB | 204.34 kB raw, 60.85 kB transfer |

Notes:

- The initial transfer is about 2.2 times today's (73.88 kB against 159.00 kB and 160.49 kB), mostly because of the helix theme styles (405.78 kB raw css, 39.61 kB transfer) and the helix components in the main chunk; this is the cost of the component decision. The 4123adb4 row used local links to helix 0.1.0/0.2.0; the published 22.2.7 build (ced2b9d5) has an initial total of 890.67 kB (+0.30 kB over 4123adb4) and a main chunk of 137,116 B gzip (measured in this task's clean worktree build of ced2b9d5, like the table rows).
- chart.js is a lazy chunk and not part of the initial load; the whole spike adds +218,263 B raw and +75,168 B gzip over all files (spike note, measured with `pnpm build:demo`, a different measure from the table).
- With helix the initial bundle (895 kB) is over the 500 kB warning budget of `angular.json` (today it is under) and 105 kB below the 1 MB error budget; the migration PBI must handle it (raise the budget or trim the theme styles).

## Consequences

- **In-house dependency.** AgentLens follows helix's release cadence and Angular version; a breaking change in helix is ours to absorb, and fixes needed by AgentLens go to helix first. The published 22.2.7 is pinned: `package.json` has the exact version `22.2.7` (no range) for both helix packages, and they resolve from the default package registry (there is no `.npmrc` or scope registry) with integrity hashes in `pnpm-lock.yaml`, not through `link:` or `workspace:` references.
- **Supply chain.** `pnpm-workspace.yaml` has a `minimumReleaseAgeExclude` entry for exactly `@gravionlabs/helix-shell@22.2.7` and `@gravionlabs/helix-ui@22.2.7`. It bypasses pnpm's release-age check for those two packages at that version only; no other package is exempt. The lockfile is committed and the workflows that install (`app.yml`, `pages.yml`) use `pnpm install --frozen-lockfile`, so CI installs what the lockfile records. Removing the entry once the release is older than the minimum age is tracked in the migration PBI #351 (it is part of its scope).
- **Bundle.** +`@angular/cdk` and helix in the initial bundle, +chart.js as a lazy chunk (see the table).
- **Dark mode.** The custom helix theme bridges the AgentLens tokens to helix, and the app's dark mode drives it: the AgentLens tokens flip with the media feature `prefers-color-scheme` and with the `data-theme` override (`src/styles.scss`, `src/helix-theme.scss`), so one theme block serves light and dark. Charts redraw on a theme change (about 90 ms measured); canvas colors must be scriptable functions, because `HxChart` leaves dataset colors alone.
- **Tests.** jsdom has no canvas: only pure builders (data, options, tooltips) are unit tested, chart components are verified in a real browser. A canvas shim would be needed to render a chart component in a unit test.
- **Canvas charts** need a table fallback for assistive technology (`hxChartFallback`) and have no selectable text; the spike did not test screen readers, the Tauri webviews (WebKitGTK, WebView2, WKWebView) or `color-mix()` on canvas there. These checks belong to #351.
- **Docs and samples.** The glossary, the demo build and the documentation site are unaffected; the output contract (JSON, snapshots) does not change.

## Follow-up

- #351 decides per chart (SVG or `HxChart`) using the spike recommendation.
- The migration PBI #351 covers the components, the shell, the removal of the budget warning and the removal of the `minimumReleaseAgeExclude` entry once the release is old enough.
- Close #229 when this ADR is merged.
