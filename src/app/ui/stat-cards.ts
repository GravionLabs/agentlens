import { ChangeDetectionStrategy, Component, input } from "@angular/core";

export interface Stat {
  value: string;
  label: string;
}

@Component({
  selector: "app-stat-cards",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="cards">
      @for (s of stats(); track s.label) {
        <div class="card">
          <div class="v">{{ s.value }}</div>
          <div class="l">{{ s.label }}</div>
        </div>
      }
    </div>
  `,
  styles: `
    .cards {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
      gap: 12px;
      margin: 12px 0;
    }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 10px 14px;
    }
    .v {
      font-size: 1.4rem;
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
