import { ChangeDetectionStrategy, Component, input, output } from "@angular/core";
import { Status } from "./status";
import { type Stat, StatCards } from "./stat-cards";

/** The stat cards of the comparison with the previous period, shown while the filter bar switch is on. */
@Component({
  selector: "app-trend-cards",
  imports: [Status, StatCards],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./trend-cards.html",
})
export class TrendCards {
  readonly stats = input.required<Stat[]>();
  readonly note = input("");
  readonly loading = input(false);
  readonly error = input<unknown>(undefined);
  readonly retry = output<void>();
}
