# Spike: charts on helix chart (chart.js)

Issue #350. Two charts were rebuilt on `HxChart` of `@gravionlabs/helix-ui` 22.2.7 behind a query switch, next to the unchanged SVG versions: the stacked bars of the skills view (`#/skills?hx`) and the timeline area chart with markers (`#/timeline?hx`, `#/timeline?hx=400` for a synthetic 400-point session with all five marker kinds). Screenshots (light, dark, narrow, SVG reference) are in `.agents/350/screenshots` in the working copy; that directory is git-ignored and not committed.

## How it was checked

`pnpm build:demo` output served locally, driven by headless Google Chrome (via the DevTools protocol, 1280 px wide, device scale 1) with the demo sample data. Dark mode was switched with the emulated `prefers-color-scheme` media feature, the real path of the app. Numbers are from one machine and one run of five; they show the order of magnitude, not a benchmark.

## Results

| Check | Result |
| --- | --- |
| Skills bars render, estimated segments hatched, light and dark | yes; stripes visible, 365 of 365 sampled 8 px blocks of the canvas mixed two or more colors (no flat fill) |
| Theme redraw (scriptable pattern color) | yes; hatch and line colors follow the theme, about 90 ms from the media change to a changed canvas (includes the helix theme delay) |
| Timeline area, line and all five marker kinds, light and dark | yes; marker colors and point styles follow the theme; compaction solid and thick, others dashed |
| 400 points, all markers: canvas created to first drawn pixels | 16 to 22 ms (five runs), no long task (over 50 ms) recorded |
| Resize 1280 to 420 px | the canvas follows the container, redraw about 40 ms; at 420 px the page itself scrolls horizontally (the app sidebar, not the chart) |
| Tooltips (compact, tilde, `thinned_from` footer), table fallback | covered by specs on the pure builders; hover was not driven in the browser |

The 1.2 s between navigating and the canvas appearing is the demo data path (two sequential sample calls with a 600 ms delay each in `ipc.ts`), not the chart.

## What it took

- Hatch: `src/app/ui/hatch.ts`, about 60 lines, shared by both charts. `HxChart` leaves dataset colors untouched, so a `CanvasPattern` works; because the theme redraw re-applies the same data object, the colors are scriptable functions and the pattern is rebuilt when the resolved CSS color changes.
- Timeline: one filled line dataset (linear x, `parsing: false`), one point-only dataset per marker kind (tooltip, point style) and an inline plugin `markerLines` of about 25 lines that draws the vertical marker lines. Axis, tooltip and fallback text reuse `compact`, `valueText` (tilde) and `markerLabel`/`markerX` of the SVG helpers. Thinning stays in Rust.
- Bundle: chart.js is a lazy chunk (204 kB raw, 61 kB transfer, bar and line only); the second chart added no chart.js code. The initial bundle grew by about 4.5 kB against the state after #362 (890.67 to 895.18 kB).

## Gaps and costs

- No total text at the end of a bar (tooltip footer instead); the bar chart height is set per row count by the component.
- Marker hover shares the nearest-x interaction with the line, so a marker and a point at the same x compete; the SVG has separate hit targets.
- Canvas text is not selectable, and the canvas needs the `hxChartFallback` table (a 400-row table here) for assistive technology; the SVG has native `<title>` tooltips.
- The area fill uses `color-mix()` on a canvas; fine in Chromium and current WebKit, worth a check on the oldest supported Tauri webview.
- Page-level token format switches (rounded/exact) are not applied to chart labels, same as the SVG.
- Jsdom has no canvas: only the pure builders are unit tested, the components are verified in a real browser.

## Recommendation

Keep SVG for the stacked bars; migrate the timeline only if interaction (zoom, brushing, per-series legend toggles) is wanted. Reasoning: the bars are simple and static and the SVG already gives hatching, native tooltips and text for free, so helix chart adds 270 kB of lazy code and a canvas fallback for no visible gain. The timeline is where chart.js pays off if the data grows or interaction is added: 400 points with all markers draw in about 20 ms and theme and resize redraws work without extra code. If the owner prefers one chart stack for consistency with the rest of helix, migrating both is feasible with the hatch helper and builders from this spike. Not recommended: migrating the timeline alone just for looks.
