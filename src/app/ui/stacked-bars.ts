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
  template: `
    <svg [attr.viewBox]="'0 0 ' + width + ' ' + height()" role="img" [attr.aria-label]="label()">
      <defs>
        @for (c of colors(); track c) {
          <pattern
            [attr.id]="patternId(c)"
            width="6"
            height="6"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <rect width="6" height="6" [attr.fill]="c" opacity="0.35" />
            <rect width="3" height="6" [attr.fill]="c" />
          </pattern>
        }
      </defs>
      @for (r of bars(); track r.label; let i = $index) {
        <text [attr.x]="left - 8" [attr.y]="i * rowHeight + 20" text-anchor="end" class="lbl">
          {{ clip(r.label) }}
        </text>
        @for (s of r.segs; track $index) {
          @if (s.w > 0) {
            <rect
              [attr.x]="s.x"
              [attr.y]="i * rowHeight + 8"
              [attr.width]="s.w"
              height="16"
              [attr.fill]="s.estimated ? 'url(#' + patternId(s.color) + ')' : s.color"
              [attr.stroke]="s.color"
              stroke-width="0.5"
            >
              <title>
                {{ r.label }} – {{ s.label }}: {{ s.estimated ? "~" : "" }}{{ fmt(s.value) }} ({{
                  s.estimated ? "estimated" : "measured"
                }})
              </title>
            </rect>
          }
        }
        <text [attr.x]="left + plot + 8" [attr.y]="i * rowHeight + 20">{{ r.total }}</text>
      }
    </svg>
  `,
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
