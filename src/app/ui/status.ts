import { ChangeDetectionStrategy, Component, computed, input, output } from "@angular/core";
import { HxButton, HxMessage, HxProgressSpinner } from "@gravionlabs/helix-ui";
import { messageOf } from "../core/analysis";

/** Loading, failure and the warnings of reading the sessions for one analysis. */
@Component({
  selector: "app-status",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxButton, HxMessage, HxProgressSpinner],
  templateUrl: "./status.html",
  styles: `
    :host {
      display: block;
    }
    hx-message {
      display: block;
      margin: 8px 0;
    }
    .retry {
      margin-left: var(--h-space-2);
    }
    .spinner {
      display: inline-block;
      width: 14px;
      height: 14px;
      margin-right: 8px;
      vertical-align: middle;
    }
  `,
})
export class Status {
  readonly loading = input(false);
  readonly error = input<unknown>(undefined);
  readonly warnings = input(0);
  readonly errors = input<string[]>([]);
  /** *Try again* in the failure message: the view runs the failed analysis again. */
  readonly retry = output<void>();

  protected readonly failure = computed(() => {
    const e = this.error();
    return e === undefined || e === null ? null : messageOf(e);
  });
}
