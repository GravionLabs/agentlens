import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { RouterOutlet } from "@angular/router";
import { LoadingService } from "../core/loading.service";

/** The page the views render in: the open view, busy while it loads. The filters sit in the topbar. */
@Component({
  selector: "app-view-frame",
  imports: [RouterOutlet],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <main [class.busy]="loading.busy()" [attr.aria-busy]="loading.busy()">
      <router-outlet />
    </main>
  `,
  styles: `
    :host {
      display: block;
      min-width: 0;
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
