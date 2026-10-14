import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  inject,
  input,
} from "@angular/core";
import { HxChart } from "@gravionlabs/helix-ui/chart";
import { compact, number, usd } from "../../core/format";
import type { MarkerKind, Timeline, TimelineMarker } from "../../core/types";
import { createPaint, resolveColor, type ColorContext, type Paint } from "../../ui/hatch";
import { valueText } from "../../ui/hx-stacked-bars";
import { MARKER_COLORS, markerLabel, markerX, type Scale } from "./chart";

/*
 * Spike (#350): the timeline area chart on helix chart (chart.js), type `line`. Findings: docs/spikes/helix-chart-spike.md.
 *
 * - Context size is one filled line dataset with linear x (request number) and `parsing: false`.
 * - Markers are one point-only dataset per kind (hover, tooltip, point style) plus the inline plugin `markerLines`
 *   that draws the dashed full-height line of each marker point (compaction: solid and thick), like the SVG.
 * - The fill and the hatch come from `ui/hatch.ts`, shared with the stacked bars.
 */

const KINDS: MarkerKind["type"][] = [
  "skill",
  "compaction",
  "tool-error",
  "hook-failed",
  "permission-prompt",
];
const KIND_NAMES: Record<MarkerKind["type"], string> = {
  skill: "skill loaded",
  compaction: "compaction",
  "tool-error": "tool error",
  "hook-failed": "hook failed",
  "permission-prompt": "permission prompt",
};
const POINT_STYLES: Record<MarkerKind["type"], string> = {
  skill: "triangle",
  compaction: "rectRot",
  "tool-error": "crossRot",
  "hook-failed": "rect",
  "permission-prompt": "circle",
};

/** x in request numbers: the SVG scale helpers with the identity as scale (display numbers are 1-based). */
const IDENTITY: Scale = { x: (r) => r, y: (t) => t, max: 1, last: 1 };

export interface XY {
  x: number;
  y: number;
}
export interface MarkerPoint extends XY {
  marker: TimelineMarker;
}

export type TimelineChartData = {
  datasets: Record<string, unknown>[];
};

/** The context line as `{x: request number, y: tokens}`. */
export function contextPoints(t: Timeline): XY[] {
  return t.points.map((p) => ({ x: p.request + 1, y: p.context_tokens }));
}

/** Markers of one kind at the gap before their request, on top of the plot (`top` = y of the points). */
export function markerPoints(t: Timeline, kind: MarkerKind["type"], top: number): MarkerPoint[] {
  const first = t.points.length > 0 ? t.points[0].request : 0;
  return t.markers
    .filter((m) => m.kind.type === kind)
    .map((m) => ({ x: markerX(m, IDENTITY, first) + 1, y: top, marker: m }));
}

/** Datasets: the filled context line, then one point dataset per marker kind that occurs. */
export function buildTimelineData(t: Timeline, paint: Paint, estimated = false): TimelineChartData {
  const peak = Math.max(1, ...t.points.map((p) => p.context_tokens));
  const area = {
    label: "context size",
    type: "line",
    data: contextPoints(t),
    parsing: false,
    normalized: true,
    fill: "origin",
    pointRadius: t.points.length > 120 ? 0 : 2.5,
    pointHoverRadius: 4,
    borderWidth: 1.5,
    tension: 0,
    order: 2,
    backgroundColor: (ctx: ColorContext) =>
      paint(
        ctx,
        estimated ? "var(--accent)" : "color-mix(in srgb, var(--accent) 18%, transparent)",
        estimated,
      ),
    borderColor: (ctx: ColorContext) => paint(ctx, "var(--accent)", false),
    pointBackgroundColor: (ctx: ColorContext) => paint(ctx, "var(--accent)", false),
  };
  const markers = KINDS.filter((k) => t.markers.some((m) => m.kind.type === k)).map((k) => ({
    label: KIND_NAMES[k],
    type: "line",
    kind: k,
    data: markerPoints(t, k, peak),
    parsing: false,
    showLine: false,
    pointStyle: POINT_STYLES[k],
    pointRadius: 5,
    pointHoverRadius: 7,
    order: 1,
    clip: false,
    backgroundColor: (ctx: ColorContext) => paint(ctx, MARKER_COLORS[k], false),
    borderColor: (ctx: ColorContext) => paint(ctx, MARKER_COLORS[k], false),
  }));
  return { datasets: [area, ...markers] };
}

/** Inline plugin: the vertical line of every marker point, drawn behind the datasets. */
export const markerLines = {
  id: "markerLines",
  beforeDatasetsDraw(chart: {
    ctx: CanvasRenderingContext2D;
    canvas: HTMLCanvasElement;
    chartArea: { top: number; bottom: number };
    data: { datasets: { kind?: MarkerKind["type"] }[] };
    getDatasetMeta(i: number): { data: { x: number }[] };
  }): void {
    const { ctx, chartArea } = chart;
    chart.data.datasets.forEach((ds, i) => {
      if (!ds.kind) return;
      const strong = ds.kind === "compaction";
      ctx.save();
      ctx.strokeStyle = resolveColor(chart.canvas, MARKER_COLORS[ds.kind]);
      ctx.globalAlpha = strong ? 1 : 0.55;
      ctx.lineWidth = strong ? 2.5 : 1;
      ctx.setLineDash(strong ? [] : [4, 3]);
      for (const el of chart.getDatasetMeta(i).data) {
        ctx.beginPath();
        ctx.moveTo(el.x, chartArea.top);
        ctx.lineTo(el.x, chartArea.bottom);
        ctx.stroke();
      }
      ctx.restore();
    });
  },
};

/** Tooltip title: `Request 12` or `Before request 12`. */
export function titleText(isMarker: boolean, x: number): string {
  return `${isMarker ? "Before request" : "Request"} ${number(Math.ceil(x))}`;
}

/** Lines for a context point: size, output and cost (tilde when the cost is estimated). */
export function pointLines(t: Timeline, index: number): string[] {
  const p = t.points[index];
  if (!p) return [];
  const lines = [
    `${valueText(p.context_tokens, false)} tokens in context (measured)`,
    `${valueText(p.output_tokens, false)} tokens out`,
  ];
  if (p.cost_usd !== null) lines.push(`cost ${usd(p.cost_usd, p.cost_estimated)}`);
  return lines;
}

/** `thinned from 2,000 to 400 points (peaks are kept)`, or null. */
export function thinnedText(t: Timeline): string | null {
  return t.thinned_from
    ? `thinned from ${number(t.thinned_from)} to ${number(t.points.length)} points (peaks are kept)`
    : null;
}

type Item = { dataset: { kind?: MarkerKind["type"] }; dataIndex: number; raw: unknown };

/** Chart.js options: linear x, compact y ticks, tooltips for points and markers. */
export function chartOptions(t: Timeline): Record<string, unknown> {
  const first = t.points.length > 0 ? t.points[0].request + 1 : 1;
  const last = Math.max(
    first + 1,
    t.points.length > 0 ? t.points[t.points.length - 1].request + 1 : 2,
  );
  return {
    animation: false,
    interaction: { mode: "nearest", axis: "x", intersect: false },
    layout: { padding: { top: 12 } },
    scales: {
      x: {
        type: "linear",
        min: first,
        max: last,
        title: { display: true, text: "request" },
        ticks: {
          callback: (v: unknown) => (Number.isInteger(v) ? number(Number(v)) : ""),
          maxTicksLimit: 8,
        },
      },
      y: {
        beginAtZero: true,
        grace: "5%",
        ticks: { callback: (v: unknown) => compact(Number(v)) },
      },
    },
    plugins: {
      legend: { display: false },
      tooltip: {
        callbacks: {
          title: (items: Item[]) =>
            titleText(!!items[0]?.dataset.kind, (items[0]?.raw as XY | undefined)?.x ?? 0),
          label: (item: Item) => {
            if (item.dataset.kind) {
              const m = (item.raw as MarkerPoint).marker;
              return markerLabel(m.kind);
            }
            return pointLines(t, item.dataIndex);
          },
          footer: () => thinnedText(t) ?? "",
        },
      },
    },
  };
}

/** The table for screen readers: one row per point. */
export function fallbackRows(t: Timeline): { request: string; cells: string[] }[] {
  return t.points.map((p) => ({
    request: number(p.request + 1),
    cells: [
      valueText(p.context_tokens, false),
      valueText(p.output_tokens, false),
      p.cost_usd === null ? "n/a" : usd(p.cost_usd, p.cost_estimated),
    ],
  }));
}

/** Fallback list of the markers. */
export function fallbackMarkers(t: Timeline): string[] {
  return t.markers.map((m) => `before request ${number(m.request + 1)}: ${markerLabel(m.kind)}`);
}

/** A synthetic timeline for the perf check (`?hx=400`): `n` points, a marker of every kind every few points. */
export function syntheticTimeline(base: Timeline, n: number): Timeline {
  const points = Array.from({ length: n }, (_, i) => ({
    request: i,
    at: null,
    context_tokens: Math.round(40_000 + ((i % 60) / 60) * 120_000 + (i % 7) * 3_000),
    output_tokens: 200 + (i % 13) * 40,
    cost_usd: (i % 5) / 100,
    cost_estimated: i % 2 === 0,
  }));
  const kinds: MarkerKind[] = [
    { type: "skill", name: "demo-skill", trigger: "model" },
    { type: "compaction" },
    { type: "tool-error", tool: "Bash" },
    { type: "hook-failed", hook: "pre-commit" },
    { type: "permission-prompt", wait_ms: 2400 },
  ];
  const markers: TimelineMarker[] = [];
  for (let r = 3; r < n; r += 9) {
    markers.push({ request: r, at: null, kind: kinds[((r / 9) % kinds.length) | 0] });
  }
  return {
    session: { ...base.session, requests: n * 5 },
    points,
    markers,
    thinned_from: n * 5,
  };
}

/** Spike: the timeline chart through helix chart. */
@Component({
  selector: "app-hx-timeline-chart",
  imports: [HxChart],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <hx-chart
      type="line"
      [data]="data()"
      [options]="options()"
      [plugins]="plugins"
      height="320px"
      ariaLabel="Context size per request"
    >
      <table hxChartFallback>
        <caption>
          Context size per request
        </caption>
        <thead>
          <tr>
            <th scope="col">Request</th>
            <th scope="col">Context tokens</th>
            <th scope="col">Output tokens</th>
            <th scope="col">Cost</th>
          </tr>
        </thead>
        <tbody>
          @for (r of table(); track $index) {
            <tr>
              <th scope="row">{{ r.request }}</th>
              @for (c of r.cells; track $index) {
                <td>{{ c }}</td>
              }
            </tr>
          }
        </tbody>
      </table>
      <ul hxChartFallback>
        @for (m of markerList(); track $index) {
          <li>{{ m }}</li>
        }
      </ul>
    </hx-chart>
  `,
  styles: `
    :host {
      display: block;
    }
    hx-chart {
      display: block;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 8px;
    }
  `,
})
export class HxTimelineChart {
  readonly timeline = input.required<Timeline>();

  readonly #host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  readonly #paint: Paint = createPaint(this.#host);

  protected readonly plugins = [markerLines];
  protected readonly data = computed(() => buildTimelineData(this.timeline(), this.#paint));
  protected readonly options = computed(() => chartOptions(this.timeline()));
  protected readonly table = computed(() => fallbackRows(this.timeline()));
  protected readonly markerList = computed(() => fallbackMarkers(this.timeline()));
}
