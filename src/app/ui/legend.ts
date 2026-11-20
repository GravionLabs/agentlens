import { ChangeDetectionStrategy, Component, input } from "@angular/core";

export interface LegendItem {
  label: string;
  color: string;
}

@Component({
  selector: "app-legend",
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./legend.html",
  styles: `
    .legend {
      display: flex;
      flex-wrap: wrap;
      gap: 6px 16px;
      margin: 8px 0;
      color: var(--muted);
      font-size: 0.85rem;
    }
    i {
      display: inline-block;
      width: 12px;
      height: 12px;
      border-radius: 2px;
      margin-right: 6px;
      vertical-align: -1px;
    }
  `,
})
export class Legend {
  readonly items = input.required<LegendItem[]>();
}
