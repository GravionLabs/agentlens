import { ChangeDetectionStrategy, Component, computed, input } from "@angular/core";
import { compact } from "../core/format";

export interface Segment {
  label: string;
  value: number;
  /** A CSS color or variable. */
  color: string;
  /** Estimated values are hatched, measured ones are solid. */
  estimated: boolean;
}

export interface BarRow {
  label: string;
  segments: Segment[];
  /** Text at the end of the bar. */
  total: string;
}

/** Horizontal stacked bars in SVG. */
@Component({
  selector: "app-stacked-bars",
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./stacked-bars.html",
  styles: `
    /* One unit is one pixel: the chart does not grow with the window, so the text keeps its size. */
    svg {
      display: block;
      width: 100%;
      max-width: 1000px;
      height: auto;
    }
    text {
      fill: var(--muted);
      font-size: 13px;
      font-family: inherit;
    }
    .lbl {
      fill: var(--text);
    }
  `,
})
export class StackedBars {
  readonly rows = input.required<BarRow[]>();
  readonly label = input("Bar chart");

  protected readonly width = 1000;
  protected readonly left = 190;
  protected readonly right = 150;
  protected readonly rowHeight = 30;
  protected readonly plot = this.width - this.left - this.right;

  protected readonly height = computed(() => this.rows().length * this.rowHeight + 8);

  protected readonly colors = computed(() => [
    ...new Set(this.rows().flatMap((r) => r.segments.map((s) => s.color))),
  ]);

  protected readonly bars = computed(() => {
    const max = Math.max(1, ...this.rows().map((r) => r.segments.reduce((a, s) => a + s.value, 0)));
    return this.rows().map((r) => {
      let x = this.left;
      const segs = r.segments.map((s) => {
        const w = (s.value / max) * this.plot;
        const seg = { ...s, x, w: w > 0 ? Math.max(w, 1) : 0 };
        x += w;
        return seg;
      });
      return { label: r.label, total: r.total, segs };
    });
  });

  protected patternId(color: string): string {
    return "hatch-" + color.replace(/[^a-z0-9]/gi, "");
  }

  protected clip(s: string): string {
    return s.length > 26 ? s.slice(0, 25) + "…" : s;
  }

  protected fmt(n: number): string {
    return compact(n);
  }
}
