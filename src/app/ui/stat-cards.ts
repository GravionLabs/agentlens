import { ChangeDetectionStrategy, Component, input } from "@angular/core";
import { HelixStatCard } from "@gravionlabs/helix-shell";
import { type DeltaKind, type DeltaView, type Good, deltaValue, deltaView } from "../core/delta";
import type { Delta } from "../core/types";

export interface Stat {
  value: string;
  label: string;
  /** A tooltip, e.g. the exact count behind a rounded one. */
  title?: string;
  /** The change against the previous period, when the comparison is on. */
  delta?: DeltaView;
}

/** A stat card for a figure of the trend: the current value with its change. */
export function trendStat(
  label: string,
  d: Delta,
  kind: DeltaKind,
  opts: { good?: Good; perDay?: boolean } = {},
): Stat {
  const perDay = opts.perDay ?? false;
  return {
    label: perDay && kind !== "ratio" ? `${label} per day` : label,
    value: deltaValue(d.after, kind, !d.measured, perDay),
    delta: deltaView(d, kind, opts.good ?? null, perDay),
  };
}

/** Thin domain wrapper: lays out helix stat cards; values arrive pre-formatted (tilde = estimate, n/a = missing). */
@Component({
  selector: "app-stat-cards",
  imports: [HelixStatCard],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="cards">
      @for (s of stats(); track s.label) {
        <div class="card">
          <helix-stat-card [label]="s.label" [value]="s.value" [attr.title]="s.title ?? null" />
          @if (s.delta; as d) {
            <p
              class="delta"
              [class.good]="d.tone === 'good'"
              [class.bad]="d.tone === 'bad'"
              [attr.data-direction]="d.direction"
            >
              @if (d.arrow) {
                <span aria-hidden="true">{{ d.arrow }}</span>
              }
              {{ d.text }}
            </p>
          }
        </div>
      }
    </div>
    @if (note()) {
      <p class="note">{{ note() }}</p>
    }
  `,
  styles: `
    .delta {
      margin: 2px 8px 0;
      font-size: 0.85rem;
      font-variant-numeric: tabular-nums;
      color: var(--muted);
    }
    .delta.good {
      color: var(--good);
    }
    .delta.bad {
      color: var(--bad);
    }
    .note {
      margin: 0 0 8px;
      font-size: 0.85rem;
      color: var(--muted);
    }
    .cards {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
      gap: 8px;
      margin: 8px 0;
    }
  `,
})
export class StatCards {
  readonly stats = input.required<Stat[]>();
  /** A line under the cards, e.g. the comparison window. */
  readonly note = input("");
}
