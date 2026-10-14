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
import { createPaint, type ColorContext, type Paint } from "./hatch";
import type { BarRow } from "./stacked-bars";

/*
 * Spike (#350): the stacked bars of the skills view on helix chart (chart.js). Findings: docs/spikes/helix-chart-spike.md.
 *
 * - HxChart passes `backgroundColor` of a dataset through untouched (it only fills it in when it is unset), so a
 *   CanvasPattern or a scriptable function works without a plugin.
 * - HxChart redraws on a theme change with the same `data` object (refresh() re-applies the input), so a ready-made
 *   pattern would keep the old color. The datasets therefore use scriptable colors: chart.js calls them on every
 *   update and the pattern is rebuilt when the resolved color differs. No extra code in HxChart is needed.
 */

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
          @for (r of table(); track $index) {
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

  protected readonly height = computed(() => `${this.rows().length * 30 + 60}px`);
  protected readonly table = computed(() => fallbackRows(this.rows()));
  protected readonly options = computed(() => chartOptions(this.rows()));
  protected readonly data = computed(() => buildChartData(this.rows(), this.#paint));

  readonly #paint: Paint = createPaint(this.#host);
}
