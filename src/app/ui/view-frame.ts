import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { RouterOutlet } from "@angular/router";
import { LoadingService } from "../core/loading.service";
import { FilterBar } from "./filter-bar";

/** The page the views render in: the filter toolbar above the open view, busy while it loads. */
@Component({
  selector: "app-view-frame",
  imports: [RouterOutlet, FilterBar],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="toolbar" aria-label="Filter toolbar"><app-filter-bar /></section>
    <main [class.busy]="loading.busy()" [attr.aria-busy]="loading.busy()">
      <router-outlet />
    </main>
  `,
  styles: `
    :host {
      display: block;
      min-width: 0;
    }
    .toolbar {
      padding: 10px 12px;
      margin-bottom: 12px;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 6px;
    }
    main {
      min-width: 0;
      padding-bottom: 24px;
    }
  `,
})
export class ViewFrame {
  protected readonly loading = inject(LoadingService);
}
