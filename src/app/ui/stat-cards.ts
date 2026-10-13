import { ChangeDetectionStrategy, Component, input } from "@angular/core";

export interface Stat {
  value: string;
  label: string;
  /** A tooltip, e.g. the exact count behind a rounded one. */
  title?: string;
}

@Component({
  selector: "app-stat-cards",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="cards">
      @for (s of stats(); track s.label) {
        <div class="card" [attr.title]="s.title ?? null">
          <div class="v">{{ s.value }}</div>
          <div class="l">{{ s.label }}</div>
        </div>
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
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 6px 12px;
    }
    .v {
      font-size: 1.25rem;
      font-weight: 650;
      font-variant-numeric: tabular-nums;
    }
    .l {
      color: var(--muted);
      font-size: 0.85rem;
    }
  `,
})
export class StatCards {
  readonly stats = input.required<Stat[]>();
}
