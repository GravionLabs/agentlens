import { ChangeDetectionStrategy, Component, computed, input } from "@angular/core";
import { HxMessage, HxProgressSpinner } from "@gravionlabs/helix-ui";
import { messageOf } from "../core/analysis";

/** Loading, failure and the warnings of reading the sessions for one analysis. */
@Component({
  selector: "app-status",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxMessage, HxProgressSpinner],
  templateUrl: "./status.html",
  styles: `
    :host {
      display: block;
    }
    hx-message {
      display: block;
      margin: 8px 0;
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

  protected readonly failure = computed(() => {
    const e = this.error();
    return e === undefined || e === null ? null : messageOf(e);
  });
}
