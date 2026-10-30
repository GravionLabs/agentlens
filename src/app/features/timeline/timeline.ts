import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { compact, day, number, sourceLabel, timeOfDay, usd } from "../../core/format";
import type { SessionSummary, Timeline as TimelineData } from "../../core/types";
import { Legend } from "../../ui/legend";
import { Status } from "../../ui/status";
import {
  HEIGHT,
  MARGIN,
  MARKER_COLORS,
  WIDTH,
  areaPath,
  linePath,
  markerLabel,
  markerX,
  scales,
  yTicks,
} from "./chart";

const MARKER_LEGEND = [
  { label: "skill loaded", color: MARKER_COLORS.skill },
  { label: "compaction", color: MARKER_COLORS.compaction },
  { label: "tool error", color: MARKER_COLORS["tool-error"] },
  { label: "hook failed", color: MARKER_COLORS["hook-failed"] },
  { label: "permission prompt", color: MARKER_COLORS["permission-prompt"] },
  { label: "model switch", color: MARKER_COLORS["model-switch"] },
];

@Component({
  selector: "app-timeline",
  imports: [PageHeader, Term, Status, Legend],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header title="Timeline" summary="Context size of every request of one session.">
        <p>
          The <app-term id="context-size">context size</app-term> (input + cache, measured) of every
          main-agent request of one session, with what happened in between.
        </p>
      </app-page-header>
      <app-status
        [loading]="list.isLoading()"
        [error]="list.error()"
        [errors]="list.value()?.errors ?? []"
      />
      @if (sessions().length > 0) {
        <label class="pick">
          <span>Session</span>
          <select (change)="choose($event)">
            @for (s of sessions(); track key(s)) {
              <option [value]="key(s)" [selected]="key(s) === current()">{{ describe(s) }}</option>
            }
          </select>
        </label>
      } @else if (!list.isLoading()) {
        <p class="muted">No sessions with requests for the selected filters.</p>
      }

      <app-status [loading]="res.isLoading()" [error]="res.error()" />
      @if (timeline(); as t) {
        <p class="meta">
          {{ label(t.session.source) }} · {{ t.session.model ?? "unknown model" }} ·
          {{ n(t.session.requests) }} requests · peak {{ n(t.session.peak_tokens) }} tokens ·
          {{ day(t.session.started_at) }}
          @if (t.thinned_from) {
            · thinned from {{ n(t.thinned_from) }} to {{ n(t.points.length) }} points (peaks are
            kept)
          }
        </p>
        <app-legend [items]="markerLegend" />
        <svg [attr.viewBox]="'0 0 ' + W + ' ' + H" role="img" aria-label="Context size per request">
          @for (v of ticks(); track v) {
            <line
              class="grid"
              [attr.x1]="m.left"
              [attr.x2]="W - m.right"
              [attr.y1]="y(v)"
              [attr.y2]="y(v)"
            />
            <text [attr.x]="m.left - 6" [attr.y]="y(v) + 4" text-anchor="end">
              {{ compact(v) }}
            </text>
          }
          <path class="area" [attr.d]="area()" />
          <path class="line" [attr.d]="line()" />
          @for (mk of markers(); track $index) {
            <line
              class="marker"
              [class.strong]="mk.strong"
              [attr.x1]="mk.x"
              [attr.x2]="mk.x"
              [attr.y1]="m.top"
              [attr.y2]="H - m.bottom"
              [attr.stroke]="mk.color"
            >
              <title>{{ mk.text }}</title>
            </line>
          }
          @for (p of t.points; track p.request) {
            <circle [attr.cx]="x(p.request)" [attr.cy]="y(p.context_tokens)" r="2.5">
              <title>
                Request {{ p.request + 1 }}: {{ n(p.context_tokens) }} tokens in context,
                {{ n(p.output_tokens) }} out @if (p.model) { , {{ p.model }} } @if (p.cost_usd !==
                null) { , {{ usd(p.cost_usd, p.cost_estimated) }} } @if (p.at) { , {{ p.at }}
                }
              </title>
            </circle>
          }
          <text [attr.x]="m.left" [attr.y]="H - 8">request 1</text>
          <text [attr.x]="W - m.right" [attr.y]="H - 8" text-anchor="end">
            request {{ n(t.session.requests) }}
          </text>
        </svg>
        @if (t.markers.length > 0) {
          <h3>Events</h3>
          <ul class="events">
            @for (mk of t.markers; track $index) {
              <li>
                <span class="dot" [style.background]="color(mk.kind.type)"></span>
                before request {{ mk.request + 1 }}: {{ text(mk) }}
                @if (mk.at) {
                  <span class="muted" [title]="mk.at">{{ time(mk.at) }}</span>
                }
              </li>
            }
          </ul>
        }
      }
    </section>
  `,
  styles: `
    h3 {
      margin: 16px 0 4px;
      font-size: 1.05rem;
    }
    .pick {
      display: inline-flex;
      flex-direction: column;
      gap: 2px;
      color: var(--muted);
      font-size: 0.85rem;
      margin: 8px 0;
    }
    select {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 5px 8px;
      max-width: 100%;
    }
    .meta {
      color: var(--muted);
      margin: 8px 0 0;
    }
    svg {
      display: block;
      width: 100%;
      height: auto;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
    }
    text {
      fill: var(--muted);
      font-size: 11px;
      font-family: inherit;
    }
    .grid {
      stroke: var(--line);
    }
    .area {
      fill: color-mix(in srgb, var(--accent) 18%, transparent);
    }
    .line {
      fill: none;
      stroke: var(--accent);
      stroke-width: 1.5;
    }
    circle {
      fill: var(--accent);
    }
    .marker {
      stroke-width: 1;
      stroke-dasharray: 4 3;
      opacity: 0.55;
    }
    /* A compaction is the event that changes the picture: solid and thick. */
    .marker.strong {
      stroke-width: 2.5;
      stroke-dasharray: none;
      opacity: 1;
    }
    .events {
      list-style: none;
      padding: 0;
      margin: 0;
      columns: 2 380px;
    }
    .events li {
      padding: 2px 0;
      break-inside: avoid;
    }
    .dot {
      display: inline-block;
      width: 8px;
      height: 8px;
      border-radius: 50%;
      margin-right: 6px;
    }
  `,
})
export class Timeline {
  protected readonly list = analysis<SessionSummary[]>("sessions");
  protected readonly sessions = computed(() => this.list.value()?.data ?? []);
  private readonly picked = signal<string | null>(null);

  /** The chosen session, or the newest one. */
  protected readonly current = computed(() => {
    const all = this.sessions();
    const p = this.picked();
    if (p && all.some((s) => this.key(s) === p)) return p;
    return all.length > 0 ? this.key(all[0]) : null;
  });

  protected readonly res = analysis<TimelineData>(
    "timeline",
    () => {
      const [source, ...id] = (this.current() ?? "").split("|");
      return { source, id: id.join("|") };
    },
    () => this.current() !== null,
  );

  protected readonly timeline = computed(() =>
    this.current() ? (this.res.value()?.data ?? null) : null,
  );
  private readonly scale = computed(() => scales(this.timeline()?.points ?? []));

  protected readonly W = WIDTH;
  protected readonly H = HEIGHT;
  protected readonly m = MARGIN;
  protected readonly markerLegend = MARKER_LEGEND;
  protected readonly n = number;
  protected readonly day = day;
  protected readonly usd = usd;
  protected readonly compact = compact;
  protected readonly label = sourceLabel;
  protected readonly time = timeOfDay;

  protected readonly ticks = computed(() => yTicks(this.scale().max));
  protected readonly line = computed(() => linePath(this.timeline()?.points ?? [], this.scale()));
  protected readonly area = computed(() => areaPath(this.timeline()?.points ?? [], this.scale()));

  protected readonly markers = computed(() => {
    const t = this.timeline();
    if (!t) return [];
    const first = t.points.length > 0 ? t.points[0].request : 0;
    return t.markers.map((mk) => ({
      x: markerX(mk, this.scale(), first),
      color: MARKER_COLORS[mk.kind.type],
      strong: mk.kind.type === "compaction",
      text: markerLabel(mk.kind),
    }));
  });

  protected x(request: number): number {
    return this.scale().x(request);
  }

  protected y(tokens: number): number {
    return this.scale().y(tokens);
  }

  protected key(s: SessionSummary): string {
    return `${s.source}|${s.id}`;
  }

  protected describe(s: SessionSummary): string {
    const where = s.project
      ? s.project.split("/").filter(Boolean).slice(-2).join("/")
      : "no project";
    return `${day(s.ended_at)} · ${sourceLabel(s.source)} · ${where} · ${number(s.requests)} requests`;
  }

  protected choose(e: Event): void {
    this.picked.set((e.target as HTMLSelectElement).value);
  }

  protected color(type: keyof typeof MARKER_COLORS): string {
    return MARKER_COLORS[type];
  }

  protected text(mk: { kind: Parameters<typeof markerLabel>[0] }): string {
    return markerLabel(mk.kind);
  }
}
