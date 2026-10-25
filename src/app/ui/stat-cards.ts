import { ChangeDetectionStrategy, Component, input } from "@angular/core";
import { HelixStatCard } from "@gravionlabs/helix-shell";

export interface Stat {
  value: string;
  label: string;
  /** A tooltip, e.g. the exact count behind a rounded one. */
  title?: string;
}

/** Thin domain wrapper: lays out helix stat cards; values arrive pre-formatted (tilde = estimate, n/a = missing). */
@Component({
  selector: "app-stat-cards",
  imports: [HelixStatCard],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="cards">
      @for (s of stats(); track s.label) {
        <helix-stat-card [label]="s.label" [value]="s.value" [attr.title]="s.title ?? null" />
      }
    </div>
  `,
  styles: `
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
}
