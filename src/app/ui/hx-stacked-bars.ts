import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  inject,
  input,
} from "@angular/core";
import { HxChart } from "@gravionlabs/helix-ui/chart";
import { compact } from "../core/format";
import type { BarRow } from "./stacked-bars";

/*
 * Spike (#350): the stacked bars of the skills view on helix chart (chart.js). Findings: .agents/350/findings.md.
 *
 * - HxChart passes `backgroundColor` of a dataset through untouched (it only fills it in when it is unset), so a
 *   CanvasPattern or a scriptable function works without a plugin.
 * - HxChart redraws on a theme change with the same `data` object (refresh() re-applies the input), so a ready-made
 *   pattern would keep the old color. The datasets therefore use scriptable colors: chart.js calls them on every
 *   update and the pattern is rebuilt when the resolved color differs. No extra code in HxChart is needed.
 */

/** The part of a canvas that the hatch needs; lets the tests pass a mock. */
export interface HatchCanvas {
  width: number;
  height: number;
  getContext(id: "2d"): HatchContext | null;
}
export interface HatchContext {
  fillStyle: unknown;
  strokeStyle: unknown;
  lineWidth: number;
  fillRect(x: number, y: number, w: number, h: number): void;
  beginPath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  stroke(): void;
  globalAlpha: number;
}

const TILE = 8;

/**
 * Diagonal hatch in `color`, like the SVG `<pattern>`: a faint background (35%) with solid 45 degree stripes.
 * `null` when no offscreen canvas is available.
 */
export function hatchPattern(
  target: { createPattern(image: never, repetition: string): unknown },
  color: string,
  makeCanvas: () => HatchCanvas = () => document.createElement("canvas") as unknown as HatchCanvas,
): unknown {
  const tile = makeCanvas();
  tile.width = TILE;
  tile.height = TILE;
  const g = tile.getContext("2d");
  if (!g) return null;
  g.fillStyle = color;
  g.globalAlpha = 0.35;
  g.fillRect(0, 0, TILE, TILE);
  g.globalAlpha = 1;
  g.strokeStyle = color;
  g.lineWidth = 3;
  // three diagonals so that the stripes meet at the tile edges
  for (const offset of [-TILE, 0, TILE]) {
    g.beginPath();
    g.moveTo(offset, TILE);
    g.lineTo(offset + TILE, 0);
    g.stroke();
  }
  return target.createPattern(tile as never, "repeat");
}

/** What the scriptable color gets from chart.js. */
export interface ColorContext {
  dataIndex: number;
  chart: { ctx: { createPattern(image: never, repetition: string): unknown } };
}

/** Resolves a CSS color or variable to a color for the canvas. */
export type Paint = (ctx: ColorContext, color: string, estimated: boolean) => unknown;

export type ChartData = {
  labels: string[];
  datasets: {
    label: string;
    data: number[];
    backgroundColor: (ctx: ColorContext) => unknown;
    borderColor: (ctx: ColorContext) => unknown;
    borderWidth: number;
    estimated: boolean[];
  }[];
};

/** Rows are bars, each segment name (baseline, load, carry) is a dataset; `estimated` is kept per bar. */
export function buildChartData(rows: BarRow[], paint: Paint): ChartData {
  const names = [...new Set(rows.flatMap((r) => r.segments.map((s) => s.label)))];
  return {
    labels: rows.map((r) => clip(r.label)),
    datasets: names.map((name) => {
      const segs = rows.map((r) => r.segments.find((s) => s.label === name));
      const color = segs.find((s) => s)?.color ?? "";
      return {
        label: name,
        data: segs.map((s) => s?.value ?? 0),
        estimated: segs.map((s) => s?.estimated ?? false),
        backgroundColor: (ctx) => paint(ctx, color, segs[ctx.dataIndex]?.estimated ?? false),
        borderColor: (ctx) => paint(ctx, color, false),
        borderWidth: 0.5,
      };
    }),
  };
}

/** Labels are clipped at 26 characters like in the SVG. */
export function clip(s: string): string {
  return s.length > 26 ? s.slice(0, 25) + "…" : s;
}

/** `2.1M`, with a tilde for estimates. */
export function valueText(value: number, estimated: boolean): string {
  return `${estimated ? "~" : ""}${compact(value)}`;
}

/** Tooltip line like the SVG `<title>`: `load: ~2.1M (estimated)`. */
export function tooltipLine(name: string, value: number, estimated: boolean): string {
  return `${name}: ${valueText(value, estimated)} (${estimated ? "estimated" : "measured"})`;
}

/** The rows of the table for screen readers: one row per bar, estimates with a tilde. */
export function fallbackRows(rows: BarRow[]): { label: string; cells: string[]; total: string }[] {
  return rows.map((r) => ({
    label: r.label,
    cells: r.segments.map((s) => `${s.label} ${valueText(s.value, s.estimated)}`),
    total: r.total,
  }));
}

/** Chart.js options: horizontal, stacked, compact tick labels, tooltip like the SVG. */
export function chartOptions(rows: BarRow[]): Record<string, unknown> {
  return {
    indexAxis: "y",
    animation: false,
    scales: {
      x: { stacked: true, ticks: { callback: (v: unknown) => compact(Number(v)) } },
      y: { stacked: true, ticks: { autoSkip: false } },
    },
    plugins: {
      legend: { display: false },
      tooltip: {
        callbacks: {
          title: (items: { dataIndex: number }[]) => rows[items[0]?.dataIndex ?? 0]?.label ?? "",
          label: (item: {
            dataset: { label: string; estimated: boolean[] };
            dataIndex: number;
            raw: unknown;
          }) =>
            tooltipLine(
              item.dataset.label,
              Number(item.raw),
              item.dataset.estimated[item.dataIndex] ?? false,
            ),
          footer: (items: { dataIndex: number }[]) => {
            const r = rows[items[0]?.dataIndex ?? 0];
            return r ? `total: ${r.total}` : "";
          },
        },
      },
    },
  };
}

/** Spike: same inputs as `StackedBars`, drawn by chart.js through helix chart. */
@Component({
  selector: "app-hx-stacked-bars",
  imports: [HxChart],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <hx-chart
      type="bar"
      [data]="data()"
      [options]="options()"
      [height]="height()"
      [ariaLabel]="label()"
    >
      <table hxChartFallback>
        <caption>
          {{
            label()
          }}
        </caption>
        <tbody>
          @for (r of table(); track r.label) {
            <tr>
              <th scope="row">{{ r.label }}</th>
              @for (c of r.cells; track $index) {
                <td>{{ c }}</td>
              }
              <td>total {{ r.total }}</td>
            </tr>
          }
        </tbody>
      </table>
    </hx-chart>
  `,
  styles: `
    :host {
      display: block;
      max-width: 1000px;
    }
  `,
})
export class HxStackedBars {
  readonly rows = input.required<BarRow[]>();
  readonly label = input("Bar chart");

  readonly #host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  readonly #cache = new Map<string, unknown>();

  protected readonly height = computed(() => `${this.rows().length * 30 + 60}px`);
  protected readonly table = computed(() => fallbackRows(this.rows()));
  protected readonly options = computed(() => chartOptions(this.rows()));
  protected readonly data = computed(() => buildChartData(this.rows(), this.#paint));

  /** Resolves `var(--c-load)` against the host on every call, so a theme change gives the new color. */
  readonly #paint: Paint = (ctx, color, estimated) => {
    const resolved = color.startsWith("var(")
      ? getComputedStyle(this.#host).getPropertyValue(color.slice(4, -1).trim()).trim()
      : color;
    if (!estimated || !resolved) return resolved;
    const cached = this.#cache.get(resolved);
    if (cached) return cached;
    const pattern = hatchPattern(ctx.chart.ctx, resolved) ?? resolved;
    this.#cache.set(resolved, pattern);
    return pattern;
  };
}
