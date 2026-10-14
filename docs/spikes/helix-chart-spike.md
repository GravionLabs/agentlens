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
| Tooltips (compact, tilde, `thinned_from` footer), table fallback | specs on the pure builders only; see Not verified |

The 1.2 s between navigating and the canvas appearing is the demo data path (two sequential sample calls with a 600 ms delay each in `ipc.ts`), not the chart.

## What it took

- Hatch: `src/app/ui/hatch.ts`, about 60 lines, shared by both charts. `HxChart` leaves dataset colors untouched, so a `CanvasPattern` works; because the theme redraw re-applies the same data object, the colors are scriptable functions and the pattern is rebuilt when the resolved CSS color changes.
- Timeline: one filled line dataset (linear x, `parsing: false`), one point-only dataset per marker kind (tooltip, point style) and an inline plugin `markerLines` of about 25 lines that draws the vertical marker lines. Axis, tooltip and fallback text reuse `compact`, `valueText` (tilde) and `markerLabel`/`markerX` of the SVG helpers. Thinning stays in Rust.
- Bundle, measured with `pnpm build:demo` (all js and css; gzip is `gzip -9` per file, summed):

  | State | Initial total raw | Initial est. transfer | All raw | All gzip |
  | --- | --- | --- | --- | --- |
  | #349 branch (4123adb4, local helix links 0.1.0/0.2.0) | 890.37 kB | 159.00 kB | 1,010,282 B | 234,308 B |
  | after #362 (ced2b9d5, published helix 22.2.7, chart.js loader) | 890.67 kB | 159.20 kB | 1,214,924 B | 304,394 B |
  | with both spike charts (this branch) | 895.18 kB | 160.49 kB | 1,228,545 B | 309,476 B |

  chart.js (bar and line only) is the lazy chunk `chart`: 204.34 kB raw, 60.85 kB transfer, not in the initial load; it makes up almost all of the +204,642 B raw / +70,086 B gzip between the first two rows. The whole spike costs +218,263 B raw / +75,168 B gzip overall and +4.8 kB raw (+1.5 kB transfer) of initial bundle. The second chart added no chart.js code. (The #349 row was measured again for this note and is 7 kB above the figure first recorded in #362; the later rows are unchanged.) The initial budget warning (500 kB) existed before.
- Dependencies: `@gravionlabs/helix-ui` and `@gravionlabs/helix-shell` are pinned to the published 22.2.7 and `chart.js` ^4.4.0 was added (peer of helix-ui). `pnpm-workspace.yaml` got a `minimumReleaseAgeExclude` entry for both helix packages at 22.2.7 (pnpm's release-age policy); remove it once the release is older than the minimum age. `pnpm test` and `pnpm build:demo` pass with the published packages.
- Canvas shim: none. The specs only test pure builders and the loader (chart.js registry), no canvas is created in jsdom; a shim would only be needed to render a chart component in a unit test.
- Hardening after review: `?hx=<n>` is capped at 5000 points; the hatch tile is drawn at the device pixel ratio and scaled back; an estimated value is never drawn like a measured one (no resolvable color gives a gray 45% fill, no offscreen canvas gives the color at 45%).

## Not verified

- Tooltip hover on the canvas: the tilde, compact numbers and the `thinned_from` footer are covered by specs on the builders only, not seen in a browser.
- Resize in the Tauri window: only a 420 px viewport in desktop Chrome was tried.
- `color-mix()` on a canvas on the oldest supported Tauri webview (WebKitGTK, WebView2, WKWebView): works in current Chrome only.
- High-DPI: checked once with an emulated device pixel ratio of 2 in headless Chrome (hatch stripes stay the same size and are sharp); no real high-DPI screen.
- Labels use `compact()` only; the page's token format switch (rounded/exact) is not applied, same as the SVG.
- Screen reader behavior of the `hxChartFallback` tables was not tested.

## Gaps and costs

- No total text at the end of a bar (tooltip footer instead); the bar chart height is set per row count by the component.
- Marker hover shares the nearest-x interaction with the line, so a marker and a point at the same x compete; the SVG has separate hit targets.
- Canvas text is not selectable, and the canvas needs the `hxChartFallback` table (a 400-row table here) for assistive technology; the SVG has native `<title>` tooltips.
- Page-level token format switches (rounded/exact) are not applied to chart labels, same as the SVG.
- Jsdom has no canvas: only the pure builders are unit tested, the components are verified in a real browser.

## Recommendation

Keep SVG for the stacked bars; migrate the timeline only if interaction (zoom, brushing, per-series legend toggles) is wanted. Reasoning: the bars are simple and static and the SVG already gives hatching, native tooltips and text for free, so helix chart adds 204 kB raw (61 kB transfer) of lazy chart.js code and a canvas fallback for no visible gain. The timeline is where chart.js pays off if the data grows or interaction is added: 400 points with all markers draw in about 20 ms and theme and resize redraws work without extra code. If the owner prefers one chart stack for consistency with the rest of helix, migrating both is feasible with the hatch helper and builders from this spike. Not recommended: migrating the timeline alone just for looks.
