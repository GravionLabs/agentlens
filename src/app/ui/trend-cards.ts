import { ChangeDetectionStrategy, Component, input } from "@angular/core";
import { Status } from "./status";
import { type Stat, StatCards } from "./stat-cards";

/** The stat cards of the comparison with the previous period, shown while the filter bar switch is on. */
@Component({
  selector: "app-trend-cards",
  imports: [Status, StatCards],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section aria-label="Change against the previous period">
      <app-status [loading]="loading()" [error]="error()" [warnings]="0" [errors]="[]" />
      <app-stat-cards [stats]="stats()" [note]="note()" />
    </section>
  `,
})
export class TrendCards {
  readonly stats = input.required<Stat[]>();
  readonly note = input("");
  readonly loading = input(false);
  readonly error = input<unknown>(undefined);
}
